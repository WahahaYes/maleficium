//! App-local revision history: content-addressed blobs plus a per-project
//! index, keyed by `(project, relPath)`. Snapshots are taken on save and
//! before any write that replaces a file's content (restore, replace), so
//! the list is always the way back. Nothing is written to the project dir.
//!
//! One store serves the app and the MCP server; an advisory lock on the
//! project's history dir serializes writers across processes.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use maleficium_events::{BatchFile, RecordOutcome, RetentionInfo, Revision, RevisionSkipReason};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Hard cap per file. Oldest revisions are evicted first.
pub const MAX_REVISIONS_PER_FILE: usize = 50;
/// Soft cap on summed distinct blob bytes for one project.
pub const MAX_HISTORY_BYTES_PER_PROJECT: u64 = 256 * 1024 * 1024;
/// Byte-cap eviction never takes a file below this many revisions.
pub const MIN_REVISIONS_KEPT_PER_FILE: usize = 5;
/// Files larger than this are not snapshotted.
pub const SNAPSHOT_MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;
/// Replace batches remembered per project, newest kept.
pub const MAX_BATCHES: usize = 20;

const FORMAT: u32 = 2;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Entry {
    rev: String,
    hash: String,
    at: u64,
    bytes: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    batch: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Batch {
    at: u64,
    files: Vec<BatchFile>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Index {
    v: u32,
    seq: u64,
    files: BTreeMap<String, Vec<Entry>>,
    #[serde(default)]
    batches: BTreeMap<String, Batch>,
}

impl Index {
    fn empty() -> Self {
        Index {
            v: FORMAT,
            seq: 0,
            files: BTreeMap::new(),
            batches: BTreeMap::new(),
        }
    }
}

/// Parse a stored index. Another format version is not read (starts empty);
/// a damaged index of this version is an error, so no caller replaces
/// history it could not read.
fn parse_index(raw: &str) -> Result<Index, String> {
    let j: serde_json::Value =
        serde_json::from_str(raw).map_err(|_| "history index is damaged".to_string())?;
    if j.get("v").and_then(|v| v.as_u64()) != Some(FORMAT as u64) {
        return Ok(Index::empty());
    }
    serde_json::from_value(j).map_err(|_| "history index is damaged".to_string())
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn hash_bytes(bytes: &[u8]) -> String {
    let d = Sha256::digest(bytes);
    d.iter().map(|b| format!("{:02x}", b)).collect()
}

fn distinct_bytes(index: &Index) -> u64 {
    let mut seen: HashMap<&str, u64> = HashMap::new();
    for list in index.files.values() {
        for e in list {
            seen.insert(&e.hash, e.bytes);
        }
    }
    seen.values().sum()
}

fn live_hashes(index: &Index) -> HashSet<String> {
    index
        .files
        .values()
        .flatten()
        .map(|e| e.hash.clone())
        .collect()
}

/// Evict to the caps: per-file first, then project bytes oldest-first across
/// files, never taking a file below the floor.
fn evict(index: &mut Index) {
    for list in index.files.values_mut() {
        if list.len() > MAX_REVISIONS_PER_FILE {
            list.drain(..list.len() - MAX_REVISIONS_PER_FILE);
        }
    }
    while distinct_bytes(index) > MAX_HISTORY_BYTES_PER_PROJECT {
        let oldest = index
            .files
            .iter()
            .filter(|(_, l)| l.len() > MIN_REVISIONS_KEPT_PER_FILE)
            .filter_map(|(rel, l)| l.first().map(|e| (e.at, rel.clone())))
            .min();
        match oldest {
            Some((_, rel)) => {
                index.files.get_mut(&rel).map(|l| l.remove(0));
            }
            None => return,
        }
    }
    while index.batches.len() > MAX_BATCHES {
        let oldest = index
            .batches
            .iter()
            .min_by_key(|(_, b)| b.at)
            .map(|(k, _)| k.clone());
        if let Some(k) = oldest {
            index.batches.remove(&k);
        }
    }
}

/// Why a path cannot be snapshotted, if it cannot.
fn ineligible(rel: &str, len: u64) -> Option<RevisionSkipReason> {
    if !maleficium_structure::is_text_path(rel) {
        return Some(RevisionSkipReason::NotText);
    }
    if len > SNAPSHOT_MAX_FILE_BYTES {
        return Some(RevisionSkipReason::TooLarge);
    }
    None
}

fn skipped(reason: RevisionSkipReason) -> RecordOutcome {
    RecordOutcome {
        stored: false,
        rev: None,
        deduped: None,
        reason: Some(reason),
    }
}

/// One project's history home.
pub struct Store {
    dir: PathBuf,
}

/// Holds the project's history lock until dropped.
struct Locked {
    _file: std::fs::File,
}

impl Store {
    /// The store for a project root: `<app data>/maleficium-history/<id>`.
    pub fn for_root(root: &Path) -> Store {
        Store::at(
            super::data_base_dir()
                .join("maleficium-history")
                .join(super::hash_root(&root.to_string_lossy())),
        )
    }

    pub fn at(dir: PathBuf) -> Store {
        Store { dir }
    }

    fn index_path(&self) -> PathBuf {
        self.dir.join("index.json")
    }

    fn blob_path(&self, hash: &str) -> PathBuf {
        self.dir.join("blobs").join(&hash[..2]).join(hash)
    }

    fn lock(&self) -> Result<Locked, String> {
        std::fs::create_dir_all(&self.dir)
            .map_err(|e| format!("history dir unavailable: {}", e))?;
        let f = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(self.dir.join("lock"))
            .map_err(|e| format!("history lock unavailable: {}", e))?;
        f.lock()
            .map_err(|e| format!("history lock unavailable: {}", e))?;
        Ok(Locked { _file: f })
    }

    /// Missing means no history yet; anything unreadable is an error.
    fn read_index(&self) -> Result<Index, String> {
        match std::fs::read_to_string(self.index_path()) {
            Ok(raw) => parse_index(&raw),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Index::empty()),
            Err(e) => Err(format!("history index unreadable: {}", e)),
        }
    }

    fn write_index(&self, index: &Index) -> Result<(), String> {
        let tmp = self.dir.join("index.json.tmp");
        let body = serde_json::to_vec(index).map_err(|e| e.to_string())?;
        let mut f = std::fs::File::create(&tmp).map_err(|e| e.to_string())?;
        f.write_all(&body).map_err(|e| e.to_string())?;
        f.sync_all().map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, self.index_path()).map_err(|e| e.to_string())
    }

    /// Delete blobs among `touched` that no revision references any more.
    fn sweep(&self, index: &Index, touched: &HashSet<String>) {
        let live = live_hashes(index);
        for h in touched.difference(&live) {
            let _ = std::fs::remove_file(self.blob_path(h));
        }
    }

    /// Add one revision to `index` (blob written if new). `None` when the
    /// content equals the file's latest revision.
    fn add(
        &self,
        index: &mut Index,
        rel: &str,
        bytes: &[u8],
        batch: Option<&str>,
    ) -> Result<Option<(String, bool)>, String> {
        let hash = hash_bytes(bytes);
        let list = index.files.entry(rel.to_string()).or_default();
        if list.last().is_some_and(|e| e.hash == hash) {
            return Ok(None);
        }
        let blob = self.blob_path(&hash);
        let deduped = blob.exists();
        if !deduped {
            std::fs::create_dir_all(blob.parent().unwrap()).map_err(|e| e.to_string())?;
            std::fs::write(&blob, bytes).map_err(|e| e.to_string())?;
        }
        index.seq += 1;
        let rev = index.seq.to_string();
        list.push(Entry {
            rev: rev.clone(),
            hash,
            at: now_ms(),
            bytes: bytes.len() as u64,
            batch: batch.map(str::to_string),
        });
        Ok(Some((rev, deduped)))
    }

    /// Evict, persist, and drop blobs the eviction orphaned.
    fn commit(&self, mut index: Index) -> Result<(), String> {
        let before = live_hashes(&index);
        evict(&mut index);
        self.write_index(&index)?;
        self.sweep(&index, &before);
        Ok(())
    }

    /// Snapshot one file's content.
    pub fn record(&self, rel: &str, bytes: &[u8]) -> RecordOutcome {
        if let Some(r) = ineligible(rel, bytes.len() as u64) {
            return skipped(r);
        }
        let Ok(_lock) = self.lock() else {
            return skipped(RevisionSkipReason::Unavailable);
        };
        let Ok(mut index) = self.read_index() else {
            return skipped(RevisionSkipReason::IndexUnreadable);
        };
        match self.add(&mut index, rel, bytes, None) {
            Ok(None) => skipped(RevisionSkipReason::Unchanged),
            Ok(Some((rev, deduped))) => match self.commit(index) {
                Ok(()) => RecordOutcome {
                    stored: true,
                    rev: Some(rev),
                    deduped: Some(deduped),
                    reason: None,
                },
                Err(_) => skipped(RevisionSkipReason::Unavailable),
            },
            Err(_) => skipped(RevisionSkipReason::Unavailable),
        }
    }

    /// Snapshot the prior content of every file a replace will change, as one
    /// batch. Refuses the whole batch when any file cannot be snapshotted.
    pub fn record_batch(&self, files: &[(String, Vec<u8>)]) -> Result<String, String> {
        for (rel, bytes) in files {
            if let Some(r) = ineligible(rel, bytes.len() as u64) {
                return Err(format!("{} cannot be kept in history ({:?})", rel, r));
            }
        }
        let _lock = self.lock()?;
        let mut index = self.read_index()?;
        let id = format!("b{}", index.seq + 1);
        let mut members = Vec::new();
        for (rel, bytes) in files {
            let rev = match self.add(&mut index, rel, bytes, Some(&id))? {
                Some((rev, _)) => rev,
                None => index.files[rel]
                    .last()
                    .map(|e| e.rev.clone())
                    .unwrap_or_default(),
            };
            members.push(BatchFile {
                rel: rel.clone(),
                rev,
            });
        }
        index.batches.insert(
            id.clone(),
            Batch {
                at: now_ms(),
                files: members,
            },
        );
        self.commit(index)?;
        Ok(id)
    }

    /// Revisions of one file, newest first. No readable history lists none.
    pub fn list(&self, rel: &str) -> Vec<Revision> {
        let Ok(index) = self.read_index() else {
            return Vec::new();
        };
        index
            .files
            .get(rel)
            .map(|l| {
                l.iter()
                    .rev()
                    .map(|e| Revision {
                        rev: e.rev.clone(),
                        at: e.at,
                        bytes: e.bytes,
                        batch: e.batch.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Exact bytes of one revision, if it and its blob are readable.
    pub fn get(&self, rel: &str, rev: &str) -> Option<Vec<u8>> {
        let index = self.read_index().ok()?;
        let e = index.files.get(rel)?.iter().find(|e| e.rev == rev)?;
        std::fs::read(self.blob_path(&e.hash)).ok()
    }

    /// The files of one replace batch; empty for an unknown or evicted batch.
    pub fn batch_files(&self, batch: &str) -> Vec<BatchFile> {
        self.read_index()
            .ok()
            .and_then(|i| i.batches.get(batch).map(|b| b.files.clone()))
            .unwrap_or_default()
    }

    pub fn retention(&self) -> RetentionInfo {
        let (revisions, bytes) = match self.read_index() {
            Ok(i) => (
                i.files.values().map(Vec::len).sum::<usize>() as u32,
                distinct_bytes(&i),
            ),
            Err(_) => (0, 0),
        };
        RetentionInfo {
            max_revisions_per_file: MAX_REVISIONS_PER_FILE as u32,
            max_history_bytes_per_project: MAX_HISTORY_BYTES_PER_PROJECT,
            min_revisions_kept_per_file: MIN_REVISIONS_KEPT_PER_FILE as u32,
            snapshot_max_file_bytes: SNAPSHOT_MAX_FILE_BYTES,
            revisions,
            bytes,
        }
    }
}

/// Write a revision back to `abs`, snapshotting what is on disk first so the
/// restore is itself undoable. Returns the restored bytes.
pub fn restore_to(store: &Store, rel: &str, rev: &str, abs: &Path) -> Option<Vec<u8>> {
    let bytes = store.get(rel, rev)?;
    if let Ok(current) = std::fs::read(abs) {
        store.record(rel, &current);
    }
    std::fs::write(abs, &bytes).ok()?;
    Some(bytes)
}

// Session-root adapters: the project is named by its session root id, paths
// are root-relative.

fn store_of(root_id: &str) -> Result<Store, String> {
    Ok(Store::for_root(&super::session_root(root_id)?))
}

pub fn record(root_id: &str, rel: &str, bytes: &[u8]) -> RecordOutcome {
    match store_of(root_id) {
        Ok(s) => s.record(rel, bytes),
        Err(_) => skipped(RevisionSkipReason::Unavailable),
    }
}

pub fn record_batch(root_id: &str, files: &[(String, Vec<u8>)]) -> Result<String, String> {
    store_of(root_id)?.record_batch(files)
}

pub fn list(root_id: &str, rel: &str) -> Vec<Revision> {
    store_of(root_id).map(|s| s.list(rel)).unwrap_or_default()
}

pub fn get(root_id: &str, rel: &str, rev: &str) -> Option<Vec<u8>> {
    store_of(root_id).ok()?.get(rel, rev)
}

pub fn restore(root_id: &str, rel: &str, rev: &str) -> Option<Vec<u8>> {
    let store = store_of(root_id).ok()?;
    let abs = super::resolve_read(root_id, rel).ok()?;
    restore_to(&store, rel, rev, &abs)
}

pub fn batch_files(root_id: &str, batch: &str) -> Vec<BatchFile> {
    store_of(root_id)
        .map(|s| s.batch_files(batch))
        .unwrap_or_default()
}

/// Put every file of a replace batch back on disk. Returns the files
/// restored; a file whose revision was evicted is left as it is.
pub fn restore_batch(root_id: &str, batch: &str) -> Result<Vec<BatchFile>, String> {
    let store = store_of(root_id)?;
    let files = store.batch_files(batch);
    if files.is_empty() {
        return Err(format!("unknown replace batch: {}", batch));
    }
    let mut done = Vec::new();
    for f in files {
        let abs = super::resolve_read(root_id, &f.rel)?;
        if restore_to(&store, &f.rel, &f.rev, &abs).is_some() {
            done.push(f);
        }
    }
    Ok(done)
}

pub fn retention(root_id: &str) -> RetentionInfo {
    match store_of(root_id) {
        Ok(s) => s.retention(),
        Err(_) => Store::at(PathBuf::new()).retention(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_store(name: &str) -> (Store, PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "maleficium-history-{}-{}",
            std::process::id(),
            name
        ));
        let _ = std::fs::remove_dir_all(&dir);
        (Store::at(dir.join("history")), dir)
    }

    fn entry(rev: u64, hash: &str, at: u64, bytes: u64) -> Entry {
        Entry {
            rev: rev.to_string(),
            hash: hash.to_string(),
            at,
            bytes,
            batch: None,
        }
    }

    #[test]
    fn stores_and_lists_newest_first() {
        let (s, _) = tmp_store("list");
        assert!(s.record("main.tex", b"one").stored);
        assert!(s.record("main.tex", b"two").stored);
        let revs: Vec<String> = s.list("main.tex").into_iter().map(|r| r.rev).collect();
        assert_eq!(revs, ["2", "1"]);
        assert_eq!(s.get("main.tex", "1").unwrap(), b"one");
    }

    #[test]
    fn skips_unchanged_and_reuses_blobs() {
        let (s, _) = tmp_store("dedupe");
        s.record("a.tex", b"x");
        assert_eq!(
            s.record("a.tex", b"x").reason,
            Some(RevisionSkipReason::Unchanged)
        );
        s.record("a.tex", b"y");
        let back = s.record("a.tex", b"x");
        assert_eq!(back.deduped, Some(true));
        let other = s.record("b.tex", b"x");
        assert_eq!(other.deduped, Some(true));
        assert_eq!(s.retention().bytes, 2);
    }

    #[test]
    fn refuses_ineligible_and_writes_nothing() {
        let (s, dir) = tmp_store("ineligible");
        assert_eq!(
            s.record("fig.png", b"x").reason,
            Some(RevisionSkipReason::NotText)
        );
        let big = vec![b'a'; SNAPSHOT_MAX_FILE_BYTES as usize + 1];
        assert_eq!(
            s.record("big.tex", &big).reason,
            Some(RevisionSkipReason::TooLarge)
        );
        assert!(!dir.exists());
    }

    #[test]
    fn keeps_exact_bytes_that_are_not_utf8() {
        let (s, _) = tmp_store("bytes");
        let raw = [0xff, 0xfe, b'a', 0x00];
        s.record("x.tex", &raw);
        assert_eq!(s.get("x.tex", "1").unwrap(), raw);
    }

    #[test]
    fn refuses_to_replace_a_damaged_index() {
        let (s, _) = tmp_store("damaged");
        s.record("a.tex", b"x");
        std::fs::write(s.index_path(), r#"{"v":2,"seq":"#).unwrap();
        assert_eq!(
            s.record("a.tex", b"y").reason,
            Some(RevisionSkipReason::IndexUnreadable)
        );
        assert_eq!(
            std::fs::read_to_string(s.index_path()).unwrap(),
            r#"{"v":2,"seq":"#
        );
    }

    #[test]
    fn another_format_version_starts_empty() {
        let (s, _) = tmp_store("version");
        std::fs::create_dir_all(&s.dir).unwrap();
        std::fs::write(s.index_path(), r#"{"v":1,"seq":3,"files":{}}"#).unwrap();
        assert!(s.list("a.tex").is_empty());
        assert!(s.record("a.tex", b"x").stored);
    }

    #[test]
    fn caps_revisions_per_file_oldest_first() {
        let mut i = Index::empty();
        let list = (0..MAX_REVISIONS_PER_FILE as u64 + 3)
            .map(|n| entry(n, &format!("h{}", n), n, 1))
            .collect();
        i.files.insert("a.tex".into(), list);
        evict(&mut i);
        let l = &i.files["a.tex"];
        assert_eq!(l.len(), MAX_REVISIONS_PER_FILE);
        assert_eq!(l[0].rev, "3");
    }

    #[test]
    fn byte_cap_evicts_oldest_across_files_but_keeps_the_floor() {
        let tenth = MAX_HISTORY_BYTES_PER_PROJECT / 10;
        let mut i = Index::empty();
        let a = (0..7)
            .map(|n| entry(n, &format!("a{}", n), n, tenth))
            .collect();
        let b = (0..6)
            .map(|n| entry(10 + n, &format!("b{}", n), 100 + n, tenth))
            .collect();
        i.files.insert("a.tex".into(), a);
        i.files.insert("b.tex".into(), b);
        evict(&mut i);
        assert!(distinct_bytes(&i) <= MAX_HISTORY_BYTES_PER_PROJECT);
        // `a` is older, so it gives up revisions until it reaches the floor.
        assert_eq!(i.files["a.tex"].len(), MIN_REVISIONS_KEPT_PER_FILE);
        assert_eq!(i.files["a.tex"][0].rev, "2");
        assert_eq!(i.files["b.tex"].len(), 5);
        assert_eq!(i.files["b.tex"][0].rev, "11");

        let mut floor = Index::empty();
        let big = (0..MIN_REVISIONS_KEPT_PER_FILE as u64)
            .map(|n| entry(n, &format!("f{}", n), n, MAX_HISTORY_BYTES_PER_PROJECT))
            .collect();
        floor.files.insert("f.tex".into(), big);
        evict(&mut floor);
        assert_eq!(floor.files["f.tex"].len(), MIN_REVISIONS_KEPT_PER_FILE);
    }

    #[test]
    fn eviction_deletes_orphaned_blobs() {
        let (s, _) = tmp_store("sweep");
        for n in 0..MAX_REVISIONS_PER_FILE + 1 {
            s.record("a.tex", format!("v{}", n).as_bytes());
        }
        let first = s.blob_path(&hash_bytes(b"v0"));
        assert!(!first.exists());
        assert!(s.blob_path(&hash_bytes(b"v1")).exists());
    }

    #[test]
    fn restore_snapshots_current_content_first() {
        let (s, dir) = tmp_store("restore");
        let abs = dir.join("main.tex");
        std::fs::create_dir_all(&dir).unwrap();
        s.record("main.tex", b"old");
        std::fs::write(&abs, b"new unsaved-to-history").unwrap();
        assert_eq!(restore_to(&s, "main.tex", "1", &abs).unwrap(), b"old");
        assert_eq!(std::fs::read(&abs).unwrap(), b"old");
        let prior = &s.list("main.tex")[0];
        assert_eq!(
            s.get("main.tex", &prior.rev).unwrap(),
            b"new unsaved-to-history"
        );
        assert!(restore_to(&s, "main.tex", "99", &abs).is_none());
    }

    #[test]
    fn batch_records_prior_content_and_restores_every_file() {
        let (s, dir) = tmp_store("batch");
        std::fs::create_dir_all(&dir).unwrap();
        s.record("a.tex", b"a0");
        let files = vec![
            ("a.tex".to_string(), b"a0".to_vec()),
            ("b.tex".to_string(), b"b0".to_vec()),
        ];
        let id = s.record_batch(&files).unwrap();
        let members = s.batch_files(&id);
        assert_eq!(members.len(), 2);
        assert_eq!(
            members[0].rev, "1",
            "unchanged content reuses the latest revision"
        );
        assert_eq!(s.list("b.tex")[0].batch.as_deref(), Some(id.as_str()));
        for f in &members {
            let abs = dir.join(&f.rel);
            std::fs::write(&abs, b"replaced").unwrap();
            restore_to(&s, &f.rel, &f.rev, &abs).unwrap();
        }
        assert_eq!(std::fs::read(dir.join("a.tex")).unwrap(), b"a0");
        assert_eq!(std::fs::read(dir.join("b.tex")).unwrap(), b"b0");
    }

    #[test]
    fn batch_refuses_when_any_file_is_ineligible() {
        let (s, _) = tmp_store("batch-refuse");
        let files = vec![
            ("a.tex".to_string(), b"x".to_vec()),
            ("p.png".to_string(), b"y".to_vec()),
        ];
        assert!(s.record_batch(&files).is_err());
        assert!(s.list("a.tex").is_empty());
    }

    #[test]
    fn keeps_the_newest_batches() {
        let (s, _) = tmp_store("batch-cap");
        let mut ids = Vec::new();
        for n in 0..MAX_BATCHES + 2 {
            ids.push(
                s.record_batch(&[("a.tex".to_string(), format!("{}", n).into_bytes())])
                    .unwrap(),
            );
        }
        assert!(s.batch_files(&ids[0]).is_empty());
        assert_eq!(s.batch_files(ids.last().unwrap()).len(), 1);
    }

    #[test]
    fn writes_only_under_the_history_home() {
        let (s, dir) = tmp_store("footprint");
        s.record("a.tex", b"x");
        let names: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names, [std::ffi::OsString::from("history")]);
    }
}
