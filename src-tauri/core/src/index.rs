//! The live project index of each session root: built by walking the root,
//! kept current by change batches (the app's watcher) or by a stat walk
//! before each read (processes without a watcher), with unsaved buffers
//! laid over it by the app. Lives in memory; nothing is written anywhere.

use crate::Core;

use std::collections::{HashMap, HashSet};
use std::path::{Component, Path};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use maleficium_index::{Disk, ProjectIndex, Unindexed};

/// Files larger than this are listed without text.
pub const MAX_TEXT_FILE_BYTES: u64 = 2 * 1024 * 1024;
/// Total text one project may hold; later files are listed without text.
pub const MAX_PROJECT_TEXT_BYTES: u64 = 256 * 1024 * 1024;
/// Files one project lists; the walk stops there.
pub const MAX_FILES: usize = 20_000;

/// Size and mtime of a file as last read.
type Stamp = (u64, Option<SystemTime>);

pub struct Live {
    pub index: ProjectIndex,
    stamps: HashMap<String, Stamp>,
    /// Disk text bytes held per file, summed in `text_bytes`.
    text_len: HashMap<String, u64>,
    text_bytes: u64,
    /// A watcher reports changes, so reads skip the stat walk.
    watched: bool,
    built: bool,
    /// Files the walk left out at `MAX_FILES`.
    pub truncated: usize,
}

/// Each session root's live index.
#[derive(Default)]
pub(crate) struct Indexes(Mutex<HashMap<String, Arc<Mutex<Live>>>>);

fn live_of(cx: &Core, root_id: &str) -> Arc<Mutex<Live>> {
    let mut reg = cx.indexes().0.lock().unwrap();
    reg.entry(root_id.to_string())
        .or_insert_with(|| {
            Arc::new(Mutex::new(Live {
                index: ProjectIndex::new(),
                stamps: HashMap::new(),
                text_len: HashMap::new(),
                text_bytes: 0,
                watched: false,
                built: false,
                truncated: 0,
            }))
        })
        .clone()
}

fn rel_of(root: &Path, abs: &Path) -> Option<String> {
    let rel = abs.strip_prefix(root).ok()?;
    let parts: Vec<String> = rel
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    (!parts.is_empty()).then(|| parts.join("/"))
}

/// True when any segment of `rel` is a name listings hide.
fn hidden(rel: &str) -> bool {
    rel.split('/').any(super::is_hidden_name)
}

fn stamp_of(meta: &std::fs::Metadata) -> Stamp {
    (meta.len(), meta.modified().ok())
}

impl Live {
    /// Read one file into the index.
    fn load(&mut self, root: &Path, rel: &str, meta: &std::fs::Metadata) {
        let bytes = meta.len();
        let disk = if !maleficium_structure::is_text_path(rel) {
            Disk::Listed {
                bytes,
                reason: Unindexed::NotText,
            }
        } else if bytes > MAX_TEXT_FILE_BYTES {
            Disk::Listed {
                bytes,
                reason: Unindexed::TooLarge,
            }
        } else if self.text_bytes + bytes > MAX_PROJECT_TEXT_BYTES {
            Disk::Listed {
                bytes,
                reason: Unindexed::OverBudget,
            }
        } else {
            match std::fs::read(root.join(rel)) {
                Ok(b) => match String::from_utf8(b) {
                    Ok(t) => Disk::Text(t),
                    Err(_) => Disk::Listed {
                        bytes,
                        reason: Unindexed::NotUtf8,
                    },
                },
                Err(_) => return self.forget(rel),
            }
        };
        self.forget_bytes(rel);
        if let Disk::Text(t) = &disk {
            self.text_bytes += t.len() as u64;
            self.text_len.insert(rel.to_string(), t.len() as u64);
        }
        self.stamps.insert(rel.to_string(), stamp_of(meta));
        self.index.set_disk(rel, disk);
    }

    fn forget_bytes(&mut self, rel: &str) {
        if let Some(n) = self.text_len.remove(rel) {
            self.text_bytes -= n;
        }
    }

    fn forget(&mut self, rel: &str) {
        self.forget_bytes(rel);
        self.stamps.remove(rel);
        self.index.remove(rel);
    }

    /// Walk the whole root: read new and changed files, drop vanished ones.
    fn walk(&mut self, root: &Path) {
        let mut seen: HashSet<String> = HashSet::new();
        let mut dirs = vec![root.to_path_buf()];
        let mut truncated = 0usize;
        while let Some(dir) = dirs.pop() {
            let Ok(rd) = std::fs::read_dir(&dir) else {
                continue;
            };
            let mut entries: Vec<_> = rd.flatten().collect();
            entries.sort_by_key(|e| e.file_name());
            for e in entries {
                let name = e.file_name();
                if super::is_hidden_name(&name.to_string_lossy()) {
                    continue;
                }
                // Symlinks are skipped: nothing outside the root is indexed.
                let Ok(ft) = e.file_type() else { continue };
                if ft.is_dir() {
                    dirs.push(e.path());
                    continue;
                }
                if !ft.is_file() {
                    continue;
                }
                let Some(rel) = rel_of(root, &e.path()) else {
                    continue;
                };
                if seen.len() >= MAX_FILES {
                    truncated += 1;
                    continue;
                }
                let Ok(meta) = e.metadata() else { continue };
                if self.stamps.get(&rel) != Some(&stamp_of(&meta)) {
                    self.load(root, &rel, &meta);
                }
                seen.insert(rel);
            }
        }
        let gone: Vec<String> = self
            .stamps
            .keys()
            .filter(|k| !seen.contains(*k))
            .cloned()
            .collect();
        for rel in gone {
            self.forget(&rel);
        }
        self.truncated = truncated;
        self.built = true;
    }

    /// Re-read the given root-relative paths (files or directories).
    fn touch(&mut self, root: &Path, rels: &[String]) {
        for rel in rels {
            if rel.is_empty() || hidden(rel) {
                continue;
            }
            match std::fs::symlink_metadata(root.join(rel)) {
                Ok(m) if m.is_file() => {
                    if self.stamps.get(rel) != Some(&stamp_of(&m)) {
                        self.load(root, rel, &m);
                    }
                }
                Ok(m) if m.is_dir() => {
                    // A directory appeared or moved in: walk everything once.
                    self.walk(root);
                    return;
                }
                _ => {
                    self.forget(rel);
                    let under: Vec<String> = self
                        .stamps
                        .keys()
                        .filter(|k| k.starts_with(&format!("{}/", rel)))
                        .cloned()
                        .collect();
                    for k in under {
                        self.forget(&k);
                    }
                }
            }
        }
    }
}

/// Run `f` on the root's index, current as of now: built on first use, and
/// stat-walked first unless a watcher keeps it current.
pub fn with<T>(cx: &Core, root_id: &str, f: impl FnOnce(&Live) -> T) -> Result<T, String> {
    let root = super::fs::session_root(cx, root_id)?;
    let live = live_of(cx, root_id);
    let mut l = live.lock().map_err(|_| "index lock poisoned".to_string())?;
    if !l.built || !l.watched {
        l.walk(&root);
    }
    Ok(f(&l))
}

/// Build (or rebuild) the root's index now, with no overlays: the opener
/// lays its buffers over it afresh. Returns the files listed.
pub fn open(cx: &Core, root_id: &str) -> Result<usize, String> {
    let root = super::fs::session_root(cx, root_id)?;
    let live = live_of(cx, root_id);
    let mut l = live.lock().map_err(|_| "index lock poisoned".to_string())?;
    let overlays: Vec<String> = l.index.overlays().map(str::to_string).collect();
    for rel in overlays {
        l.index.set_overlay(&rel, None);
    }
    l.walk(&root);
    Ok(l.index.len())
}

/// A watcher now reports this root's changes (`true`), or stopped (`false`).
pub fn set_watched(cx: &Core, root_id: &str, watched: bool) -> Result<(), String> {
    let live = live_of(cx, root_id);
    live.lock()
        .map_err(|_| "index lock poisoned".to_string())?
        .watched = watched;
    Ok(())
}

/// Apply a batch of changed paths (absolute or root-relative).
pub fn touch(cx: &Core, root_id: &str, paths: &[String]) -> Result<(), String> {
    let root = super::fs::session_root(cx, root_id)?;
    let rels: Vec<String> = paths
        .iter()
        .filter_map(|p| {
            let p = Path::new(p);
            if p.is_absolute() {
                rel_of(&root, p)
            } else {
                Some(p.to_string_lossy().into_owned())
            }
        })
        .collect();
    let live = live_of(cx, root_id);
    let mut l = live.lock().map_err(|_| "index lock poisoned".to_string())?;
    if !l.built {
        l.walk(&root);
    } else {
        l.touch(&root, &rels);
    }
    Ok(())
}

/// Lay an unsaved buffer over `rel`, or lift it (`None`).
pub fn overlay(cx: &Core, root_id: &str, rel: &str, text: Option<String>) -> Result<(), String> {
    crate::guard::reject_empty_nul(rel)?;
    // Components, not a `/` split: on Windows `a\..\..` climbs too.
    let escapes = Path::new(rel)
        .components()
        .any(|c| !matches!(c, Component::Normal(_) | Component::CurDir));
    if escapes {
        return Err(format!("forbidden path (outside project): {}", rel));
    }
    let live = live_of(cx, root_id);
    live.lock()
        .map_err(|_| "index lock poisoned".to_string())?
        .index
        .set_overlay(rel, text);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::time::Instant;

    fn project(cx: &Core, name: &str, files: &[(&str, &str)]) -> (String, PathBuf) {
        let dir = crate::test_scratch::dir(&format!("ix-{}", name));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for (rel, text) in files {
            let p = dir.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, text).unwrap();
        }
        let canon = dunce::canonicalize(&dir).unwrap();
        let id = format!("ix-{}", name);
        super::super::grant_root(cx, &id, &canon.to_string_lossy()).unwrap();
        (id, canon)
    }

    fn rels(cx: &Core, id: &str) -> Vec<String> {
        with(cx, id, |l| {
            l.index.iter().map(|f| f.rel.to_string()).collect()
        })
        .unwrap()
    }

    #[test]
    fn walk_lists_files_and_skips_hidden_ones() {
        let cx = &Core::default();
        let (id, _) = project(
            cx,
            "walk",
            &[
                ("main.tex", "x"),
                ("ch/a.tex", "y"),
                ("fig.png", "\u{89}PNG"),
                (".git/HEAD", "ref"),
                ("main.aux", "aux"),
                ("out/main.pdf", "pdf"),
            ],
        );
        assert_eq!(rels(cx, &id), ["ch/a.tex", "fig.png", "main.tex"]);
        with(cx, &id, |l| {
            assert_eq!(
                l.index.get("fig.png").unwrap().unindexed,
                Some(Unindexed::NotText)
            );
            assert_eq!(l.index.get("ch/a.tex").unwrap().text, Some("y"));
        })
        .unwrap();
    }

    #[test]
    fn unwatched_reads_see_disk_changes() {
        let cx = &Core::default();
        let (id, dir) = project(cx, "stat", &[("a.tex", "one")]);
        assert_eq!(rels(cx, &id), ["a.tex"]);
        std::fs::write(dir.join("a.tex"), "two, longer").unwrap();
        std::fs::write(dir.join("b.tex"), "new").unwrap();
        with(cx, &id, |l| {
            assert_eq!(l.index.get("a.tex").unwrap().text, Some("two, longer"))
        })
        .unwrap();
        std::fs::remove_file(dir.join("b.tex")).unwrap();
        assert_eq!(rels(cx, &id), ["a.tex"]);
    }

    #[test]
    fn watched_roots_apply_change_batches() {
        let cx = &Core::default();
        let (id, dir) = project(cx, "watch", &[("a.tex", "one"), ("d/x.tex", "x")]);
        open(cx, &id).unwrap();
        set_watched(cx, &id, true).unwrap();
        std::fs::write(dir.join("a.tex"), "changed!").unwrap();
        // No batch yet: a watched index is not re-walked.
        with(cx, &id, |l| {
            assert_eq!(l.index.get("a.tex").unwrap().text, Some("one"))
        })
        .unwrap();
        touch(cx, &id, &[dir.join("a.tex").to_string_lossy().into_owned()]).unwrap();
        with(cx, &id, |l| {
            assert_eq!(l.index.get("a.tex").unwrap().text, Some("changed!"))
        })
        .unwrap();
        std::fs::remove_dir_all(dir.join("d")).unwrap();
        touch(cx, &id, &["d".to_string()]).unwrap();
        assert_eq!(rels(cx, &id), ["a.tex"]);
        std::fs::create_dir_all(dir.join("e/f")).unwrap();
        std::fs::write(dir.join("e/f/g.tex"), "g").unwrap();
        touch(cx, &id, &["e".to_string()]).unwrap();
        assert_eq!(rels(cx, &id), ["a.tex", "e/f/g.tex"]);
    }

    #[test]
    fn overlays_are_confined_to_the_root() {
        let cx = &Core::default();
        let (id, _) = project(cx, "overlay", &[("a.tex", "disk")]);
        overlay(cx, &id, "a.tex", Some("buf".into())).unwrap();
        with(cx, &id, |l| {
            assert_eq!(l.index.get("a.tex").unwrap().text, Some("buf"))
        })
        .unwrap();
        for bad in crate::test_scratch::escapes() {
            assert!(overlay(cx, &id, bad, Some("y".into())).is_err(), "{bad}");
        }
    }

    #[test]
    fn non_utf8_text_is_listed_without_text() {
        let cx = &Core::default();
        let (id, dir) = project(cx, "utf8", &[]);
        std::fs::write(dir.join("bad.tex"), [0xff, 0xfe]).unwrap();
        with(cx, &id, |l| {
            assert_eq!(
                l.index.get("bad.tex").unwrap().unindexed,
                Some(Unindexed::NotUtf8)
            )
        })
        .unwrap();
    }

    /// Budget pin: cold build of a synthetic 3000-file project and, when
    /// present, a large real-world paper. Prints ms; loose caps.
    #[test]
    fn budget_cold_build() {
        let cx = &Core::default();
        let dir = crate::test_scratch::dir("ix-budget");
        let _ = std::fs::remove_dir_all(&dir);
        for c in 0..30 {
            let ch = dir.join(format!("chapters/c{c:02}"));
            std::fs::create_dir_all(&ch).unwrap();
            for s in 0..100 {
                let body = format!(
                    "\\section{{Section {c}.{s}}}\\label{{sec:{c}-{s}}}\n{}\nSee \\ref{{sec:{c}-0}} and \\cite{{key{s}}}.\n\\newcommand{{\\m{c}x{s}}}{{x}}\n",
                    "Lorem ipsum dolor sit amet, consectetur adipiscing elit. ".repeat(40)
                );
                std::fs::write(ch.join(format!("s{s:03}.tex")), body).unwrap();
            }
        }
        let canon = dunce::canonicalize(&dir).unwrap();
        super::super::grant_root(cx, "ix-budget", &canon.to_string_lossy()).unwrap();
        let t = Instant::now();
        let n = open(cx, "ix-budget").unwrap();
        let cold = t.elapsed().as_millis();
        let t = Instant::now();
        with(cx, "ix-budget", |l| l.index.maps().labels.len()).unwrap();
        let rewalk = t.elapsed().as_millis();
        println!(
            "index budget: synthetic {n} files cold {cold} ms, stat re-walk + maps {rewalk} ms"
        );
        assert_eq!(n, 3000);
        assert!(cold < 10_000, "cold build {cold} ms over the 10 s cap");
        assert!(rewalk < 2_000, "re-walk {rewalk} ms over the 2 s cap");
        let _ = std::fs::remove_dir_all(&dir);

        let tvcg = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../refs/TVCG_Paper_Ref");
        if let Ok(canon) = dunce::canonicalize(&tvcg) {
            super::super::grant_root(cx, "ix-tvcg", &canon.to_string_lossy()).unwrap();
            let t = Instant::now();
            let n = open(cx, "ix-tvcg").unwrap();
            let ms = t.elapsed().as_millis();
            println!("index budget: TVCG ref {n} files cold {ms} ms");
            assert!(ms < 10_000, "TVCG cold build {ms} ms over the 10 s cap");
        }
    }
}
