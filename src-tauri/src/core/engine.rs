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

/// Whether the engine may fetch: `CachedOnly` passes `-C`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheMode {
    CachedOnly,
    Online,
}

/// The engine's command line for one main file.
fn command(out: &MainOutputs, cache: &Path, mode: CacheMode) -> Result<Command, String> {
    let mut cmd = Command::new(super::sidecar_path_for("maleficium-tectonic")?);
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

/// How a compile ended: the last engine run's status, every run's lines,
/// and the dependency it lacked. `missing` is `bundle-changed` beside an
/// otherwise clean result when the pin resolved to another digest.
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

/// One engine run inside the flow: its mode, each line as it arrives,
/// and how it ended.
type RunOne<'a> = dyn for<'e> FnMut(
        CacheMode,
        &'e mut dyn FnMut(&CompileLine),
    ) -> Result<(JobStatus, Vec<CompileLine>), String>
    + 'a;

/// Compile trying the network whenever the cache cannot answer: with
/// nothing resolved, compile online at once; otherwise compile from the
/// cache alone and fetch what it lacks with one online rerun. A file the
/// bundle does not carry stops without fetching. An online attempt the
/// engine blames on the network reads as offline. `networked` compiles
/// online first, then proves the result from the cache alone.
pub fn compile(
    out: &MainOutputs,
    slot: &Mutex<Option<Child>>,
    timeout_secs: u64,
    networked: bool,
    on_line: &mut dyn FnMut(&CompileLine),
) -> Result<Compiled, String> {
    let cache = cache_dir();
    std::fs::create_dir_all(&cache).map_err(|e| format!("engine cache unreachable: {}", e))?;
    std::fs::create_dir_all(&out.outdir).map_err(|e| format!("outdir unreachable: {}", e))?;
    compile_with(&cache, networked, on_line, &mut |mode, emit| {
        run(out, &cache, mode, slot, timeout_secs, &mut *emit)
    })
}

/// A status line carrying the online attempt: the status bar shows it
/// while the engine is tried, and the event log keeps it.
fn attempt_line(text: String) -> CompileLine {
    CompileLine {
        signal: Some(LineSignal::Phase {
            phase: CompilePhase::Connect,
            detail: None,
        }),
        ..status_line(text)
    }
}

/// The flow over an injected engine run: the product path passes the
/// spawn, tests script it. Every run's lines join the report; only the
/// last run decides the outcome.
fn compile_with(
    cache: &Path,
    networked: bool,
    on_line: &mut dyn FnMut(&CompileLine),
    run_one: &mut RunOne<'_>,
) -> Result<Compiled, String> {
    let mut mode = CacheMode::CachedOnly;
    let mut verify = networked;
    let mut every_line: Vec<CompileLine> = Vec::new();
    if verify {
        on_line(&status_line(String::from(
            "making available offline: fetching everything this document needs",
        )));
        mode = CacheMode::Online;
    } else if check_digest(cache) == DigestCheck::Unresolved {
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
        let automatic = !verify && mode == CacheMode::Online;
        let (status, lines) = run_one(mode, on_line)?;
        every_line.extend(lines.iter().cloned());
        let in_bundle = |f: &str| bundle_files(cache).is_none_or(|names| names.contains(f));
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
        if mode == CacheMode::CachedOnly && refetch {
            let what = missing
                .as_ref()
                .and_then(|m| m.file.clone())
                .unwrap_or_else(|| String::from("TeX support files"));
            on_line(&attempt_line(format!("trying online: fetching {what}")));
            mode = CacheMode::Online;
            continue;
        }
        let unreachable = missing.as_ref().is_some_and(|m| {
            matches!(
                m.reason,
                MissingReason::BundleUnreachable | MissingReason::FetchFailed
            )
        });
        if automatic && unreachable {
            on_line(&status_line(String::from(
                "offline: the online attempt failed",
            )));
            missing = missing.map(ms::offline_reading);
        }
        if missing.is_none() {
            if let DigestCheck::Changed(d) = check_digest(cache) {
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
            lines: every_line,
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

    fn err_line(text: &str) -> CompileLine {
        CompileLine {
            stream: CompileStream::Stderr,
            text: text.to_string(),
            signal: None,
        }
    }

    fn cached_only_miss(file: &str) -> Vec<CompileLine> {
        let texts = [
            "note: \"version 2\" Tectonic command-line interface activated".to_string(),
            "note: using only cached resource files".to_string(),
            "note: Running TeX ...".to_string(),
            format!("error: main.tex:3: ! LaTeX Error: File `{file}' not found."),
        ];
        texts.iter().map(|s| err_line(s)).collect()
    }

    /// A route without transit: the machine dials, nothing answers. The
    /// engine retries, then reports the fetch it could not make.
    fn no_transit_attempt(file: &str) -> Vec<CompileLine> {
        let texts = [
            "note: Running TeX ...".to_string(),
            format!("warning: failure fetching \"{file}\" from network (1/3)"),
            "caused by: error sending request for url (https://data1b.fullyjustified.net/tlextras-2022.0r0.tar)".to_string(),
            "caused by: dns error: failed to lookup address information".to_string(),
            format!("warning: failure fetching \"{file}\" from network (2/3)"),
            format!("warning: open of input {file} failed"),
            format!(
                "caused by: failed to download \"{file}\"; please check your network connection."
            ),
            format!("error: main.tex:3: ! LaTeX Error: File `{file}' not found."),
        ];
        texts.iter().map(|s| err_line(s)).collect()
    }

    fn unreachable_bundle() -> Vec<CompileLine> {
        [
            "note: \"version 2\" Tectonic command-line interface activated",
            "error: this bundle isn't cached, and we couldn't get it from the internet. Error: error sending request for url (https://data1b.fullyjustified.net/tlextras-2022.0r0.tar.index.gz)",
        ]
        .into_iter()
        .map(err_line)
        .collect()
    }

    /// A scripted engine: each run streams its lines, then ends as told.
    fn scripted(
        runs: Vec<(JobStatus, Vec<CompileLine>)>,
        seen: &Mutex<Vec<CacheMode>>,
    ) -> Box<RunOne<'_>> {
        let mut runs = runs;
        runs.reverse();
        Box::new(move |mode, emit| {
            seen.lock().unwrap().push(mode);
            let (status, lines) = runs.pop().expect("more runs than scripted");
            for l in &lines {
                emit(l);
            }
            Ok((status, lines))
        })
    }

    fn flow(
        cache: &Path,
        networked: bool,
        runs: Vec<(JobStatus, Vec<CompileLine>)>,
    ) -> (Vec<CacheMode>, Vec<CompileLine>, Compiled) {
        let seen = Mutex::new(Vec::new());
        let mut emitted = Vec::new();
        let c = compile_with(
            cache,
            networked,
            &mut |l| emitted.push(l.clone()),
            &mut scripted(runs, &seen),
        )
        .unwrap();
        let modes = seen.lock().unwrap().clone();
        (modes, emitted, c)
    }

    fn scratch_cache(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("maleficium-attempt-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn resolved_cache(name: &str) -> PathBuf {
        let dir = scratch_cache(name);
        let hashes = dir.join("bundles").join("hashes");
        std::fs::create_dir_all(&hashes).unwrap();
        std::fs::write(
            hashes.join(url_key(BUNDLE_URL)),
            format!("{BUNDLE_DIGEST}\n"),
        )
        .unwrap();
        dir
    }

    #[test]
    fn a_cached_only_miss_is_tried_online_then_read_as_not_cached() {
        let cache = resolved_cache("refetch");
        let (modes, emitted, c) = flow(
            &cache,
            false,
            vec![
                (JobStatus::Failed, cached_only_miss("booktabs.sty")),
                (JobStatus::Failed, no_transit_attempt("booktabs.sty")),
            ],
        );
        assert_eq!(modes, vec![CacheMode::CachedOnly, CacheMode::Online]);
        let texts: Vec<&str> = emitted.iter().map(|l| l.text.as_str()).collect();
        let attempt = texts
            .iter()
            .position(|t| t.starts_with("trying online:"))
            .expect("an attempt state before the fallback");
        let fallback = texts
            .iter()
            .position(|t| t.starts_with("offline:"))
            .expect("a fallback state after the attempt");
        assert!(attempt < fallback, "{texts:?}");
        assert!(matches!(
            emitted[attempt].signal,
            Some(LineSignal::Phase {
                phase: CompilePhase::Connect,
                detail: None
            })
        ));
        assert_eq!(c.status, JobStatus::Failed);
        assert_eq!(
            c.missing,
            Some(MissingDependency {
                file: Some("booktabs.sty".into()),
                reason: MissingReason::NotCached,
            })
        );
        assert!(!c.cached_only);
    }

    #[test]
    fn an_empty_cache_is_tried_online_then_read_as_cache_empty() {
        let cache = scratch_cache("empty");
        assert_eq!(check_digest(&cache), DigestCheck::Unresolved);
        let (modes, emitted, c) = flow(
            &cache,
            false,
            vec![(JobStatus::Failed, unreachable_bundle())],
        );
        assert_eq!(modes, vec![CacheMode::Online]);
        let texts: Vec<&str> = emitted.iter().map(|l| l.text.as_str()).collect();
        assert!(
            texts.iter().any(|t| t.starts_with("first compile:")),
            "{texts:?}"
        );
        assert!(texts.iter().any(|t| t.starts_with("offline:")), "{texts:?}");
        assert_eq!(c.status, JobStatus::Failed);
        assert_eq!(
            c.missing,
            Some(MissingDependency {
                file: None,
                reason: MissingReason::CacheEmpty,
            })
        );
    }

    #[test]
    fn a_recovered_fetch_is_a_success() {
        let cache = resolved_cache("recover");
        let (modes, _, c) = flow(
            &cache,
            false,
            vec![
                (JobStatus::Failed, cached_only_miss("booktabs.sty")),
                (
                    JobStatus::Success,
                    vec![err_line("note: downloading booktabs.sty")],
                ),
            ],
        );
        assert_eq!(modes, vec![CacheMode::CachedOnly, CacheMode::Online]);
        assert_eq!(c.status, JobStatus::Success);
        assert_eq!(c.missing, None);
        assert!(!c.cached_only);
    }

    #[test]
    fn an_explicit_networked_run_keeps_the_engines_verdict() {
        let cache = scratch_cache("explicit");
        let (modes, emitted, c) = flow(
            &cache,
            true,
            vec![(JobStatus::Failed, unreachable_bundle())],
        );
        assert_eq!(modes, vec![CacheMode::Online]);
        assert!(
            !emitted.iter().any(|l| l.text.starts_with("offline:")),
            "no fallback: the run was asked to be online"
        );
        assert_eq!(
            c.missing,
            Some(MissingDependency {
                file: None,
                reason: MissingReason::BundleUnreachable,
            })
        );
    }
}
