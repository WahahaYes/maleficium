//! The Tectonic engine: the pinned bundle, the app-owned cache it resolves
//! into, and the one way a compile spawns the sidecar and pumps its output.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex, OnceLock};

use maleficium_events::{CompileLine, CompileStream};
use maleficium_structure::{
    self as ms, CompilePhase, LineSignal, MissingDependency, MissingReason,
};

use super::{JobOutcome, JobStatus, MainOutputs};

/// The bundle every compile resolves against: an immutable direct URL, never
/// the relay redirect, so an upstream rotation cannot move it.
pub const BUNDLE_URL: &str = "https://data1b.fullyjustified.net/tlextras-2022.0r0.tar";

/// SHA-256 digest `BUNDLE_URL` must resolve to.
pub const BUNDLE_DIGEST: &str = "6ffe055852f8faf66c0acbe1a7fb27f87b869a90bad1204f3bf4d9683f597c7c";

/// The engine's cache for the pinned bundle: app-owned, beside the output
/// shards, one directory per digest so its size can be reported and cleared.
pub fn cache_dir() -> PathBuf {
    super::out_base_dir()
        .join("maleficium-tectonic")
        .join(BUNDLE_DIGEST)
}

/// The engine's file name for a bundle URL: every byte outside
/// `[A-Za-z0-9._-]` becomes `,<decimal byte>,`.
fn url_key(url: &str) -> String {
    let mut out = String::with_capacity(url.len() + 16);
    for b in url.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_') {
            out.push(b as char);
        } else {
            out.push_str(&format!(",{},", b));
        }
    }
    out
}

/// The digest the engine recorded when it resolved `url` into `cache`, if it
/// ever did.
pub fn resolved_digest(cache: &Path, url: &str) -> Option<String> {
    let text =
        std::fs::read_to_string(cache.join("bundles").join("hashes").join(url_key(url))).ok()?;
    let digest = text.trim();
    (!digest.is_empty()).then(|| digest.to_string())
}

/// How the cache stands against the pin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DigestCheck {
    /// The bundle was never resolved into this cache.
    Unresolved,
    Pinned,
    /// The URL now resolves to a different digest.
    Changed(String),
}

pub fn check_digest(cache: &Path) -> DigestCheck {
    match resolved_digest(cache, BUNDLE_URL) {
        None => DigestCheck::Unresolved,
        Some(d) if d == BUNDLE_DIGEST => DigestCheck::Pinned,
        Some(d) => DigestCheck::Changed(d),
    }
}

/// The cached listing of every file in the pinned bundle: one name per line,
/// ahead of its offset and length.
pub fn index_path(cache: &Path) -> PathBuf {
    cache
        .join("bundles")
        .join("data")
        .join(format!("{BUNDLE_DIGEST}.index"))
}

/// One index file version (path, size, mtime) and the names it lists.
type IndexMemo = (
    (PathBuf, u64, Option<std::time::SystemTime>),
    Arc<HashSet<String>>,
);
static INDEX: OnceLock<Mutex<Option<IndexMemo>>> = OnceLock::new();

/// The names in the cached bundle index, read once per index file version;
/// `None` until the bundle has been resolved.
pub fn bundle_files(cache: &Path) -> Option<Arc<HashSet<String>>> {
    let path = index_path(cache);
    let meta = std::fs::metadata(&path).ok()?;
    let key = (path.clone(), meta.len(), meta.modified().ok());
    let mut memo = INDEX.get_or_init(|| Mutex::new(None)).lock().unwrap();
    if let Some((k, names)) = memo.as_ref() {
        if *k == key {
            return Some(names.clone());
        }
    }
    let text = std::fs::read_to_string(&path).ok()?;
    let names: Arc<HashSet<String>> = Arc::new(
        text.lines()
            .filter_map(|l| l.split(' ').next())
            .filter(|n| !n.is_empty())
            .map(str::to_string)
            .collect(),
    );
    *memo = Some((key, names.clone()));
    Some(names)
}

/// Whether this machine has a route off-host. A UDP connect is a local
/// routing-table lookup: no packet leaves, so the app makes no network call.
pub fn online() -> bool {
    use std::net::UdpSocket;
    let probe = |bind: &str, to: &str| UdpSocket::bind(bind).and_then(|s| s.connect(to)).is_ok();
    probe("0.0.0.0:0", "192.0.2.1:443") || probe("[::]:0", "[2001:db8::1]:443")
}

/// Whether the engine may fetch: `CachedOnly` passes `-C`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheMode {
    CachedOnly,
    Online,
}

/// The engine's command line for one main file.
fn command(out: &MainOutputs, cache: &Path, mode: CacheMode) -> Result<Command, String> {
    let mut cmd = Command::new(super::sidecar_path_for("tectonic")?);
    cmd.args(["-X", "compile", &out.main_file, "--outdir"])
        .arg(&out.outdir)
        .args(["--synctex", "-b", BUNDLE_URL]);
    if mode == CacheMode::CachedOnly {
        cmd.arg("-C");
    }
    cmd.env("TECTONIC_CACHE_DIR", cache)
        .current_dir(&out.dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    Ok(cmd)
}

/// How a compile ended: the last engine run's status and lines (nothing
/// ran when the flow stopped before spawning), and the dependency it
/// lacked. `missing` is `bundle-changed` beside an otherwise clean result
/// when the pin resolved to another digest.
pub struct Compiled {
    pub status: JobStatus,
    pub lines: Vec<CompileLine>,
    pub missing: Option<MissingDependency>,
    /// The last run used only cached files.
    pub cached_only: bool,
}

impl Compiled {
    pub fn texts(&self) -> Vec<String> {
        self.lines.iter().map(|l| l.text.clone()).collect()
    }
}

fn status_line(text: String) -> CompileLine {
    CompileLine {
        stream: CompileStream::Status,
        text,
        signal: None,
    }
}

/// Compile offline-first. With nothing resolved, compile online, or stop
/// with `cache-empty` before spawning when there is no network. Otherwise
/// compile from the cache alone; a file the cache lacks is fetched by one
/// online rerun when there is network, else reported `not-cached`. A file
/// the bundle does not carry is reported `not-in-bundle` without fetching.
/// `networked` (with network) compiles online first, then proves the result
/// by compiling again from the cache alone.
pub fn compile(
    out: &MainOutputs,
    slot: &Mutex<Option<Child>>,
    timeout_secs: u64,
    network: bool,
    networked: bool,
    on_line: &mut dyn FnMut(&CompileLine),
) -> Result<Compiled, String> {
    let cache = cache_dir();
    std::fs::create_dir_all(&cache).map_err(|e| format!("engine cache unreachable: {}", e))?;
    std::fs::create_dir_all(&out.outdir).map_err(|e| format!("outdir unreachable: {}", e))?;

    let mut mode = CacheMode::CachedOnly;
    let mut verify = networked && network;
    if verify {
        on_line(&status_line(String::from(
            "making available offline: fetching everything this document needs",
        )));
        mode = CacheMode::Online;
    } else if check_digest(&cache) == DigestCheck::Unresolved {
        if !network {
            on_line(&status_line(String::from(
                "no TeX support files are cached yet and there is no network",
            )));
            return Ok(Compiled {
                status: JobStatus::Failed,
                lines: Vec::new(),
                missing: Some(MissingDependency {
                    file: None,
                    reason: MissingReason::CacheEmpty,
                }),
                cached_only: true,
            });
        }
        on_line(&CompileLine {
            signal: Some(LineSignal::Phase {
                phase: CompilePhase::FirstCompile,
                detail: None,
            }),
            ..status_line(String::from("first compile: downloading TeX support files"))
        });
        mode = CacheMode::Online;
    }
    loop {
        let (status, lines) = run(out, &cache, mode, slot, timeout_secs, on_line)?;
        let in_bundle = |f: &str| bundle_files(&cache).is_none_or(|names| names.contains(f));
        let mut missing = match status {
            JobStatus::Failed => ms::missing_dependency(
                &lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>(),
                false,
                &in_bundle,
            ),
            _ => None,
        };
        let refetch = matches!(
            missing.as_ref().map(|m| m.reason),
            Some(MissingReason::NotCached | MissingReason::CacheEmpty)
        );
        if verify && mode == CacheMode::Online && status == JobStatus::Success {
            on_line(&status_line(String::from(
                "verifying offline: compiling from cached files only",
            )));
            verify = false;
            mode = CacheMode::CachedOnly;
            continue;
        }
        if mode == CacheMode::CachedOnly && refetch && network {
            let what = missing
                .as_ref()
                .and_then(|m| m.file.clone())
                .unwrap_or_else(|| String::from("TeX support files"));
            on_line(&status_line(format!("fetching {what}: not cached yet")));
            mode = CacheMode::Online;
            continue;
        }
        if missing.is_none() {
            if let DigestCheck::Changed(d) = check_digest(&cache) {
                on_line(&status_line(format!(
                    "TeX bundle changed: {BUNDLE_URL} now resolves to {d}"
                )));
                missing = Some(MissingDependency {
                    file: None,
                    reason: MissingReason::BundleChanged,
                });
            }
        }
        return Ok(Compiled {
            status,
            lines,
            missing,
            cached_only: mode == CacheMode::CachedOnly,
        });
    }
}

/// Spawn the engine, park the child in `slot` (whoever takes it from there
/// owns reaping it: a cancel, or this call once output ends), hand each line
/// to `on_line` as it arrives, then wait up to `timeout_secs`.
fn run(
    out: &MainOutputs,
    cache: &Path,
    mode: CacheMode,
    slot: &Mutex<Option<Child>>,
    timeout_secs: u64,
    on_line: &mut dyn FnMut(&CompileLine),
) -> Result<(JobStatus, Vec<CompileLine>), String> {
    let mut child = command(out, cache, mode)?
        .spawn()
        .map_err(|e| format!("bundled tectonic spawn failed: {}", e))?;
    let (Some(stdout), Some(stderr)) = (child.stdout.take(), child.stderr.take()) else {
        let _ = child.kill();
        let _ = child.wait();
        return Err(String::from(
            "bundled tectonic spawn failed: sidecar pipes unavailable",
        ));
    };
    slot.lock().unwrap().replace(child);

    let (tx, rx) = std::sync::mpsc::channel::<CompileLine>();
    let pump = |pipe: Box<dyn std::io::Read + Send>, stream: CompileStream| {
        let tx = tx.clone();
        std::thread::spawn(move || {
            use std::io::{BufRead, BufReader};
            for text in BufReader::new(pipe).lines().map_while(Result::ok) {
                let signal = ms::line_signal(&text);
                if tx
                    .send(CompileLine {
                        stream,
                        text,
                        signal,
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
    };
    let readers = [
        pump(Box::new(stdout), CompileStream::Stdout),
        pump(Box::new(stderr), CompileStream::Stderr),
    ];
    drop(tx);
    let mut lines = Vec::new();
    for line in rx {
        on_line(&line);
        lines.push(line);
    }
    for r in readers {
        let _ = r.join();
    }

    let taken = slot.lock().unwrap().take();
    let status = match taken {
        None => JobStatus::Cancelled,
        Some(c) => match super::wait_for_child(c, timeout_secs) {
            JobOutcome::Cancelled => JobStatus::Cancelled,
            JobOutcome::TimedOut => JobStatus::TimedOut,
            JobOutcome::Exited(s) if s.success() => JobStatus::Success,
            JobOutcome::Exited(_) => JobStatus::Failed,
        },
    };
    Ok((status, lines))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_key_matches_the_engine_cache_layout() {
        assert_eq!(
            url_key(BUNDLE_URL),
            "https,58,,47,,47,data1b.fullyjustified.net,47,tlextras-2022.0r0.tar"
        );
    }

    #[test]
    fn cache_is_app_owned_and_keyed_by_digest() {
        let c = cache_dir();
        assert!(c.starts_with(super::super::out_base_dir()));
        assert!(c.ends_with(format!("maleficium-tectonic/{BUNDLE_DIGEST}")));
    }

    fn cache_with(digest: Option<&str>) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "maleficium-engine-{}-{}",
            std::process::id(),
            digest.unwrap_or("none").get(..8).unwrap_or("none")
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let hashes = dir.join("bundles").join("hashes");
        std::fs::create_dir_all(&hashes).unwrap();
        if let Some(d) = digest {
            std::fs::write(hashes.join(url_key(BUNDLE_URL)), format!("{d}\n")).unwrap();
        }
        dir
    }

    #[test]
    fn digest_check_reads_what_the_engine_resolved() {
        assert_eq!(check_digest(&cache_with(None)), DigestCheck::Unresolved);
        assert_eq!(
            check_digest(&cache_with(Some(BUNDLE_DIGEST))),
            DigestCheck::Pinned
        );
        let other = "0".repeat(64);
        assert_eq!(
            check_digest(&cache_with(Some(&other))),
            DigestCheck::Changed(other)
        );
    }

    #[test]
    fn command_pins_the_bundle_and_the_cache() {
        let out = super::super::main_outputs(Path::new("/home/u/paper/main.tex")).unwrap();
        let cache = Path::new("/c");
        for (mode, cached_only) in [(CacheMode::CachedOnly, true), (CacheMode::Online, false)] {
            let cmd = command(&out, cache, mode).unwrap();
            let args: Vec<String> = cmd
                .get_args()
                .map(|a| a.to_string_lossy().to_string())
                .collect();
            let b = args.iter().position(|a| a == "-b").unwrap();
            assert_eq!(args[b + 1], BUNDLE_URL);
            assert_eq!(args.iter().any(|a| a == "-C"), cached_only);
            let env: Vec<_> = cmd.get_envs().collect();
            assert!(env
                .iter()
                .any(|(k, v)| *k == "TECTONIC_CACHE_DIR" && *v == Some(cache.as_os_str())));
            assert_eq!(cmd.get_current_dir(), Some(Path::new("/home/u/paper")));
        }
    }
}
