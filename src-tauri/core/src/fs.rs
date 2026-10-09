//! Filesystem operations against explicit session roots. Every op resolves
//! its path against a granted root and rejects escapes before touching
//! the fs.

use crate::Core;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{hash_root, is_hidden_name, FileEntry};

/// Granted roots, keyed by session id. Roots stay readable until the
/// `Core` is dropped.
#[derive(Default)]
pub(crate) struct Sessions(Mutex<HashMap<String, PathBuf>>);

fn validate_root_id(id: &str) -> Result<&str, String> {
    crate::guard::reject_empty_nul(id)?;
    if id.len() > 64 || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
        return Err("forbidden root id (alphanumeric/dash, max 64)".to_string());
    }
    Ok(id)
}

/// Mint a session root: validate the directory, canonicalize it, store it.
pub fn grant_root(cx: &Core, id: &str, root: &str) -> Result<PathBuf, String> {
    validate_root_id(id)?;
    let canon = crate::guard::canonical_root(root)?;
    crate::widget_approval::refuse_store_overlap(&canon, &crate::widget_approval::store_base())?;
    cx.sessions()
        .0
        .lock()
        .map_err(|_| "roots lock poisoned".to_string())?
        .insert(id.to_string(), canon.clone());
    Ok(canon)
}

/// Register a project root under its own id (djb2 of the canonical path).
/// Returns the canonical root and the id that names it across the seam.
pub fn grant_project(cx: &Core, root: &str) -> Result<(PathBuf, String), String> {
    let canon = crate::guard::canonical_root(root)?;
    let id = hash_root(&canon.to_string_lossy());
    grant_root(cx, &id, &canon.to_string_lossy())?;
    Ok((canon, id))
}

/// Create the untitled scratch dir if needed and register it like a project.
pub fn grant_untitled(cx: &Core) -> Result<(PathBuf, String), String> {
    let dir = super::untitled_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("scratch dir unavailable: {}", e))?;
    grant_project(cx, &dir.to_string_lossy())
}

/// Look up a session root by id.
pub fn session_root(cx: &Core, id: &str) -> Result<PathBuf, String> {
    validate_root_id(id)?;
    cx.sessions()
        .0
        .lock()
        .map_err(|_| "roots lock poisoned".to_string())?
        .get(id)
        .cloned()
        .ok_or_else(|| format!("unknown project root: {}", id))
}

/// The root id this process granted for `project`, if any (lowest id when
/// several name the same folder).
pub fn granted_as(cx: &Core, project: &str) -> Option<String> {
    let want = dunce::canonicalize(project).ok()?;
    let roots = cx.sessions().0.lock().ok()?;
    let mut ids: Vec<&String> = roots
        .iter()
        .filter(|(_, root)| **root == want)
        .map(|(id, _)| id)
        .collect();
    ids.sort();
    ids.first().map(|id| id.to_string())
}

/// Resolve `candidate` inside the session root: rejects NUL, requires the
/// canonicalized path to sit under the granted root.
pub fn resolve_in(cx: &Core, id: &str, candidate: &str) -> Result<PathBuf, String> {
    crate::guard::reject_empty_nul(candidate)?;
    let root = session_root(cx, id)?;
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
pub fn resolve_read(cx: &Core, id: &str, candidate: &str) -> Result<PathBuf, String> {
    if let Ok(p) = resolve_in(cx, id, candidate) {
        return Ok(p);
    }
    crate::guard::reject_empty_nul(candidate)?;
    let root = session_root(cx, id)?;
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
/// separator: `%` -> `%25`, then `_` -> `%5F`.
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
    if !crate::guard::is_bare_filename(&base)
        || comps.iter().any(|c| c.is_empty())
        || rel.contains('\0')
    {
        return None;
    }
    Some((base, rel))
}

/// Move a project file to the app-local trash home. Returns the trash path.
pub fn trash_file(cx: &Core, id: &str, rel: &str, confirm: &str) -> Result<String, String> {
    let abs = resolve_in(cx, id, rel)?;
    // Compared as paths, not strings: on Windows the caller may spell the
    // same file with `/`, mixed separators, or another letter case. Only an
    // absolute path confirms (a bare name would resolve against the CWD).
    let confirmed = confirm == abs.to_string_lossy()
        || (Path::new(confirm).is_absolute()
            && dunce::canonicalize(confirm).is_ok_and(|c| c == abs));
    if !confirmed {
        return Err(if confirm.is_empty() {
            format!(
                "confirm needed: call delete again with confirm set to {}",
                abs.display()
            )
        } else {
            format!(
                "confirm must be the file's absolute path: {}",
                abs.display()
            )
        });
    }
    if !abs.is_file() {
        return Err(format!("not a file: {}", rel));
    }
    let root = session_root(cx, id)?;
    let home = trash_home(&root);
    std::fs::create_dir_all(&home).map_err(|e| format!("trash home unreachable: {}", e))?;
    let dest = home.join(trash_name(&abs, rel));
    let trashed = match std::fs::rename(&abs, &dest) {
        Ok(()) => Ok(dest.to_string_lossy().to_string()),
        Err(_) => {
            let bytes = std::fs::read(&abs).map_err(|e| format!("trash copy failed: {}", e))?;
            std::fs::write(&dest, &bytes).map_err(|e| format!("trash copy failed: {}", e))?;
            std::fs::remove_file(&abs).map_err(|e| format!("trash copy failed: {}", e))?;
            Ok(dest.to_string_lossy().to_string())
        }
    };
    super::watch::mark_removed(cx, &abs);
    trashed
}

/// Restore a trashed file to its original path.
pub fn undo_trash(cx: &Core, id: &str, trash_path: &str) -> Result<String, String> {
    crate::guard::reject_empty_nul(trash_path)?;
    let root = session_root(cx, id)?;
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
    let dest = resolve_read(cx, id, &rel)?;
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("restore failed: {}", e))?;
    }
    let restored = match std::fs::rename(&canon_src, &dest) {
        Ok(()) => Ok(dest.to_string_lossy().to_string()),
        Err(_) => {
            let bytes = std::fs::read(&canon_src).map_err(|e| format!("restore failed: {}", e))?;
            std::fs::write(&dest, &bytes).map_err(|e| format!("restore failed: {}", e))?;
            std::fs::remove_file(&canon_src).map_err(|e| format!("restore failed: {}", e))?;
            Ok(dest.to_string_lossy().to_string())
        }
    };
    super::watch::mark_settled(cx, &dest);
    restored
}

/// List one directory level, sorted dirs-first. Hidden/build names skipped.
pub fn list_dir(cx: &Core, id: &str, rel: &str) -> Result<Vec<FileEntry>, String> {
    let dir = if rel.is_empty() || rel == "." {
        session_root(cx, id)?
    } else {
        resolve_read(cx, id, rel)?
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
pub fn read_text(cx: &Core, id: &str, rel: &str) -> Result<String, String> {
    let abs = resolve_in(cx, id, rel)?;
    std::fs::read_to_string(&abs).map_err(|e| format!("read failed: {}", e))
}

/// Resolve `candidate` for writing: an existing path resolves strictly
/// (symlinks followed, must stay under the root); a missing path resolves
/// lexically with its canonicalized parent checked, so a write never lands
/// through a symlink that points outside the project. Parents must exist:
/// writes never create them.
pub fn resolve_write(cx: &Core, id: &str, candidate: &str) -> Result<PathBuf, String> {
    let probe = resolve_read(cx, id, candidate)?;
    let root = session_root(cx, id)?;
    match dunce::canonicalize(&probe) {
        Ok(canon) => {
            if canon.starts_with(&root) {
                Ok(canon)
            } else {
                Err(format!("forbidden path (outside project): {}", candidate))
            }
        }
        Err(_) => {
            let parent = probe
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .ok_or_else(|| format!("forbidden path (no parent directory): {}", candidate))?;
            let canon_parent = dunce::canonicalize(parent)
                .map_err(|e| format!("forbidden path (unresolvable): {}: {}", candidate, e))?;
            if !canon_parent.starts_with(&root) {
                return Err(format!("forbidden path (outside project): {}", candidate));
            }
            Ok(probe)
        }
    }
}

/// Read a project file as bytes (previews go through here; text reads use
/// `read_text`).
pub fn read_bytes(cx: &Core, id: &str, rel: &str) -> Result<Vec<u8>, String> {
    let abs = resolve_in(cx, id, rel)?;
    std::fs::read(&abs).map_err(|e| format!("read failed: {}", e))
}

/// Write bytes to a project file. The destination's parents must exist.
pub fn write_bytes(cx: &Core, id: &str, rel: &str, bytes: &[u8]) -> Result<(), String> {
    let abs = resolve_write(cx, id, rel)?;
    std::fs::write(&abs, bytes).map_err(|e| format!("write failed: {}", e))?;
    super::watch::mark_written(cx, &abs, bytes);
    Ok(())
}

/// Save a buffer in one call: refuse when the disk holds an outside edit
/// the caller has not seen (`base`: what the buffer last synced), else
/// write and snapshot a history revision. A missing file writes fresh; a
/// disk that already holds the new bytes saves without complaint. Returns
/// the revision outcome for the caller's event log.
pub fn save(
    cx: &Core,
    id: &str,
    rel: &str,
    bytes: &[u8],
    base: Option<&[u8]>,
) -> Result<maleficium_events::RecordOutcome, String> {
    let abs = resolve_write(cx, id, rel)?;
    let current = std::fs::read(&abs).ok();
    // A missing file writes fresh, as the frontend's null-disk read does.
    if let (Some(want), Some(cur)) = (base, current.as_deref()) {
        if cur != want && cur != bytes {
            return Err("changed on disk: reload or keep your edits first".to_string());
        }
    }
    std::fs::write(&abs, bytes).map_err(|e| format!("write failed: {}", e))?;
    super::watch::mark_written(cx, &abs, bytes);
    Ok(crate::history::record(cx, id, rel, bytes))
}

/// Rename within the project. Both sides stay confined.
pub fn rename_path(cx: &Core, id: &str, old_rel: &str, new_rel: &str) -> Result<(), String> {
    let from = resolve_write(cx, id, old_rel)?;
    let to = resolve_write(cx, id, new_rel)?;
    std::fs::rename(&from, &to).map_err(|e| format!("rename failed: {}", e))?;
    super::watch::mark_removed(cx, &from);
    super::watch::mark_settled(cx, &to);
    Ok(())
}

/// Create a project directory and its missing parents.
pub fn make_dir(cx: &Core, id: &str, rel: &str) -> Result<(), String> {
    let abs = resolve_write(cx, id, rel)?;
    std::fs::create_dir_all(&abs).map_err(|e| format!("mkdir failed: {}", e))?;
    super::watch::mark_settled(cx, &abs);
    Ok(())
}

/// Remove a project file or directory (directories only with `recursive`).
pub fn remove_path(cx: &Core, id: &str, rel: &str, recursive: bool) -> Result<(), String> {
    let abs = resolve_write(cx, id, rel)?;
    if abs.is_dir() {
        if recursive {
            std::fs::remove_dir_all(&abs)
        } else {
            std::fs::remove_dir(&abs)
        }
        .map_err(|e| format!("remove failed: {}", e))?;
    } else {
        std::fs::remove_file(&abs).map_err(|e| format!("remove failed: {}", e))?;
    }
    super::watch::mark_removed(cx, &abs);
    Ok(())
}

/// Stat of a project file. Missing or unreadable reads as absent (`None`),
/// matching the frontend seam's contract; escapes are errors.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct FileStat {
    pub size: u64,
    pub is_file: bool,
    pub is_dir: bool,
}

pub fn stat_path(cx: &Core, id: &str, rel: &str) -> Result<Option<FileStat>, String> {
    let probe = if rel.is_empty() || rel == "." {
        session_root(cx, id)?
    } else {
        resolve_read(cx, id, rel)?
    };
    let root = session_root(cx, id)?;
    let canon = match dunce::canonicalize(&probe) {
        Ok(c) => c,
        Err(_) => return Ok(None),
    };
    if !canon.starts_with(&root) {
        return Ok(None);
    }
    match std::fs::metadata(&canon) {
        Ok(m) => Ok(Some(FileStat {
            size: m.len(),
            is_file: m.is_file(),
            is_dir: m.is_dir(),
        })),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("stat failed: {}", e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grant_tmp(cx: &Core, name: &str) -> (String, PathBuf) {
        let dir = crate::test_scratch::dir(&format!("fs-{}", name));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let canon = dunce::canonicalize(&dir).unwrap();
        let id = format!("t-{}", name);
        grant_root(cx, &id, &canon.to_string_lossy()).unwrap();
        (id, canon)
    }

    #[test]
    fn grant_project_mints_byte_hash_id() {
        let cx = &Core::default();
        let dir = crate::test_scratch::dir("fs-proj-ü");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let (canon, id) = grant_project(cx, &dir.to_string_lossy()).unwrap();
        assert_eq!(id, hash_root(&canon.to_string_lossy()));
        assert_eq!(session_root(cx, &id).unwrap(), canon);
        std::fs::write(canon.join("main.tex"), "x").unwrap();
        assert!(resolve_in(cx, &id, "main.tex").is_ok());
        assert!(resolve_in(cx, &id, "../x").is_err());
        assert!(grant_project(cx, "relative/dir").is_err());
    }

    #[test]
    fn grant_rejects_bad_ids_and_paths() {
        let cx = &Core::default();
        assert!(grant_root(cx, "", "/tmp").is_err());
        assert!(grant_root(cx, "a/b", "/tmp").is_err());
        assert!(grant_root(cx, "ok-1", "relative/path").is_err());
        assert!(grant_root(cx, "ok-2", "/tmp/a\0b").is_err());
    }

    #[test]
    fn resolve_blocks_escape() {
        let cx = &Core::default();
        let (id, _dir) = grant_tmp(cx, "escape");
        for bad in crate::test_scratch::escapes() {
            assert!(resolve_in(cx, &id, bad).is_err(), "{bad}");
            assert!(resolve_read(cx, &id, bad).is_err(), "{bad}");
        }
        assert!(resolve_in(cx, &id, "").is_err());
        assert!(resolve_in(cx, &id, "a\0b").is_err());
    }

    #[test]
    fn trash_requires_matching_confirm() {
        let cx = &Core::default();
        let (id, dir) = grant_tmp(cx, "confirm");
        std::fs::write(dir.join("a.tex"), "hi").unwrap();
        let abs = resolve_in(cx, &id, "a.tex")
            .unwrap()
            .to_string_lossy()
            .into_owned();
        // The first call asks for the confirm, naming the value to pass.
        let ask = trash_file(cx, &id, "a.tex", "").unwrap_err();
        assert!(
            ask.starts_with("confirm needed") && ask.ends_with(&abs),
            "{ask}"
        );
        let wrong = trash_file(cx, &id, "a.tex", "a.tex").unwrap_err();
        assert!(
            wrong.contains("absolute path") && wrong.ends_with(&abs),
            "{wrong}"
        );
        assert!(trash_file(cx, &id, "a.tex", "wrong").is_err());
        assert!(dir.join("a.tex").exists());
    }

    #[test]
    fn trash_confirm_accepts_another_spelling_of_the_same_file() {
        let cx = &Core::default();
        let (id, dir) = grant_tmp(cx, "confirm-spelling");
        std::fs::write(dir.join("a.tex"), "hi").unwrap();
        let spelled = format!("{}/./a.tex", dir.to_string_lossy());
        assert!(trash_file(cx, &id, "a.tex", &spelled).is_ok());
        assert!(!dir.join("a.tex").exists());
    }

    #[test]
    fn trash_round_trip_restores_bytes() {
        let cx = &Core::default();
        let (id, dir) = grant_tmp(cx, "roundtrip");
        std::fs::write(dir.join("a.tex"), "hello").unwrap();
        let abs = dir.join("a.tex").to_string_lossy().to_string();
        let trashed = trash_file(cx, &id, "a.tex", &abs).unwrap();
        assert!(!dir.join("a.tex").exists());
        let back = undo_trash(cx, &id, &trashed).unwrap();
        assert_eq!(back, abs);
        assert_eq!(std::fs::read_to_string(dir.join("a.tex")).unwrap(), "hello");
    }

    #[test]
    fn mcp_delete_lands_in_app_trash_dir() {
        let cx = &Core::default();
        let (id, dir) = grant_tmp(cx, "apptrash");
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("sub/a.tex"), "x").unwrap();
        let abs = dir.join("sub/a.tex").to_string_lossy().to_string();
        let trashed = PathBuf::from(trash_file(cx, &id, "sub/a.tex", &abs).unwrap());
        let app_dir = super::super::data_base_dir()
            .join("maleficium-trash")
            .join(hash_root(&dir.to_string_lossy()));
        assert_eq!(trashed.parent().unwrap(), app_dir);
        let name = trashed.file_name().unwrap().to_string_lossy().to_string();
        let stamp = name.strip_prefix("a.tex__sub__a.tex__").unwrap();
        assert!(!stamp.is_empty() && stamp.bytes().all(|b| b.is_ascii_digit()));
        undo_trash(cx, &id, &trashed.to_string_lossy()).unwrap();
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
        // The escaped literal below pins the wire format core undo reads.
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
        let cx = &Core::default();
        let (id, dir) = grant_tmp(cx, "underscore");
        std::fs::create_dir_all(dir.join("a__b")).unwrap();
        std::fs::write(dir.join("a__b/my__notes.tex"), "u").unwrap();
        let abs = dir.join("a__b/my__notes.tex").to_string_lossy().to_string();
        let trashed = trash_file(cx, &id, "a__b/my__notes.tex", &abs).unwrap();
        assert!(!dir.join("a__b/my__notes.tex").exists());
        let back = undo_trash(cx, &id, &trashed).unwrap();
        assert_eq!(back, abs);
        assert_eq!(
            std::fs::read_to_string(dir.join("a__b/my__notes.tex")).unwrap(),
            "u"
        );
    }

    #[test]
    fn undo_rejects_outside_trash() {
        let cx = &Core::default();
        let (id, dir) = grant_tmp(cx, "outside");
        std::fs::write(dir.join("a.tex"), "hi").unwrap();
        let abs = dir.join("a.tex").to_string_lossy().to_string();
        assert!(undo_trash(cx, &id, &abs).is_err());
    }

    #[test]
    fn write_rename_mkdir_remove_stat_round_trip() {
        let cx = &Core::default();
        let (id, dir) = grant_tmp(cx, "mutate");
        make_dir(cx, &id, "sub").unwrap();
        write_bytes(cx, &id, "sub/a.tex", b"hello").unwrap();
        assert_eq!(read_bytes(cx, &id, "sub/a.tex").unwrap(), b"hello");
        let st = stat_path(cx, &id, "sub/a.tex").unwrap().unwrap();
        assert_eq!((st.size, st.is_file, st.is_dir), (5, true, false));
        rename_path(cx, &id, "sub/a.tex", "sub/b.tex").unwrap();
        assert!(!dir.join("sub/a.tex").exists());
        assert_eq!(read_bytes(cx, &id, "sub/b.tex").unwrap(), b"hello");
        remove_path(cx, &id, "sub/b.tex", false).unwrap();
        assert!(stat_path(cx, &id, "sub/b.tex").unwrap().is_none());
        remove_path(cx, &id, "sub", false).unwrap();
        assert!(!dir.join("sub").exists());
    }

    #[test]
    fn mutating_ops_reject_escapes() {
        let cx = &Core::default();
        let (id, _dir) = grant_tmp(cx, "mut-escape");
        for bad in crate::test_scratch::escapes() {
            assert!(resolve_write(cx, &id, bad).is_err(), "{bad}");
            assert!(write_bytes(cx, &id, bad, b"x").is_err(), "{bad}");
            assert!(read_bytes(cx, &id, bad).is_err(), "{bad}");
            assert!(rename_path(cx, &id, bad, "ok.tex").is_err(), "{bad}");
            assert!(rename_path(cx, &id, "ok.tex", bad).is_err(), "{bad}");
            assert!(make_dir(cx, &id, bad).is_err(), "{bad}");
            assert!(remove_path(cx, &id, bad, true).is_err(), "{bad}");
            assert!(stat_path(cx, &id, bad).is_err(), "{bad}");
            assert!(save(cx, &id, bad, b"x", None).is_err(), "{bad}");
        }
        // Writes never create parents on their own.
        assert!(write_bytes(cx, &id, "no/such/parent/a.tex", b"x").is_err());
    }

    #[test]
    fn save_conflict_check_mirrors_the_frontend() {
        let cx = &Core::default();
        let (id, _dir) = grant_tmp(cx, "save-conflict");
        // A missing file writes fresh, and the write snapshots a revision.
        let first = save(cx, &id, "a.tex", b"mine", Some(b"synced")).unwrap();
        assert!(first.stored, "{first:?}");
        // Saving over the text the buffer last synced, or over its own
        // text, succeeds; an unseen outside edit is refused.
        assert!(save(cx, &id, "a.tex", b"mine2", Some(b"mine")).is_ok());
        assert!(save(cx, &id, "a.tex", b"mine2", Some(b"mine2")).is_ok());
        let err = save(cx, &id, "a.tex", b"mine3", Some(b"stale")).unwrap_err();
        assert!(err.contains("changed on disk"), "{err}");
        assert_eq!(read_bytes(cx, &id, "a.tex").unwrap(), b"mine2");
        // No base: overwrite, as the compile persist does.
        assert!(save(cx, &id, "a.tex", b"forced", None).is_ok());
    }

    #[test]
    fn stat_missing_reads_absent_and_root_stats() {
        let cx = &Core::default();
        let (id, _dir) = grant_tmp(cx, "stat-absent");
        assert!(stat_path(cx, &id, "missing.tex").unwrap().is_none());
        for root in ["", "."] {
            let st = stat_path(cx, &id, root).unwrap().unwrap();
            assert!((st.is_dir, st.is_file) == (true, false), "{st:?}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn writes_never_land_through_an_outside_symlink() {
        let cx = &Core::default();
        let (id, dir) = grant_tmp(cx, "sym-write");
        let outside = crate::test_scratch::dir("sym-write-outside");
        let _ = std::fs::remove_dir_all(&outside);
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("secret.tex"), "secret").unwrap();
        std::os::unix::fs::symlink(outside.join("secret.tex"), dir.join("link.tex")).unwrap();
        std::os::unix::fs::symlink(&outside, dir.join("linkdir")).unwrap();
        // Every mutating op refuses the link itself...
        assert!(write_bytes(cx, &id, "link.tex", b"x").is_err());
        assert!(save(cx, &id, "link.tex", b"x", None).is_err());
        assert!(rename_path(cx, &id, "link.tex", "b.tex").is_err());
        assert!(remove_path(cx, &id, "link.tex", false).is_err());
        // ...as well as paths beneath the linked directory, whether or not
        // the far side exists.
        assert!(write_bytes(cx, &id, "linkdir/a.tex", b"x").is_err());
        assert!(write_bytes(cx, &id, "linkdir/secret.tex", b"x").is_err());
        assert!(make_dir(cx, &id, "linkdir/sub").is_err());
        // The outside file is untouched throughout.
        assert_eq!(
            std::fs::read_to_string(outside.join("secret.tex")).unwrap(),
            "secret"
        );
        // A stat through the link reads as absent, never as the outside file.
        assert!(stat_path(cx, &id, "link.tex").unwrap().is_none());
        let _ = std::fs::remove_dir_all(&outside);
    }
}
