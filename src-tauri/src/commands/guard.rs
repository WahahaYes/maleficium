//! Trust-boundary guards: input validation for IPC commands +
//! the runtime project-scope grant.
//!
//! Rule (design §5): custom Rust commands validate inputs themselves and
//! never trust the frontend. Validation lives server-side only — there is
//! deliberately no frontend mirror.
//!
//! Layout: pure/testable helpers (`reject_empty_nul`, `require_absolute`,
//! `is_bare_filename`, `require_repo_path`) + thin wrappers that touch the
//! fs (`canonical_root`, `require_allowed` — both canonicalize and fail
//! closed) + scope fetch (`live_scope`) + the one command that mints runtime
//! scope (`grant_project_access`).

use std::path::{Path, PathBuf};

use tauri::AppHandle;
use tauri_plugin_fs::FsExt;

/// Reject empty strings and anything containing a NUL byte. Every entry
/// point below starts here — NUL would truncate at C boundaries downstream.
pub fn reject_empty_nul(raw: &str) -> Result<&str, String> {
    if raw.is_empty() {
        return Err("forbidden path: empty".to_string());
    }
    if raw.contains('\0') {
        return Err("forbidden path: NUL byte".to_string());
    }
    Ok(raw)
}

/// Require an absolute path (pure — no fs access). Relative frontend strings
/// would resolve against an attacker-influenced CWD, so they never validate.
pub fn require_absolute(path: &Path, raw: &str) -> Result<(), String> {
    if !path.is_absolute() {
        return Err(format!("forbidden path (not absolute): {}", raw));
    }
    Ok(())
}

/// True when `name` is a bare filename: no `/`, `\`, or NUL, not
/// empty, not `.`/`..`. Pure — no fs access. Used wherever a frontend
/// string is interpolated into a tool argument rather than opened as a path
/// (e.g. the synctex `page:x:y:name` tag, `HEAD:<file>`).
///
/// Note: path separators are rejected but interior dots are fine —
/// `ch1..v2.tex` is a legal filename (`..` traverses only beside a
/// separator, and separators never survive this check).
pub fn is_bare_filename(name: &str) -> bool {
    if name.is_empty() || name.contains('\0') {
        return false;
    }
    if name == "." || name == ".." {
        return false;
    }
    if name.contains('/') || name.contains('\\') {
        return false;
    }
    true
}

/// Validate a candidate project root: non-empty, no NUL, absolute,
/// canonicalizable (fails closed on missing paths — symlinks resolved), and
/// a directory. Returns the canonical path, which callers adopt so later
/// `starts_with` checks compare canonical-vs-canonical.
pub fn canonical_root(raw: &str) -> Result<PathBuf, String> {
    reject_empty_nul(raw)?;
    let path = Path::new(raw);
    require_absolute(path, raw)?;
    let canon = path
        .canonicalize()
        .map_err(|e| format!("forbidden path (unresolvable): {}: {}", raw, e))?;
    if !canon.is_dir() {
        return Err(format!("forbidden path (not a directory): {}", raw));
    }
    Ok(canon)
}

/// Validate a bare filename (no fs access). See `is_bare_filename`.
pub fn require_bare_filename(name: &str) -> Result<&str, String> {
    if !is_bare_filename(name) {
        return Err(format!("forbidden name (not a bare filename): {}", name));
    }
    Ok(name)
}

/// Validate a `HEAD:<file>`-style repo-relative path: rejects empty, NUL,
/// absolute paths, and any `..` component (pure lexical check — no fs
/// access, so it works for files that exist only in HEAD, not on disk).
/// Windows separators are rejected too (this is a git-path, always `/`).
pub fn require_repo_path(file: &str) -> Result<&str, String> {
    reject_empty_nul(file)?;
    let p = Path::new(file);
    if p.is_absolute() {
        return Err(format!("forbidden path (not repo-relative): {}", file));
    }
    if file.contains('\\') {
        return Err(format!("forbidden path (backslash): {}", file));
    }
    if p.components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(format!("forbidden path (parent escape): {}", file));
    }
    Ok(file)
}

/// Fetch the live fs scope, or fail closed when unavailable (never panic —
/// `try_fs_scope` returns None outside a managed window context).
pub fn live_scope(app: &AppHandle) -> Result<tauri::fs::Scope, String> {
    app.try_fs_scope()
        .ok_or_else(|| "forbidden path: fs scope unavailable".to_string())
}

/// Validate that `candidate` canonicalizes to a path the LIVE fs scope
/// currently allows (runtime grant from `grant_project_access`, dialog
/// picks, or static capability homes). This is the enforcement behind
/// every custom command: the scope is the source of truth, so commands
/// accept exactly what the fs plugin itself would serve.
pub fn require_allowed(app: &AppHandle, candidate: &str) -> Result<PathBuf, String> {
    reject_empty_nul(candidate)?;
    let canon = Path::new(candidate)
        .canonicalize()
        .map_err(|e| format!("forbidden path (unresolvable): {}: {}", candidate, e))?;
    let scope = live_scope(app)?;
    if scope.is_allowed(&canon) {
        Ok(canon)
    } else {
        Err(format!("forbidden path (outside scope): {}", candidate))
    }
}

/// Mint a recursive runtime fs-scope grant for one validated project root
/// (design §4). Additive per session (design §7 Q2): previously opened roots
/// stay readable until quit; no forbid-on-close in v1.
///
/// The dialog-picked path also lands here unconditionally — the dialog grant
/// is idempotent, and one unconditional code path beats four special cases
/// (dialog pick, Open Recent, restore-on-launch, `?project=` preset).
///
/// Returns the canonical path string; the frontend adopts it as `root`.
#[tauri::command]
pub fn grant_project_access(app: AppHandle, root: String) -> Result<String, String> {
    let canon = canonical_root(&root)?;
    let scope = live_scope(&app)?;
    scope
        .allow_directory(&canon, true)
        .map_err(|e| format!("grant failed for {}: {}", canon.display(), e))?;
    Ok(canon.to_string_lossy().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Fresh canonical scratch dir per test (pid + name; cleaned first).
    fn scratch(name: &str) -> PathBuf {
        let base =
            std::env::temp_dir().join(format!("maleficium-guard-{}-{}", std::process::id(), name));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        base.canonicalize().unwrap()
    }

    #[test]
    fn rejects_empty_and_nul() {
        assert!(reject_empty_nul("").is_err());
        assert!(reject_empty_nul("a\0b").is_err());
        assert!(reject_empty_nul("/ok/path").is_ok());
        assert!(canonical_root("").is_err());
        assert!(canonical_root("/tmp/a\0b").is_err());
        assert!(require_bare_filename("a\0b").is_err());
    }

    #[test]
    fn requires_absolute_root() {
        assert!(canonical_root("relative/path").is_err());
        assert!(canonical_root("../up").is_err());
    }

    #[test]
    fn fails_closed_on_missing_path() {
        let missing =
            std::env::temp_dir().join(format!("maleficium-guard-missing-{}", std::process::id()));
        let _ = fs::remove_dir_all(&missing);
        assert!(canonical_root(&missing.to_string_lossy()).is_err());
    }

    #[test]
    fn requires_directory_for_roots() {
        let base = scratch("isdir");
        let file = base.join("f.tex");
        fs::write(&file, "x").unwrap();
        assert!(canonical_root(&file.to_string_lossy()).is_err());
        assert_eq!(
            canonical_root(&base.to_string_lossy()).unwrap(),
            base.canonicalize().unwrap()
        );
    }

    #[test]
    fn bare_filename_rule() {
        assert!(is_bare_filename("main.pdf"));
        assert!(is_bare_filename("ch 1 (final).tex"));
        assert!(!is_bare_filename(""));
        assert!(!is_bare_filename("."));
        assert!(!is_bare_filename(".."));
        assert!(!is_bare_filename("a/b"));
        assert!(!is_bare_filename("/abs"));
        assert!(!is_bare_filename("a\\b"));
        assert!(!is_bare_filename("../x"));
        assert!(is_bare_filename("a..b"));
        assert!(!is_bare_filename("a\0b"));
        assert!(require_bare_filename("ok.tex").is_ok());
        assert!(require_bare_filename("a/b").is_err());
    }

    #[test]
    fn repo_path_rule() {
        assert!(require_repo_path("chapters/method.tex").is_ok());
        assert!(require_repo_path("main.tex").is_ok());
        // Works for HEAD-only paths (no fs access — missing file still validates).
        assert!(require_repo_path("deleted-in-workdir.tex").is_ok());
        assert!(require_repo_path("").is_err());
        assert!(require_repo_path("a\0b").is_err());
        assert!(require_repo_path("/abs/path.tex").is_err());
        assert!(require_repo_path("../outside.tex").is_err());
        assert!(require_repo_path("sub/../../outside.tex").is_err());
        assert!(require_repo_path("sub\\win.tex").is_err());
    }
}
