//! Trust-boundary guards (Slice A): input validation for IPC commands +
//! the runtime project-scope grant.
//!
//! Rule (design §5): custom Rust commands validate inputs themselves and
//! never trust the frontend. Validation lives server-side only — there is
//! deliberately no frontend mirror.
//!
//! Layout: pure/testable helpers (`reject_empty_nul`, `require_absolute`,
//! `is_within`, `is_bare_filename`) + thin wrappers that touch the fs
//! (`canonical_root`, `require_within` — both canonicalize and fail closed)
//! + the one command that mints runtime scope (`grant_project_access`).

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

/// Pure containment: true when canonical `path` equals `base` or sits under
/// it. Both sides MUST already be canonicalized (symlinks resolved) by the
/// caller — `Path::starts_with` compares components, so a raw
/// `/base/../evil` would otherwise prefix-match. See `require_within`.
///
/// Reserved for Slice B/C command validation (design §5); the grant path in
/// this slice needs only `canonical_root`.
// Note: `mod commands` is private, so `pub` items read as dead until the
// validation call sites land — the allow is intentional, not legacy.
#[allow(dead_code)]
pub fn is_within(canonical_path: &Path, canonical_base: &Path) -> bool {
    canonical_path.starts_with(canonical_base)
}

/// True when `name` is a bare filename: no `/`, `\`, or NUL, not
/// empty, not `.`/`..`. Pure — no fs access. Used wherever a frontend
/// string is interpolated into a tool argument rather than opened as a path
/// (e.g. the synctex `page:x:y:name` tag, `HEAD:<file>`).
///
/// Note: path separators are rejected but interior dots are fine —
/// `ch1..v2.tex` is a legal filename (`..` traverses only beside a
/// separator, and separators never survive this check).
///
/// Reserved for Slice B/C command validation (design §5).
#[allow(dead_code)]
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

/// Validate that `candidate` canonicalizes to a path inside canonical `base`.
/// Fails closed on unresolvable paths AND on containment failure.
///
/// Reserved for Slice B/C command validation (design §5).
#[allow(dead_code)]
pub fn require_within(candidate: &str, canonical_base: &Path) -> Result<PathBuf, String> {
    reject_empty_nul(candidate)?;
    let canon = Path::new(candidate)
        .canonicalize()
        .map_err(|e| format!("forbidden path (unresolvable): {}: {}", candidate, e))?;
    if !is_within(&canon, canonical_base) {
        return Err(format!("forbidden path (outside scope): {}", candidate));
    }
    Ok(canon)
}

/// Validate a bare filename (no fs access). See `is_bare_filename`.
///
/// Reserved for Slice B/C command validation (design §5).
#[allow(dead_code)]
pub fn require_bare_filename(name: &str) -> Result<&str, String> {
    if !is_bare_filename(name) {
        return Err(format!("forbidden name (not a bare filename): {}", name));
    }
    Ok(name)
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
    let scope = app
        .try_fs_scope()
        .ok_or_else(|| "forbidden path: fs scope unavailable".to_string())?;
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
        let missing = std::env::temp_dir().join(format!(
            "maleficium-guard-missing-{}",
            std::process::id()
        ));
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
    fn normalization_collapses_dotdot_inside_scope() {
        let base = scratch("norm");
        let sub = base.join("sub");
        fs::create_dir_all(&sub).unwrap();
        let tricky = format!("{}/sub/../sub", base.to_string_lossy());
        assert_eq!(require_within(&tricky, &base).unwrap(), sub.canonicalize().unwrap());
    }

    #[test]
    fn dotdot_escape_rejected() {
        let outer = scratch("escape");
        let base = outer.join("proj");
        let outside = outer.join("outside");
        fs::create_dir_all(&base).unwrap();
        fs::create_dir_all(&outside).unwrap();
        let base = base.canonicalize().unwrap();
        // Exists (canonicalize succeeds) but outside base → containment error.
        let evil = format!("{}/../outside", base.to_string_lossy());
        let err = require_within(&evil, &base).unwrap_err();
        assert!(err.contains("outside scope"), "unexpected: {}", err);
    }

    #[test]
    fn absolute_escape_rejected() {
        let base = scratch("abs");
        let sibling = base
            .parent()
            .unwrap()
            .join(format!("maleficium-guard-abs-sib-{}", std::process::id()));
        fs::create_dir_all(&sibling).unwrap();
        let err = require_within(&sibling.to_string_lossy(), &base).unwrap_err();
        assert!(err.contains("outside scope"), "unexpected: {}", err);
        let _ = fs::remove_dir_all(&sibling);
    }

    #[test]
    fn within_allows_equal_and_prefix_trick_fails() {
        // Pure component comparison: "/srv/proj2" is NOT within "/srv/proj".
        assert!(is_within(Path::new("/srv/proj"), Path::new("/srv/proj")));
        assert!(is_within(Path::new("/srv/proj/a/b"), Path::new("/srv/proj")));
        assert!(!is_within(Path::new("/srv/proj2"), Path::new("/srv/proj")));
        assert!(!is_within(Path::new("/srv"), Path::new("/srv/proj")));
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

    #[cfg(unix)]
    #[test]
    fn symlink_escape_collapses_to_target() {
        use std::os::unix::fs::symlink;
        let outer = scratch("symlink");
        let base = outer.join("proj");
        let outside = outer.join("outside");
        fs::create_dir_all(&base).unwrap();
        fs::create_dir_all(&outside).unwrap();
        let base = base.canonicalize().unwrap();
        symlink(&outside, base.join("link")).unwrap();
        // The link resolves outside base → rejected even though the raw
        // string prefix-matches base.
        let evil = format!("{}/link", base.to_string_lossy());
        assert!(require_within(&evil, &base).is_err());
    }
}
