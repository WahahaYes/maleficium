//! Input validation for IPC commands + the runtime project-scope grant.
//!
//! Commands validate inputs themselves server-side and never trust the
//! caller. There is no client-side mirror of these checks.

use std::path::{Path, PathBuf};

use tauri::AppHandle;
use tauri_plugin_fs::FsExt;

/// Reject empty strings and anything containing a NUL byte. Every check
/// below starts here — NUL would truncate at C boundaries downstream.
pub fn reject_empty_nul(raw: &str) -> Result<&str, String> {
    if raw.is_empty() {
        return Err("forbidden path: empty".to_string());
    }
    if raw.contains('\0') {
        return Err("forbidden path: NUL byte".to_string());
    }
    Ok(raw)
}

/// Require an absolute path (pure — no fs access). Relative strings
/// would resolve against an attacker-influenced CWD, so they never validate.
pub fn require_absolute(path: &Path, raw: &str) -> Result<(), String> {
    if !path.is_absolute() {
        return Err(format!("forbidden path (not absolute): {}", raw));
    }
    Ok(())
}

/// True when `name` is a bare filename: no `/`, `\`, or NUL, not
/// empty, not `.`/`..`. Pure — no fs access. For strings interpolated
/// into a tool argument rather than opened as a path.
/// Path separators are rejected but interior dots are fine.
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
/// a directory. Returns the canonical path.
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

/// Validate a bare filename (no fs access).
pub fn require_bare_filename(name: &str) -> Result<&str, String> {
    if !is_bare_filename(name) {
        return Err(format!("forbidden name (not a bare filename): {}", name));
    }
    Ok(name)
}

/// Fetch the live fs scope, or fail closed when unavailable (never panic —
/// `try_fs_scope` returns None outside a managed window context).
pub fn live_scope(app: &AppHandle) -> Result<tauri::fs::Scope, String> {
    app.try_fs_scope()
        .ok_or_else(|| "forbidden path: fs scope unavailable".to_string())
}

/// Validate that `candidate` canonicalizes to a path the live fs scope
/// currently allows. The scope is the source of truth: exactly what the
/// fs plugin itself would serve is accepted.
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

/// A granted project: its canonical path and the session-root id that the
/// compile and SyncTeX commands take.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectGrant {
    pub path: String,
    pub root_id: String,
}

/// Open a registered root in the live fs scope, recursively.
fn allow_granted(app: &AppHandle, canon: PathBuf, root_id: String) -> Result<ProjectGrant, String> {
    live_scope(app)?
        .allow_directory(&canon, true)
        .map_err(|e| format!("grant failed for {}: {}", canon.display(), e))?;
    Ok(ProjectGrant {
        path: canon.to_string_lossy().to_string(),
        root_id,
    })
}

/// Mint a recursive runtime fs-scope grant for one validated project root and
/// register it as a session root. Granted roots stay readable until quit. One
/// unconditional code path for every open route (dialog pick, recent,
/// restore, preset).
#[tauri::command]
pub fn grant_project_access(app: AppHandle, root: String) -> Result<ProjectGrant, String> {
    let (canon, root_id) = crate::core::grant_project(&root)?;
    allow_granted(&app, canon, root_id)
}

/// Grant the backend-owned scratch root that untitled documents compile in.
#[tauri::command]
pub fn grant_untitled_access(app: AppHandle) -> Result<ProjectGrant, String> {
    let (canon, root_id) = crate::core::grant_untitled()?;
    allow_granted(&app, canon, root_id)
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
}
