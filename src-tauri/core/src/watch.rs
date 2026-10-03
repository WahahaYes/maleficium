//! Core-owned file watcher: one `notify` thread per session root feeding
//! the index and queuing external-change events for attached adapters.
//!
//! The thread resolves every batch against the session root, re-reads the
//! index entries (`index::touch`), then drops echoes of core's own writes —
//! every core mutating op marks what it left on disk — and queues the rest.
//! Adapters drain the queue: the desktop frontend polls `watch_poll` on a
//! timer, the future HTTP adapter the same way. Processes without a watcher
//! (MCP calls) never go stale either: index reads stat-walk the root unless
//! a watcher keeps it current, so the MCP index stays fresh without the app.
//!
//! Docker bind mounts (xp.2 risk): inotify does not cross Docker Desktop's
//! file-sharing boundary, so a watcher inside the container misses host
//! writes. `MALEFICIUM_WATCH_POLL_MS` switches this module to a polling
//! watcher with that interval; unset, the OS watcher runs. Either way the
//! stat walk behind unwatched reads and the frontend's focus recheck stay
//! the backstop: a missed event costs freshness, never correctness.

use std::collections::{HashMap, VecDeque};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

use maleficium_events::WatchChange;
use notify::event::{ModifyKind, RenameMode};
use notify::{Config, Event, EventKind, PollWatcher, RecommendedWatcher, Watcher};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use ts_rs::TS;

use crate::Core;

/// One queued change: the root-relative path and what happened to it.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct WatchEvent {
    pub rel: String,
    pub change: WatchChange,
}

/// What a path should hold after our write: a content hash, a directory, or
/// absent. Echo matching is content-based, never time-based: a save can
/// echo late or repeatedly without false alarms, and an external edit
/// landing right after a save is never swallowed.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Sig {
    File(String),
    Dir,
    Absent,
}

/// Expected disk state by normalized absolute path.
#[derive(Default)]
pub(crate) struct OwnWrites(Mutex<HashMap<String, Sig>>);

/// Stop flags of the live watcher threads, by session root id.
#[derive(Default)]
pub(crate) struct Watchers(Mutex<HashMap<String, Arc<AtomicBool>>>);

/// Queued external changes by session root id, drained by `poll`.
#[derive(Default)]
pub(crate) struct Queues(Mutex<HashMap<String, VecDeque<WatchEvent>>>);

/// Batches settle this long after the last raw event before processing.
const DEBOUNCE_MS: u64 = 250;
/// Queued changes kept per root; overflow drops the oldest (a stat walk on
/// the next read still converges the index, so nothing is lost but latency).
const MAX_QUEUED: usize = 2000;

/// Map key for an absolute path: separators normalized (a rename target the
/// OS reports with `/` must match the `\`-joined mark on Windows).
fn key_of(abs: &Path) -> String {
    let s = abs.to_string_lossy().into_owned();
    #[cfg(windows)]
    {
        s.replace('\\', "/").to_lowercase()
    }
    #[cfg(not(windows))]
    {
        s
    }
}

fn hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn sig_of(path: &Path) -> Sig {
    match std::fs::symlink_metadata(path) {
        Ok(m) if m.is_dir() => Sig::Dir,
        Ok(m) if m.is_file() => match std::fs::read(path) {
            Ok(b) => Sig::File(hex(&b)),
            Err(_) => Sig::Absent,
        },
        _ => Sig::Absent,
    }
}

/// We wrote `bytes` to `abs`.
pub fn mark_written(cx: &Core, abs: &Path, bytes: &[u8]) {
    own_of(cx)
        .lock()
        .map(|mut o| o.insert(key_of(abs), Sig::File(hex(bytes))))
        .ok();
}

/// We removed `abs`.
pub fn mark_removed(cx: &Core, abs: &Path) {
    own_of(cx)
        .lock()
        .map(|mut o| o.insert(key_of(abs), Sig::Absent))
        .ok();
}

/// An own change we did not write byte by byte: record what `abs` holds now.
pub fn mark_settled(cx: &Core, abs: &Path) {
    let sig = sig_of(abs);
    own_of(cx)
        .lock()
        .map(|mut o| o.insert(key_of(abs), sig))
        .ok();
}

fn own_of(cx: &Core) -> &Mutex<HashMap<String, Sig>> {
    &cx.owns().0
}

/// True while `abs` on disk is still exactly what we left there.
pub fn is_echo(cx: &Core, abs: &Path) -> bool {
    let key = key_of(abs);
    let want = match own_of(cx).lock().map(|o| o.get(&key).cloned()) {
        Ok(Some(w)) => w,
        _ => return false,
    };
    if sig_of(abs) == want {
        return true;
    }
    // A newer own write may have replaced the record meanwhile: keep that one.
    if let Ok(mut o) = own_of(cx).lock() {
        if o.get(&key) == Some(&want) {
            o.remove(&key);
        }
    }
    false
}

/// Which watcher implementation a root gets.
#[derive(Debug, Clone, Copy)]
enum Mode {
    Recommended,
    Poll(Duration),
}

fn mode_from_env() -> Mode {
    match std::env::var("MALEFICIUM_WATCH_POLL_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|ms| *ms > 0)
    {
        Some(ms) => Mode::Poll(Duration::from_millis(ms)),
        None => Mode::Recommended,
    }
}

/// Start tracking `root_id`: a watcher thread feeds the index and queues
/// external changes. Idempotent: a live root reports success.
pub fn start(cx: &Core, root_id: &str) -> Result<(), String> {
    start_with_mode(cx, root_id, mode_from_env())
}

fn start_with_mode(cx: &Core, root_id: &str, mode: Mode) -> Result<(), String> {
    let root = super::fs::session_root(cx, root_id)?;
    if !root.is_dir() {
        return Err(format!("project root not a directory: {}", root.display()));
    }
    let stop = Arc::new(AtomicBool::new(false));
    {
        let mut live = cx
            .watchers()
            .0
            .lock()
            .map_err(|_| "watcher registry poisoned".to_string())?;
        if live.contains_key(root_id) {
            return Ok(());
        }
        live.insert(root_id.to_string(), stop.clone());
    }
    let (tx, rx) = mpsc::channel::<Result<Event, notify::Error>>();
    // Build on this thread: a construction failure (fd exhaustion for
    // inotify) reports to the caller instead of dying silently inside one.
    let mut watcher: Box<dyn notify::Watcher + Send> = match mode {
        Mode::Recommended => Box::new(
            RecommendedWatcher::new(tx, Config::default())
                .map_err(|e| format!("watch failed for {}: {}", root.display(), e))?,
        ),
        Mode::Poll(interval) => Box::new(
            PollWatcher::new(tx, Config::default().with_poll_interval(interval))
                .map_err(|e| format!("watch failed for {}: {}", root.display(), e))?,
        ),
    };
    if let Err(e) = watcher.watch(&root, notify::RecursiveMode::Recursive) {
        cx.watchers().0.lock().ok().map(|mut l| l.remove(root_id));
        return Err(format!("watch failed for {}: {}", root.display(), e));
    }
    let cx2 = cx.clone();
    let id = root_id.to_string();
    let thread_root = root.clone();
    std::thread::Builder::new()
        .name(format!("maleficium-watch-{root_id}"))
        .spawn(move || run_loop(&cx2, &id, &thread_root, &stop, rx, watcher))
        .map_err(|e| {
            cx.watchers().0.lock().ok().map(|mut l| l.remove(root_id));
            format!("watch failed for {}: {}", root.display(), e)
        })?;
    // The watcher keeps the index current from here on, so reads skip the
    // stat walk until it stops.
    let _ = super::index::set_watched(cx, root_id, true);
    Ok(())
}

/// Drain the queued external changes for `root_id`.
pub fn poll(cx: &Core, root_id: &str) -> Result<Vec<WatchEvent>, String> {
    super::fs::session_root(cx, root_id)?;
    Ok(cx
        .queues()
        .0
        .lock()
        .map_err(|_| "watch queue poisoned".to_string())?
        .remove(root_id)
        .unwrap_or_default()
        .into())
}

/// Stop tracking `root_id`. Idempotent: an untracked root reports success.
pub fn stop(cx: &Core, root_id: &str) -> Result<(), String> {
    if let Ok(mut live) = cx.watchers().0.lock() {
        // The flag outlives the registry entry: the thread exits on its
        // next tick even though no new batch will ever queue for this root.
        if let Some(stop) = live.remove(root_id) {
            stop.store(true, Ordering::Relaxed);
        }
    }
    let _ = super::index::set_watched(cx, root_id, false);
    Ok(())
}

fn run_loop(
    cx: &Core,
    root_id: &str,
    root: &Path,
    stop: &AtomicBool,
    rx: mpsc::Receiver<Result<Event, notify::Error>>,
    _watcher: Box<dyn notify::Watcher + Send>,
) {
    let mut pending: Vec<Event> = Vec::new();
    let mut last = std::time::Instant::now();
    loop {
        if stop.load(Ordering::Relaxed) {
            break;
        }
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(Ok(ev)) => {
                pending.push(ev);
                last = std::time::Instant::now();
            }
            Ok(Err(_)) => {}
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
        if !pending.is_empty() && last.elapsed() >= Duration::from_millis(DEBOUNCE_MS) {
            let batch = std::mem::take(&mut pending);
            process_batch(cx, root_id, root, batch);
        }
    }
}

/// Root-relative `/`-path for an absolute path under `root`, else `None`.
fn rel_of(root: &Path, abs: &Path) -> Option<String> {
    let rel = abs.strip_prefix(root).ok()?;
    let parts: Vec<String> = rel
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    (!parts.is_empty()).then(|| parts.join("/"))
}

fn change_of(kind: &EventKind) -> Option<WatchChange> {
    match kind {
        EventKind::Any | EventKind::Other => Some(WatchChange::Modify),
        EventKind::Create(_) => Some(WatchChange::Create),
        EventKind::Remove(_) => Some(WatchChange::Delete),
        EventKind::Modify(_) => Some(WatchChange::Modify),
        EventKind::Access(_) => None,
    }
}

fn process_batch(cx: &Core, root_id: &str, root: &Path, events: Vec<Event>) {
    // Collapse the burst to the latest change per path (order: first seen).
    let mut order: Vec<String> = Vec::new();
    let mut kinds: HashMap<String, WatchChange> = HashMap::new();
    let mut note = |rel: String, change: WatchChange| {
        if !kinds.contains_key(&rel) {
            order.push(rel.clone());
        }
        kinds.insert(rel, change);
    };
    for ev in &events {
        // A rename names both sides: the source left, the target arrived.
        // `touch` below is content-driven, so the kinds only tune the UI.
        if matches!(
            ev.kind,
            EventKind::Modify(ModifyKind::Name(RenameMode::Both))
        ) {
            let mut rels = ev.paths.iter().filter_map(|p| rel_of(root, p));
            match (rels.next(), rels.next()) {
                (Some(from), Some(to)) => {
                    note(from, WatchChange::Delete);
                    note(to, WatchChange::Create);
                }
                (Some(only), None) => {
                    note(only, WatchChange::Modify);
                }
                _ => {}
            }
            continue;
        }
        let Some(change) = change_of(&ev.kind) else {
            continue;
        };
        for rel in ev.paths.iter().filter_map(|p| rel_of(root, p)) {
            // A lone rename side still reads as what it is: a source that
            // no longer resolves, or a target that just appeared.
            let change = match (&ev.kind, &change) {
                (EventKind::Modify(ModifyKind::Name(RenameMode::From)), _) => WatchChange::Delete,
                (EventKind::Modify(ModifyKind::Name(RenameMode::To)), _) => WatchChange::Create,
                _ => change,
            };
            note(rel, change);
        }
    }
    if order.is_empty() {
        return;
    }
    // Every change, own writes included, is what the disk now holds.
    let abs: Vec<String> = order
        .iter()
        .map(|rel| root.join(rel).to_string_lossy().into_owned())
        .collect();
    let _ = super::index::touch(cx, root_id, &abs);
    let mut queued = match cx.queues().0.lock() {
        Ok(q) => q,
        Err(_) => return,
    };
    let queue = queued.entry(root_id.to_string()).or_default();
    for rel in order {
        let Some(change) = kinds.remove(&rel) else {
            continue;
        };
        // Echoes of our own writes: the disk still holds what we wrote.
        if is_echo(cx, &root.join(&rel)) {
            continue;
        }
        // Hidden paths (the poster cache, build outputs, version control)
        // are never shown, so a change there is nothing to react to.
        if rel.split('/').any(crate::is_hidden_name) {
            continue;
        }
        queue.push_back(WatchEvent { rel, change });
        while queue.len() > MAX_QUEUED {
            queue.pop_front();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn grant_tmp(cx: &Core, name: &str) -> (String, PathBuf) {
        let dir = crate::test_scratch::dir(&format!("watch-{}", name));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let canon = dunce::canonicalize(&dir).unwrap();
        let id = format!("w-{}", name);
        super::super::grant_root(cx, &id, &canon.to_string_lossy()).unwrap();
        (id, canon)
    }

    /// Drain `poll` until `want` appears or the budget runs out (watcher
    /// threads and OS event delivery are timing, not logic).
    fn poll_until(cx: &Core, id: &str, want: &str, tries: u32) -> Vec<WatchEvent> {
        for _ in 0..tries {
            let got = poll(cx, id).unwrap();
            if got.iter().any(|e| e.rel == want) {
                return got;
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        poll(cx, id).unwrap()
    }

    #[test]
    fn suppresses_echoes_of_a_save_however_many_arrive() {
        let cx = &Core::default();
        let (_id, dir) = grant_tmp(cx, "echo");
        let abs = dir.join("a.tex");
        std::fs::write(&abs, "hello").unwrap();
        mark_written(cx, &abs, b"hello");
        assert!(is_echo(cx, &abs));
        assert!(is_echo(cx, &abs));
    }

    #[test]
    fn reports_an_external_edit_made_right_after_a_save() {
        let cx = &Core::default();
        let (_id, dir) = grant_tmp(cx, "external");
        let abs = dir.join("a.tex");
        std::fs::write(&abs, "mine").unwrap();
        mark_written(cx, &abs, b"mine");
        std::fs::write(&abs, "theirs").unwrap();
        assert!(!is_echo(cx, &abs));
        // The record is gone: later events on this path are external too.
        std::fs::write(&abs, "mine").unwrap();
        assert!(!is_echo(cx, &abs));
    }

    #[test]
    fn never_treats_an_unrecorded_path_as_an_echo() {
        let cx = &Core::default();
        let (_id, dir) = grant_tmp(cx, "unrecorded");
        let abs = dir.join("b.tex");
        std::fs::write(&abs, "x").unwrap();
        assert!(!is_echo(cx, &abs));
    }

    #[test]
    fn matches_a_removal_against_an_absent_path() {
        let cx = &Core::default();
        let (_id, dir) = grant_tmp(cx, "removal");
        let abs = dir.join("old.tex");
        std::fs::write(&abs, "x").unwrap();
        std::fs::remove_file(&abs).unwrap();
        mark_removed(cx, &abs);
        assert!(is_echo(cx, &abs));
        std::fs::write(&abs, "recreated elsewhere").unwrap();
        assert!(!is_echo(cx, &abs));
    }

    #[test]
    fn records_a_settled_path_from_what_the_disk_holds() {
        let cx = &Core::default();
        let (_id, dir) = grant_tmp(cx, "settled");
        let abs = dir.join("new.tex");
        std::fs::write(&abs, "moved content").unwrap();
        mark_settled(cx, &abs);
        assert!(is_echo(cx, &abs));
    }

    #[test]
    fn save_marks_its_own_write() {
        let cx = &Core::default();
        let (id, _dir) = grant_tmp(cx, "save-marks");
        super::super::save(cx, &id, "a.tex", b"hello", None).unwrap();
        let root = super::super::session_root(cx, &id).unwrap();
        assert!(is_echo(cx, &root.join("a.tex")));
        std::fs::write(root.join("a.tex"), "theirs").unwrap();
        assert!(!is_echo(cx, &root.join("a.tex")));
    }

    #[test]
    fn watcher_queues_external_writes_and_feeds_the_index() {
        let cx = &Core::default();
        let (id, dir) = grant_tmp(cx, "live");
        start_with_mode(cx, &id, Mode::Recommended).unwrap();
        // Idempotent: a second start neither errors nor duplicates.
        start_with_mode(cx, &id, Mode::Recommended).unwrap();
        std::fs::write(dir.join("a.tex"), "one").unwrap();
        let got = poll_until(cx, &id, "a.tex", 25);
        assert!(
            got.iter().any(|e| e.rel == "a.tex"),
            "no event for a.tex in {got:?}"
        );
        super::super::index::with(cx, &id, |l| {
            assert_eq!(l.index.get("a.tex").unwrap().text, Some("one"));
        })
        .unwrap();
        stop(cx, &id).unwrap();
        // Idempotent stop.
        stop(cx, &id).unwrap();
    }

    #[test]
    fn watcher_never_queues_poster_cache_writes() {
        let cx = &Core::default();
        let (id, dir) = grant_tmp(cx, "hidden");
        start_with_mode(cx, &id, Mode::Recommended).unwrap();
        std::thread::sleep(Duration::from_millis(500));
        std::fs::create_dir_all(dir.join(".maleficium/posters")).unwrap();
        std::fs::write(dir.join(".maleficium/posters/a.png"), "png").unwrap();
        std::fs::write(dir.join("seen.tex"), "x").unwrap();
        let mut got = Vec::new();
        for _ in 0..25 {
            got.extend(poll(cx, &id).unwrap());
            if got.iter().any(|e| e.rel == "seen.tex") {
                break;
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        assert!(got.iter().any(|e| e.rel == "seen.tex"), "{got:?}");
        assert!(
            !got.iter().any(|e| e.rel.starts_with(".maleficium")),
            "cache write queued in {got:?}"
        );
        stop(cx, &id).unwrap();
    }

    #[test]
    fn watcher_hides_own_saves_but_keeps_the_index_current() {
        let cx = &Core::default();
        let (id, dir) = grant_tmp(cx, "own");
        start_with_mode(cx, &id, Mode::Recommended).unwrap();
        // Let the watcher thread build its watch before the save lands.
        std::thread::sleep(Duration::from_millis(500));
        super::super::save(cx, &id, "b.tex", b"mine", None).unwrap();
        // Drain for longer than the batch window: an echo would arrive here.
        std::thread::sleep(Duration::from_millis(1200));
        let got = poll(cx, &id).unwrap();
        assert!(
            !got.iter().any(|e| e.rel == "b.tex"),
            "own save queued in {got:?}"
        );
        // ... while the index still learned what the disk now holds.
        super::super::index::with(cx, &id, |l| {
            assert_eq!(l.index.get("b.tex").unwrap().text, Some("mine"));
        })
        .unwrap();
        stop(cx, &id).unwrap();
        let _ = dir;
    }

    #[test]
    fn poll_mode_watches_without_the_os_backend() {
        let cx = &Core::default();
        let (id, dir) = grant_tmp(cx, "pollmode");
        start_with_mode(cx, &id, Mode::Poll(Duration::from_millis(100))).unwrap();
        std::fs::write(dir.join("p.tex"), "polled").unwrap();
        let got = poll_until(cx, &id, "p.tex", 30);
        assert!(
            got.iter().any(|e| e.rel == "p.tex"),
            "no event for p.tex in {got:?}"
        );
        stop(cx, &id).unwrap();
    }

    #[test]
    fn poll_interval_comes_from_the_environment() {
        assert!(matches!(mode_from_env(), Mode::Recommended));
    }

    #[test]
    fn start_rejects_unknown_roots() {
        let cx = &Core::default();
        assert!(start(cx, "no-such-root").is_err());
        assert!(poll(cx, "no-such-root").is_err());
    }
}
