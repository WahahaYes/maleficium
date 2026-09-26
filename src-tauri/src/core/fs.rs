//! Filesystem operations against explicit session roots. Every op resolves
//! its path against a granted root and rejects escapes before touching
//! the fs.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use super::{hash_root, is_hidden_name, FileEntry};

/// Granted roots, keyed by session id. Roots stay readable until quit.
static ROOTS: OnceLock<Mutex<HashMap<String, PathBuf>>> = OnceLock::new();

fn roots() -> &'static Mutex<HashMap<String, PathBuf>> {
    ROOTS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn validate_root_id(id: &str) -> Result<&str, String> {
    crate::commands::guard::reject_empty_nul(id)?;
    if id.len() > 64 || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
        return Err("forbidden root id (alphanumeric/dash, max 64)".to_string());
    }
    Ok(id)
}

/// Mint a session root: validate the directory, canonicalize it, store it.
pub fn grant_root(id: &str, root: &str) -> Result<PathBuf, String> {
    validate_root_id(id)?;
    let canon = crate::commands::guard::canonical_root(root)?;
    roots()
        .lock()
        .map_err(|_| "roots lock poisoned".to_string())?
        .insert(id.to_string(), canon.clone());
    Ok(canon)
}

/// Register a project root under its own id (djb2 of the canonical path).
/// Returns the canonical root and the id that names it across the seam.
pub fn grant_project(root: &str) -> Result<(PathBuf, String), String> {
    let canon = crate::commands::guard::canonical_root(root)?;
    let id = hash_root(&canon.to_string_lossy());
    grant_root(&id, &canon.to_string_lossy())?;
    Ok((canon, id))
}

/// Create the untitled scratch dir if needed and register it like a project.
pub fn grant_untitled() -> Result<(PathBuf, String), String> {
    let dir = super::untitled_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("scratch dir unavailable: {}", e))?;
    grant_project(&dir.to_string_lossy())
}

/// Look up a session root by id.
pub fn session_root(id: &str) -> Result<PathBuf, String> {
    validate_root_id(id)?;
    roots()
        .lock()
        .map_err(|_| "roots lock poisoned".to_string())?
        .get(id)
        .cloned()
        .ok_or_else(|| format!("unknown project root: {}", id))
}

/// Resolve `candidate` inside the session root: rejects NUL, requires the
/// canonicalized path to sit under the granted root.
pub fn resolve_in(id: &str, candidate: &str) -> Result<PathBuf, String> {
    crate::commands::guard::reject_empty_nul(candidate)?;
    let root = session_root(id)?;
    let joined = if Path::new(candidate).is_absolute() {
        PathBuf::from(candidate)
    } else {
        root.join(candidate)
    };
    let canon = dunce::canonicalize(&joined)
        .map_err(|e| format!("forbidden path (unresolvable): {}: {}", candidate, e))?;
    if !canon.starts_with(&root) {
        return Err(format!("forbidden path (outside project): {}", candidate));
    }
    Ok(canon)
}

/// Resolve `candidate` for reading: an existing file must resolve inside the
/// root; a missing path resolves lexically (no `..` escape past the root).
pub fn resolve_read(id: &str, candidate: &str) -> Result<PathBuf, String> {
    if let Ok(p) = resolve_in(id, candidate) {
        return Ok(p);
    }
    crate::commands::guard::reject_empty_nul(candidate)?;
    let root = session_root(id)?;
    if candidate.contains('\0') {
        return Err("forbidden path: NUL byte".to_string());
    }
    if Path::new(candidate).is_absolute() {
        return Err(format!("forbidden path (outside project): {}", candidate));
    }
    let mut depth: i32 = 0;
    for comp in Path::new(candidate).components() {
        match comp {
            std::path::Component::ParentDir => depth -= 1,
            std::path::Component::CurDir => {}
            std::path::Component::Normal(_) => depth += 1,
            _ => return Err(format!("forbidden path (outside project): {}", candidate)),
        }
        if depth < 0 {
            return Err(format!("forbidden path (parent escape): {}", candidate));
        }
    }
    Ok(root.join(candidate))
}

/// App-scoped trash home for one root: the same dir the app's undo reads
/// (`appTrashDir(appDataDir(), root)` in `src/lib/paths.ts`).
fn trash_home(root: &Path) -> PathBuf {
    super::data_base_dir()
        .join("maleficium-trash")
        .join(hash_root(&root.to_string_lossy()))
}

/// Escape one path component so `__` in the entry name is only ever a
/// separator: `%` -> `%25`, then `_` -> `%5F`. Mirrored by `escapeComponent`
/// in `src/lib/file-history.ts`.
fn escape_component(s: &str) -> String {
    s.replace('%', "%25").replace('_', "%5F")
}

fn unescape_component(s: &str) -> Option<String> {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find(['%', '_']) {
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        let (ch, t) = if let Some(t) = tail.strip_prefix("%5F") {
            ('_', t)
        } else {
            ('%', tail.strip_prefix("%25")?)
        };
        out.push(ch);
        rest = t;
    }
    out.push_str(rest);
    Some(out)
}

/// `<base>__<c1>__<c2>...__<ms>`, every component escaped.
fn trash_name(original: &Path, rel: &str) -> String {
    let base = original
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "file".to_string());
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let flat_rel: Vec<String> = rel.split('/').map(escape_component).collect();
    format!(
        "{}__{}__{}",
        escape_component(&base),
        flat_rel.join("__"),
        stamp
    )
}

fn split_trash_name(name: &str) -> Option<(String, String)> {
    let parts: Vec<&str> = name.split("__").collect();
    if parts.len() < 3 {
        return None;
    }
    let stamp = parts[parts.len() - 1];
    if stamp.is_empty() || !stamp.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let base = unescape_component(parts[0])?;
    let comps = parts[1..parts.len() - 1]
        .iter()
        .map(|c| unescape_component(c))
        .collect::<Option<Vec<String>>>()?;
    if comps.last() != Some(&base) {
        return None;
    }
    let rel = comps.join("/");
    if !crate::commands::guard::is_bare_filename(&base)
        || comps.iter().any(|c| c.is_empty())
        || rel.contains('\0')
    {
        return None;
    }
    Some((base, rel))
}

/// Move a project file to the app-local trash home. Returns the trash path.
pub fn trash_file(id: &str, rel: &str, confirm: &str) -> Result<String, String> {
    let abs = resolve_in(id, rel)?;
    if confirm != abs.to_string_lossy() {
        return Err(format!(
            "confirmation mismatch: pass the file path back as confirm to delete {}",
            abs.display()
        ));
    }
    if !abs.is_file() {
        return Err(format!("not a file: {}", rel));
    }
    let root = session_root(id)?;
    let home = trash_home(&root);
    std::fs::create_dir_all(&home).map_err(|e| format!("trash home unreachable: {}", e))?;
    let dest = home.join(trash_name(&abs, rel));
    match std::fs::rename(&abs, &dest) {
        Ok(()) => Ok(dest.to_string_lossy().to_string()),
        Err(_) => {
            let bytes = std::fs::read(&abs).map_err(|e| format!("trash copy failed: {}", e))?;
            std::fs::write(&dest, &bytes).map_err(|e| format!("trash copy failed: {}", e))?;
            std::fs::remove_file(&abs).map_err(|e| format!("trash copy failed: {}", e))?;
            Ok(dest.to_string_lossy().to_string())
        }
    }
}

/// Restore a trashed file to its original path.
pub fn undo_trash(id: &str, trash_path: &str) -> Result<String, String> {
    crate::commands::guard::reject_empty_nul(trash_path)?;
    let root = session_root(id)?;
    let home = trash_home(&root);
    let src = PathBuf::from(trash_path);
    let canon_src = dunce::canonicalize(&src)
        .map_err(|e| format!("forbidden path (unresolvable): {}: {}", trash_path, e))?;
    if !canon_src.starts_with(&home) {
        return Err("forbidden path (not in trash)".to_string());
    }
    let name = canon_src
        .file_name()
        .ok_or_else(|| "forbidden path (no file name)".to_string())?
        .to_string_lossy();
    let (_base, rel) =
        split_trash_name(&name).ok_or_else(|| "forbidden path (not a trash entry)".to_string())?;
    let dest = resolve_read(id, &rel)?;
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("restore failed: {}", e))?;
    }
    match std::fs::rename(&canon_src, &dest) {
        Ok(()) => Ok(dest.to_string_lossy().to_string()),
        Err(_) => {
            let bytes = std::fs::read(&canon_src).map_err(|e| format!("restore failed: {}", e))?;
            std::fs::write(&dest, &bytes).map_err(|e| format!("restore failed: {}", e))?;
            std::fs::remove_file(&canon_src).map_err(|e| format!("restore failed: {}", e))?;
            Ok(dest.to_string_lossy().to_string())
        }
    }
}

/// List one directory level, sorted dirs-first. Hidden/build names skipped.
pub fn list_dir(id: &str, rel: &str) -> Result<Vec<FileEntry>, String> {
    let dir = if rel.is_empty() || rel == "." {
        session_root(id)?
    } else {
        resolve_read(id, rel)?
    };
    if !dir.is_dir() {
        return Err(format!("not a directory: {}", rel));
    }
    let mut out = Vec::new();
    let entries = std::fs::read_dir(&dir).map_err(|e| format!("list failed: {}", e))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("list failed: {}", e))?;
        let name = entry.file_name().to_string_lossy().to_string();
        if is_hidden_name(&name) {
            continue;
        }
        let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
        let rel_path = name.clone();
        out.push(FileEntry {
            name: rel_path,
            entry_type: if is_dir {
                "dir".to_string()
            } else {
                "file".to_string()
            },
        });
    }
    out.sort_by(|a, b| {
        let ad = i32::from(a.entry_type == "dir");
        let bd = i32::from(b.entry_type == "dir");
        bd.cmp(&ad).then_with(|| {
            a.name
                .to_lowercase()
                .cmp(&b.name.to_lowercase())
                .then_with(|| a.name.cmp(&b.name))
        })
    });
    Ok(out)
}

/// Read a project file as UTF-8 text.
pub fn read_text(id: &str, rel: &str) -> Result<String, String> {
    let abs = resolve_in(id, rel)?;
    std::fs::read_to_string(&abs).map_err(|e| format!("read failed: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grant_tmp(name: &str) -> (String, PathBuf) {
        let dir = crate::test_scratch::dir(&format!("fs-{}", name));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let canon = dunce::canonicalize(&dir).unwrap();
        let id = format!("t-{}", name);
        grant_root(&id, &canon.to_string_lossy()).unwrap();
        (id, canon)
    }

    #[test]
    fn grant_project_mints_byte_hash_id() {
        let dir = crate::test_scratch::dir("fs-proj-ü");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let (canon, id) = grant_project(&dir.to_string_lossy()).unwrap();
        assert_eq!(id, hash_root(&canon.to_string_lossy()));
        assert_eq!(session_root(&id).unwrap(), canon);
        std::fs::write(canon.join("main.tex"), "x").unwrap();
        assert!(resolve_in(&id, "main.tex").is_ok());
        assert!(resolve_in(&id, "../x").is_err());
        assert!(grant_project("relative/dir").is_err());
    }

    #[test]
    fn grant_rejects_bad_ids_and_paths() {
        assert!(grant_root("", "/tmp").is_err());
        assert!(grant_root("a/b", "/tmp").is_err());
        assert!(grant_root("ok-1", "relative/path").is_err());
        assert!(grant_root("ok-2", "/tmp/a\0b").is_err());
    }

    #[test]
    fn resolve_blocks_escape() {
        let (id, _dir) = grant_tmp("escape");
        assert!(resolve_in(&id, "../outside.tex").is_err());
        assert!(resolve_in(&id, "/etc/hostname").is_err());
        assert!(resolve_in(&id, "").is_err());
        assert!(resolve_in(&id, "a\0b").is_err());
    }

    #[test]
    fn trash_requires_matching_confirm() {
        let (id, dir) = grant_tmp("confirm");
        std::fs::write(dir.join("a.tex"), "hi").unwrap();
        assert!(trash_file(&id, "a.tex", "wrong").is_err());
        assert!(dir.join("a.tex").exists());
    }

    #[test]
    fn trash_round_trip_restores_bytes() {
        let (id, dir) = grant_tmp("roundtrip");
        std::fs::write(dir.join("a.tex"), "hello").unwrap();
        let abs = dir.join("a.tex").to_string_lossy().to_string();
        let trashed = trash_file(&id, "a.tex", &abs).unwrap();
        assert!(!dir.join("a.tex").exists());
        let back = undo_trash(&id, &trashed).unwrap();
        assert_eq!(back, abs);
        assert_eq!(std::fs::read_to_string(dir.join("a.tex")).unwrap(), "hello");
    }

    #[test]
    fn mcp_delete_lands_in_app_trash_dir() {
        let (id, dir) = grant_tmp("apptrash");
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("sub/a.tex"), "x").unwrap();
        let abs = dir.join("sub/a.tex").to_string_lossy().to_string();
        let trashed = PathBuf::from(trash_file(&id, "sub/a.tex", &abs).unwrap());
        let app_dir = super::super::data_base_dir()
            .join("maleficium-trash")
            .join(hash_root(&dir.to_string_lossy()));
        assert_eq!(trashed.parent().unwrap(), app_dir);
        let name = trashed.file_name().unwrap().to_string_lossy().to_string();
        let stamp = name.strip_prefix("a.tex__sub__a.tex__").unwrap();
        assert!(!stamp.is_empty() && stamp.bytes().all(|b| b.is_ascii_digit()));
        undo_trash(&id, &trashed.to_string_lossy()).unwrap();
        assert!(dir.join("sub/a.tex").exists());
    }

    #[test]
    fn trash_name_round_trips_underscores_and_percent() {
        for rel in [
            "my__notes.tex",
            "a__b/x.tex",
            "_lead/trail_.tex",
            "100%.tex",
            "%5F.tex",
            "sub/a.tex",
        ] {
            let base = rel.rsplit('/').next().unwrap();
            let name = trash_name(Path::new(base), rel);
            let (b, r) = split_trash_name(&name).unwrap();
            assert_eq!((b.as_str(), r.as_str()), (base, rel), "{}", name);
        }
        // Plain names keep the pre-escaping format, so old entries still decode.
        let plain = trash_name(Path::new("a.tex"), "sub/a.tex");
        assert!(plain.starts_with("a.tex__sub__a.tex__"));
        // Same literal as `trashName` in src/lib/file-history.test.ts.
        assert_eq!(
            split_trash_name("my%5F%5Fnotes.tex__a%5F%5Fb__my%5F%5Fnotes.tex__1234"),
            Some((
                "my__notes.tex".to_string(),
                "a__b/my__notes.tex".to_string()
            ))
        );
    }

    #[test]
    fn split_trash_name_rejects_malformed() {
        assert!(split_trash_name("a.tex__a.tex__12x").is_none());
        assert!(split_trash_name("a.tex__a.tex__").is_none());
        assert!(split_trash_name("a%41.tex__a%41.tex__1").is_none());
        assert!(split_trash_name("a%.tex__a%.tex__1").is_none());
        assert!(split_trash_name("a___b.tex__a___b.tex__1").is_none());
        assert!(split_trash_name("a.tex__b.tex__1").is_none());
        assert!(split_trash_name("a.tex__1").is_none());
        assert!(split_trash_name("b.tex__a____b.tex__1").is_none());
    }

    #[test]
    fn mcp_undo_restores_underscored_path() {
        let (id, dir) = grant_tmp("underscore");
        std::fs::create_dir_all(dir.join("a__b")).unwrap();
        std::fs::write(dir.join("a__b/my__notes.tex"), "u").unwrap();
        let abs = dir.join("a__b/my__notes.tex").to_string_lossy().to_string();
        let trashed = trash_file(&id, "a__b/my__notes.tex", &abs).unwrap();
        assert!(!dir.join("a__b/my__notes.tex").exists());
        let back = undo_trash(&id, &trashed).unwrap();
        assert_eq!(back, abs);
        assert_eq!(
            std::fs::read_to_string(dir.join("a__b/my__notes.tex")).unwrap(),
            "u"
        );
    }

    #[test]
    fn undo_rejects_outside_trash() {
        let (id, dir) = grant_tmp("outside");
        std::fs::write(dir.join("a.tex"), "hi").unwrap();
        let abs = dir.join("a.tex").to_string_lossy().to_string();
        assert!(undo_trash(&id, &abs).is_err());
    }
}
