//! Input validation shared by every adapter. Callers never trust their
//! input; there is no client-side mirror of these checks.

use std::path::{Path, PathBuf};

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
    let canon = dunce::canonicalize(path)
        .map_err(|e| format!("forbidden path (unresolvable): {}: {}", raw, e))?;
    if !canon.is_dir() {
        return Err(format!("forbidden path (not a directory): {}", raw));
    }
    Ok(canon)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Fresh canonical scratch dir per test (pid + name; cleaned first).
    fn scratch(name: &str) -> PathBuf {
        let base = crate::test_scratch::dir(&format!("guard-{}", name));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        dunce::canonicalize(&base).unwrap()
    }

    #[test]
    fn rejects_empty_and_nul() {
        assert!(reject_empty_nul("").is_err());
        assert!(reject_empty_nul("a\0b").is_err());
        assert!(reject_empty_nul("/ok/path").is_ok());
        assert!(canonical_root("").is_err());
        assert!(canonical_root("/tmp/a\0b").is_err());
    }

    #[test]
    fn requires_absolute_root() {
        assert!(canonical_root("relative/path").is_err());
        assert!(canonical_root("../up").is_err());
    }

    #[test]
    fn fails_closed_on_missing_path() {
        let missing = crate::test_scratch::dir("guard-missing");
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
            dunce::canonicalize(&base).unwrap()
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
    }
}
