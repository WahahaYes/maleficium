//! `maleficium-engine convert`: latexml turns the main file into one HTML5
//! document, run as a killable child with the compile's discipline.
//!
//! The child runs under a timeout, Cancel ([`cancel`]) and the app's exit
//! hook ([`shutdown`]) reach it through the registry on [`Core`], and a crash
//! or hang costs the export its article, never the editor. The engine
//! resolves every TeX file from the pinned bundle in the app-owned cache, the
//! one the compile filled. An export never fetches, so the child always runs
//! cached-only (`-C` and `MALEFICIUM_CACHED_ONLY=1`). Its stderr is noisy and
//! is only kept for the failure message; the conversion log is `--log`.
//!
//! Format dumps: the packaged engine finds `resources/dumps` in the app's
//! resource directory itself (`engine/src/session.rs`, `resource_dir`;
//! verified on Linux, where it is `<exe dir>/../lib/Maleficium/`). A debug
//! build has no resource directory (its engine is the copy beside
//! `target/debug/` or the one in `src-tauri/binaries/`), so it passes the dev
//! tree's `src-tauri/resources/dumps` as `--dumps`. A release build never does.

use crate::compile::JobStatus;
use crate::Core;

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// How long one conversion may run before it is killed.
pub const CONVERT_TIMEOUT_SECS: u64 = 300;

/// What one conversion produced.
#[derive(Debug, Clone)]
pub struct Conversion {
    /// The HTML document, or why there is none.
    pub html: Result<String, String>,
    /// The `Error:` and `Fatal:` lines of the conversion log, in order.
    pub errors: Vec<String>,
}

/// Turns a main file into HTML. The product path is [`Engine`]; a test
/// installs its own with [`Core::set_converter`].
pub trait Converter: Send + Sync {
    /// Convert `main` (a canonical `.tex` path), writing anything the
    /// converter needs under `work`, an empty app-owned scratch directory.
    fn convert(&self, cx: &Core, main: &Path, work: &Path) -> Conversion;
}

/// The bundled `maleficium-engine`.
pub struct Engine;

/// The converter exports use: the one a test installed, else [`Engine`].
pub fn converter(cx: &Core) -> Arc<dyn Converter> {
    cx.converter().unwrap_or_else(|| Arc::new(Engine))
}

/// One running child; whoever takes it out owns killing and reaping it.
type Slot = Arc<Mutex<Option<Child>>>;

/// The running conversion children, so Cancel and exit can reach them.
#[derive(Default)]
pub(crate) struct Running {
    live: Mutex<Vec<(u64, Slot)>>,
    next: AtomicU64,
}

/// Kill every running conversion; the export it belongs to reads as
/// cancelled.
pub fn cancel(cx: &Core) -> Result<String, String> {
    let slots: Vec<_> = cx.converts().live.lock().unwrap().clone();
    let mut killed = 0;
    for (_, slot) in slots {
        if let Some(mut c) = slot.lock().unwrap().take() {
            let _ = c.kill();
            let _ = c.wait();
            killed += 1;
        }
    }
    if killed == 0 {
        Err(String::from("no conversion is running"))
    } else {
        Ok(String::from("cancelled"))
    }
}

/// The exit hook: kill and reap every running conversion.
pub fn shutdown(cx: &Core) {
    let _ = cancel(cx);
}

const OUT: &str = "article.html";
const LOG: &str = "convert.log";
const STDERR: &str = "engine-stderr.txt";

impl Converter for Engine {
    fn convert(&self, cx: &Core, main: &Path, work: &Path) -> Conversion {
        let failed = |why: String| Conversion {
            html: Err(why),
            errors: Vec::new(),
        };
        let engine = match crate::sidecar_path_for("maleficium-engine") {
            Ok(p) => p,
            Err(e) => return failed(e),
        };
        let cache = crate::engine::cache_dir();
        if let Err(e) = std::fs::create_dir_all(&cache) {
            return failed(format!("engine cache unreachable: {e}"));
        }
        let mut cmd = command(&engine, main, work, &cache, dev_dumps().as_deref());
        match std::fs::File::create(work.join(STDERR)) {
            Ok(f) => cmd.stderr(f),
            Err(e) => return failed(format!("cannot write in {}: {e}", work.display())),
        };
        let (status, spawn) = run(cx, cmd, CONVERT_TIMEOUT_SECS);
        if let Err(e) = spawn {
            return failed(format!("the converter did not start: {e}"));
        }
        finish(status, work)
    }
}

/// The engine's command line for one conversion.
fn command(engine: &Path, main: &Path, work: &Path, cache: &Path, dumps: Option<&Path>) -> Command {
    let mut cmd = crate::quiet_command(engine);
    cmd.arg("convert")
        .arg(main)
        .arg("--out")
        .arg(work.join(OUT))
        .arg("--log")
        .arg(work.join(LOG))
        .args(["-b", &crate::engine::bundle_url()])
        .arg("--cache")
        .arg(cache)
        .arg("-C");
    if let Some(d) = dumps {
        cmd.arg("--dumps").arg(d);
    }
    cmd.env("TECTONIC_CACHE_DIR", cache)
        .env("MALEFICIUM_CACHED_ONLY", "1")
        .current_dir(main.parent().unwrap_or(Path::new(".")))
        .stdin(Stdio::null())
        .stdout(Stdio::null());
    cmd
}

/// The dev tree's dumps in a debug build, where the engine has no resource
/// directory to find them in; `None` in a release build.
fn dev_dumps() -> Option<PathBuf> {
    if !cfg!(debug_assertions) {
        return None;
    }
    let tree = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    dunce::canonicalize(tree.join("resources").join("dumps")).ok()
}

/// Spawn `cmd`, park the child where [`cancel`] finds it, wait up to
/// `timeout_secs`. The status is `Cancelled` when someone took the child.
fn run(cx: &Core, mut cmd: Command, timeout_secs: u64) -> (JobStatus, Result<(), String>) {
    let child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => return (JobStatus::Failed, Err(e.to_string())),
    };
    let reg = cx.converts();
    let id = reg.next.fetch_add(1, Ordering::Relaxed);
    let slot = Arc::new(Mutex::new(Some(child)));
    reg.live.lock().unwrap().push((id, slot.clone()));
    // Poll so a cancel can take the child between checks.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(timeout_secs);
    let status = loop {
        let mut guard = slot.lock().unwrap();
        let Some(child) = guard.as_mut() else {
            break JobStatus::Cancelled;
        };
        match child.try_wait() {
            Ok(Some(s)) => {
                guard.take();
                break if s.success() {
                    JobStatus::Success
                } else {
                    JobStatus::Failed
                };
            }
            Ok(None) if std::time::Instant::now() < deadline => {}
            _ => {
                let mut c = guard.take().expect("checked above");
                let _ = c.kill();
                let _ = c.wait();
                break JobStatus::TimedOut;
            }
        }
        drop(guard);
        std::thread::sleep(std::time::Duration::from_millis(50));
    };
    reg.live.lock().unwrap().retain(|(i, _)| *i != id);
    (status, Ok(()))
}

/// Read what a finished run left in `work`.
fn finish(status: JobStatus, work: &Path) -> Conversion {
    let log = std::fs::read_to_string(work.join(LOG)).unwrap_or_default();
    let errors = log_errors(&log);
    let html = match status {
        JobStatus::Success => std::fs::read_to_string(work.join(OUT))
            .map_err(|e| format!("the converter wrote no article: {e}")),
        JobStatus::TimedOut => Err(format!(
            "the conversion took longer than {CONVERT_TIMEOUT_SECS}s and was stopped"
        )),
        JobStatus::Cancelled | JobStatus::Running => {
            Err(String::from("the conversion was cancelled"))
        }
        JobStatus::Failed => {
            let stderr = std::fs::read_to_string(work.join(STDERR)).unwrap_or_default();
            let why = stderr
                .lines()
                .rev()
                .find(|l| l.starts_with("error:"))
                .map(|l| l.trim_start_matches("error:").trim().to_string())
                .unwrap_or_else(|| String::from("the converter failed"));
            Err(why)
        }
    };
    Conversion { html, errors }
}

/// The error lines of a conversion log: latexml writes one `Error:` or
/// `Fatal:` line per problem (an undefined macro is `Error:undefined:\foo`).
pub fn log_errors(log: &str) -> Vec<String> {
    log.lines()
        .filter(|l| l.starts_with("Error:") || l.starts_with("Fatal:"))
        .map(|l| l.trim().to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_command_is_cached_only_with_its_own_log_and_the_pinned_bundle() {
        let cmd = command(
            Path::new("/e/maleficium-engine"),
            Path::new("/p/paper/main.tex"),
            Path::new("/w"),
            Path::new("/c"),
            Some(Path::new("/d")),
        );
        let args: Vec<String> = cmd
            .get_args()
            .map(|a| a.to_string_lossy().to_string())
            .collect();
        assert_eq!(args[..2], ["convert", "/p/paper/main.tex"]);
        let after = |flag: &str| args[args.iter().position(|a| a == flag).unwrap() + 1].clone();
        assert_eq!(after("--out"), "/w/article.html");
        assert_eq!(after("--log"), "/w/convert.log");
        assert_eq!(after("-b"), crate::engine::BUNDLE_URL);
        assert_eq!(after("--cache"), "/c");
        assert_eq!(after("--dumps"), "/d");
        assert!(args.iter().any(|a| a == "-C"));
        let env: Vec<_> = cmd.get_envs().collect();
        for (k, v) in [
            ("TECTONIC_CACHE_DIR", "/c"),
            ("MALEFICIUM_CACHED_ONLY", "1"),
        ] {
            assert!(env
                .iter()
                .any(|(ek, ev)| *ek == k && ev.map(|s| s.to_string_lossy() == v) == Some(true)));
        }
        assert_eq!(cmd.get_current_dir(), Some(Path::new("/p/paper")));
        let bare = command(
            Path::new("/e/x"),
            Path::new("/p/m.tex"),
            Path::new("/w"),
            Path::new("/c"),
            None,
        );
        assert!(!bare.get_args().any(|a| a == "--dumps"));
    }

    #[test]
    fn a_debug_build_gets_the_dev_dumps_wherever_its_engine_sits() {
        // The dev app's engine is the copy beside target/debug, not the one
        // in src-tauri/binaries: the dumps must not depend on where it is.
        let tree = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        if tree.join("resources").join("dumps").is_dir() {
            let d = dev_dumps().expect("a debug build passes the dev dumps");
            assert!(d.ends_with("resources/dumps") && d.is_dir());
        }
        assert_eq!(dev_dumps().is_none(), !cfg!(debug_assertions));
    }

    #[test]
    fn log_errors_keep_errors_and_fatals_only() {
        let log = "Status:conversion:2\nWarning:missing_file:x.sty\nError:undefined:\\foo The token \\foo is not defined\nInfo:x\nFatal:timeout:1 bye\n";
        assert_eq!(
            log_errors(log),
            [
                "Error:undefined:\\foo The token \\foo is not defined",
                "Fatal:timeout:1 bye"
            ]
        );
    }

    fn work(name: &str) -> PathBuf {
        let d = crate::test_scratch::dir(&format!("convert-{name}"));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[cfg(unix)]
    fn sh(script: &str) -> Command {
        let mut c = Command::new("sh");
        c.args(["-c", script]);
        c
    }

    #[cfg(unix)]
    #[test]
    fn a_hung_child_is_killed_at_the_timeout() {
        let cx = Core::default();
        let t = std::time::Instant::now();
        let (status, spawned) = run(&cx, sh("sleep 30"), 1);
        assert!(spawned.is_ok());
        assert_eq!(status, JobStatus::TimedOut);
        assert!(t.elapsed().as_secs() < 10);
        assert!(cx.converts().live.lock().unwrap().is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn cancel_reaches_a_running_conversion() {
        let cx = Core::default();
        assert!(cancel(&cx).is_err(), "nothing runs yet");
        let c2 = cx.clone();
        let h = std::thread::spawn(move || run(&c2, sh("sleep 30"), 60).0);
        let t = std::time::Instant::now();
        while cx.converts().live.lock().unwrap().is_empty() {
            assert!(t.elapsed().as_secs() < 10, "the child never registered");
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert_eq!(cancel(&cx).unwrap(), "cancelled");
        assert_eq!(h.join().unwrap(), JobStatus::Cancelled);
        assert!(t.elapsed().as_secs() < 10);
    }

    #[cfg(unix)]
    #[test]
    fn a_crash_is_an_error_with_the_engines_reason_and_the_log_errors() {
        let w = work("crash");
        std::fs::write(w.join(LOG), "Status:conversion:3\nFatal:boom it broke\n").unwrap();
        std::fs::write(
            w.join(STDERR),
            "noise\nerror: conversion failed (Status:conversion:3)\n",
        )
        .unwrap();
        let cx = Core::default();
        let (status, _) = run(&cx, sh("exit 1"), 10);
        let c = finish(status, &w);
        assert_eq!(
            c.html.unwrap_err(),
            "conversion failed (Status:conversion:3)"
        );
        assert_eq!(c.errors, ["Fatal:boom it broke"]);
        let (status, _) = run(&cx, sh("kill -SEGV $$"), 10);
        assert!(
            finish(status, &w).html.is_err(),
            "a signal is a failure, not a hang"
        );
    }

    #[test]
    fn a_success_reads_the_article_and_a_missing_one_is_an_error() {
        let w = work("ok");
        std::fs::write(w.join(OUT), "<!doctype html><p>x</p>").unwrap();
        std::fs::write(w.join(LOG), "Status:conversion:1\nWarning:a\n").unwrap();
        let c = finish(JobStatus::Success, &w);
        assert_eq!(c.html.unwrap(), "<!doctype html><p>x</p>");
        assert!(c.errors.is_empty());
        std::fs::remove_file(w.join(OUT)).unwrap();
        assert!(finish(JobStatus::Success, &w).html.is_err());
        assert!(finish(JobStatus::TimedOut, &w)
            .html
            .unwrap_err()
            .contains("300s"));
        assert!(finish(JobStatus::Cancelled, &w)
            .html
            .unwrap_err()
            .contains("cancelled"));
    }
}
