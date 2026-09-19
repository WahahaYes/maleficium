//! Filesystem operations against explicit session roots.
//!
//! The MCP sidecar is native code: Tauri capabilities do not gate it, so the
//! session-root map below is the entire boundary. Every op resolves its path
//! against a granted root and rejects escapes before touching the fs.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use super::{hash_root, is_hidden_name, out_dir_for, FileEntry};

/// Granted roots, keyed by session id. Additive per process: roots stay
/// readable until quit; no forbid-on-close in v1.
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
    let canon = joined
        .canonicalize()
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

fn trash_home(root: &Path) -> PathBuf {
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|h| {
                let mut p = PathBuf::from(h);
                p.push(".local/share");
                p
            })
        })
        .unwrap_or_else(std::env::temp_dir);
    base.join("maleficium-trash")
        .join(hash_root(&root.to_string_lossy()))
}

fn trash_name(original: &Path, rel: &str) -> String {
    let base = original
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "file".to_string());
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let flat_rel = rel.replace('/', "__");
    format!("{}__{}__{}", base, flat_rel, stamp)
}

fn split_trash_name(name: &str) -> Option<(String, String)> {
    let (base, rest) = name.split_once("__")?;
    let (flat_rel, _stamp) = rest.rsplit_once("__")?;
    let rel = flat_rel.replace("__", "/");
    if !crate::commands::guard::is_bare_filename(base) || rel.is_empty() || rel.contains('\0') {
        return None;
    }
    if rel.contains("__") {
        return None;
    }
    Some((base.to_string(), rel))
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
    let canon_src = src
        .canonicalize()
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

/// Read the tail of the engine log for one main-file dir shard.
pub fn log_tail(id: &str, rel: &str, max_lines: usize) -> Result<String, String> {
    let abs = resolve_in(id, rel)?;
    let dir = abs
        .parent()
        .ok_or_else(|| "no parent directory".to_string())?;
    let stem = abs
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let outdir = out_dir_for(&super::out_base_dir(), &dir.to_string_lossy());
    let log_path = outdir.join(format!("{}.log", stem));
    let text = std::fs::read_to_string(&log_path).map_err(|e| format!("log unavailable: {}", e))?;
    let lines: Vec<&str> = text.lines().collect();
    let start = lines.len().saturating_sub(max_lines.max(1));
    Ok(lines[start..].join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grant_tmp(name: &str) -> (String, PathBuf) {
        let dir =
            std::env::temp_dir().join(format!("maleficium-fs-{}-{}", std::process::id(), name));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let canon = dir.canonicalize().unwrap();
        let id = format!("t-{}", name);
        grant_root(&id, &canon.to_string_lossy()).unwrap();
        (id, canon)
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
    fn undo_rejects_outside_trash() {
        let (id, dir) = grant_tmp("outside");
        std::fs::write(dir.join("a.tex"), "hi").unwrap();
        let abs = dir.join("a.tex").to_string_lossy().to_string();
        assert!(undo_trash(&id, &abs).is_err());
    }
}
