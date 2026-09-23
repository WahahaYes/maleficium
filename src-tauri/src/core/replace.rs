//! Replace across a project: preview a plan, apply it by token, undo it as
//! one history batch. Apply refuses the whole plan when any file it covers
//! has changed since the preview. Before anything is written, every file's
//! prior content goes into one history batch, so one undo restores them all.

use std::sync::{Mutex, OnceLock};

use maleficium_events::BatchFile;
use maleficium_index::replace::{self, BufferEdit, Planned, ReplaceApplied, ReplacePreview};
use maleficium_index::search::Query;

/// Plans held for apply; the oldest is dropped past this.
const MAX_PLANS: usize = 8;

struct Held {
    root_id: String,
    files: Vec<Planned>,
    replacements: u32,
}

static PLANS: OnceLock<Mutex<Vec<(String, Held)>>> = OnceLock::new();

fn plans() -> &'static Mutex<Vec<(String, Held)>> {
    PLANS.get_or_init(Default::default)
}

/// Plan replacing every match of `q` with `replacement`.
pub fn preview(
    root_id: &str,
    q: &Query,
    replacement: &str,
    main_rel: Option<&str>,
) -> Result<ReplacePreview, String> {
    let (files, hunks_truncated) = super::index::with(root_id, |l| {
        replace::plan(&l.index, q, replacement, main_rel)
    })??;
    let replacements = files.iter().map(|f| f.file.replacements).sum();
    let seed = format!(
        "{}\0{}\0{:?}\0{}",
        root_id,
        replacement,
        q,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    );
    let token = format!(
        "r{}",
        maleficium_structure::revision(
            std::iter::once(("plan", seed.as_str())).chain(
                files
                    .iter()
                    .map(|f| (f.file.rel.as_str(), f.file.revision.as_str()))
            )
        )
    );
    let out = ReplacePreview {
        token: token.clone(),
        files: files.iter().map(|f| f.file.clone()).collect(),
        replacements,
        hunks_truncated,
    };
    let mut held = plans()
        .lock()
        .map_err(|_| "plan store poisoned".to_string())?;
    held.push((
        token,
        Held {
            root_id: root_id.to_string(),
            files,
            replacements,
        },
    ));
    while held.len() > MAX_PLANS {
        held.remove(0);
    }
    Ok(out)
}

/// Apply a previewed plan. Files named in `keep_open` are not written: their
/// new text comes back in `edits` for the caller's open buffers. Every file,
/// written or not, is in the returned history batch.
pub fn apply(root_id: &str, token: &str, keep_open: &[String]) -> Result<ReplaceApplied, String> {
    let held = {
        let mut plans = plans()
            .lock()
            .map_err(|_| "plan store poisoned".to_string())?;
        let i = plans
            .iter()
            .position(|(t, h)| t == token && h.root_id == root_id)
            .ok_or_else(|| "unknown or expired replace plan: preview again".to_string())?;
        plans.remove(i).1
    };
    let stale: Vec<String> = super::index::with(root_id, |l| {
        held.files
            .iter()
            .filter(|f| {
                l.index.get(&f.file.rel).map(|v| v.revision) != Some(f.file.revision.as_str())
            })
            .map(|f| f.file.rel.clone())
            .collect()
    })?;
    if !stale.is_empty() {
        return Err(format!(
            "changed since the preview, nothing replaced: {}",
            stale.join(", ")
        ));
    }
    let prior: Vec<(String, Vec<u8>)> = held
        .files
        .iter()
        .map(|f| (f.file.rel.clone(), f.before.clone().into_bytes()))
        .collect();
    let batch = super::history::record_batch(root_id, &prior)?;
    let mut written = Vec::new();
    let mut edits = Vec::new();
    for f in &held.files {
        if keep_open.contains(&f.file.rel) {
            edits.push(BufferEdit {
                rel: f.file.rel.clone(),
                text: f.after.clone(),
            });
            continue;
        }
        let abs = super::fs::resolve_in(root_id, &f.file.rel)?;
        std::fs::write(&abs, &f.after)
            .map_err(|e| format!("write failed: {}: {}", f.file.rel, e))?;
        written.push(f.file.rel.clone());
    }
    super::index::touch(root_id, &written)?;
    Ok(ReplaceApplied {
        batch,
        written,
        edits,
        replacements: held.replacements,
    })
}

/// Undo a replace: every file of the batch back on disk as it was before.
pub fn undo(root_id: &str, batch: &str) -> Result<Vec<BatchFile>, String> {
    let done = super::history::restore_batch(root_id, batch)?;
    let rels: Vec<String> = done.iter().map(|f| f.rel.clone()).collect();
    super::index::touch(root_id, &rels)?;
    Ok(done)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn project(name: &str, files: &[(&str, &str)]) -> (String, PathBuf) {
        let dir =
            std::env::temp_dir().join(format!("maleficium-rp-{}-{}", std::process::id(), name));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for (rel, text) in files {
            let p = dir.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, text).unwrap();
        }
        let canon = dir.canonicalize().unwrap();
        let id = format!("rp-{}", name);
        super::super::grant_root(&id, &canon.to_string_lossy()).unwrap();
        (id, canon)
    }

    fn held_count(root_id: &str) -> usize {
        plans()
            .lock()
            .map(|p| p.iter().filter(|(_, h)| h.root_id == root_id).count())
            .unwrap_or(0)
    }

    fn q(p: &str) -> Query {
        Query {
            pattern: p.into(),
            ..Default::default()
        }
    }

    fn read(dir: &std::path::Path, rel: &str) -> String {
        std::fs::read_to_string(dir.join(rel)).unwrap()
    }

    #[test]
    fn preview_apply_undo_round_trips_bytes() {
        let (id, dir) = project(
            "roundtrip",
            &[
                ("a.tex", "old old\r\n"),
                ("ch/b.tex", "old\n"),
                ("c.tex", "keep"),
            ],
        );
        let p = preview(&id, &q("old"), "new", None).unwrap();
        assert_eq!((p.files.len(), p.replacements), (2, 3));
        assert_eq!(read(&dir, "a.tex"), "old old\r\n", "preview writes nothing");
        let a = apply(&id, &p.token, &[]).unwrap();
        assert_eq!(a.written, ["a.tex", "ch/b.tex"]);
        assert_eq!(read(&dir, "a.tex"), "new new\r\n");
        assert_eq!(read(&dir, "ch/b.tex"), "new\n");
        assert!(apply(&id, &p.token, &[]).is_err(), "a plan applies once");
        // The index sees the write at once.
        let again = preview(&id, &q("old"), "new", None).unwrap();
        assert!(again.files.is_empty());
        let restored = undo(&id, &a.batch).unwrap();
        assert_eq!(restored.len(), 2);
        assert_eq!(read(&dir, "a.tex"), "old old\r\n");
        assert_eq!(read(&dir, "ch/b.tex"), "old\n");
        assert_eq!(read(&dir, "c.tex"), "keep");
    }

    #[test]
    fn a_stale_plan_is_refused_whole() {
        let (id, dir) = project("stale", &[("a.tex", "x"), ("b.tex", "x")]);
        let p = preview(&id, &q("x"), "y", None).unwrap();
        std::fs::write(dir.join("b.tex"), "x changed").unwrap();
        let e = apply(&id, &p.token, &[]).unwrap_err();
        assert!(e.contains("b.tex"), "{e}");
        assert_eq!(read(&dir, "a.tex"), "x", "nothing replaced");
        assert!(apply(&id, "r-unknown", &[]).is_err());
    }

    #[test]
    fn open_buffers_come_back_as_edits_and_join_the_batch() {
        let (id, dir) = project("open", &[("a.tex", "x on disk"), ("b.tex", "x")]);
        super::super::index::overlay(&id, "a.tex", Some("x unsaved".into())).unwrap();
        let p = preview(&id, &q("x"), "y", None).unwrap();
        let a = apply(&id, &p.token, &["a.tex".to_string()]).unwrap();
        assert_eq!(a.written, ["b.tex"]);
        assert_eq!(
            a.edits,
            [BufferEdit {
                rel: "a.tex".into(),
                text: "y unsaved".into()
            }]
        );
        assert_eq!(read(&dir, "a.tex"), "x on disk");
        let members = super::super::history::batch_files(&id, &a.batch);
        let a_rev = &members.iter().find(|f| f.rel == "a.tex").unwrap().rev;
        assert_eq!(
            super::super::history::get(&id, "a.tex", a_rev).unwrap(),
            b"x unsaved"
        );
    }

    #[test]
    fn plans_are_bounded() {
        let (id, _) = project("bounded", &[("a.tex", "x")]);
        for _ in 0..MAX_PLANS + 3 {
            preview(&id, &q("x"), "y", None).unwrap();
        }
        assert!(held_count(&id) <= MAX_PLANS);
    }
}
