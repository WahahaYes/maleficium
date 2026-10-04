//! Compile jobs against explicit session roots: each job runs the engine on
//! a worker thread and is polled or cancelled by id.

use crate::Core;

use std::collections::HashMap;
use std::process::Child;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use maleficium_structure::{
    CompilePhase, FetchOutcome, LineSignal, MissingDependency, MissingReason,
};

use maleficium_events::{
    Actor, BusEvent, CompileFailure, CompileLine, CompileReport, OfflineReadiness,
};

use super::{engine, readiness};
use crate::widgets::poster::cache as posters;

/// One compile timeout for every adapter: the desktop streaming run and the
/// MCP job service both give the engine this long before killing it.
pub const COMPILE_TIMEOUT_SECS: u64 = 120;

/// Where a compile's lines go while it runs. The desktop adapter forwards
/// them to the window's `compile-line` event, the MCP job service buffers
/// them for `compile_poll`, the web adapter will push them over WebSocket.
/// Closures qualify through the blanket impl below.
pub trait EventSink {
    fn push(&mut self, line: &CompileLine);
}

impl<F: FnMut(&CompileLine)> EventSink for F {
    fn push(&mut self, line: &CompileLine) {
        self(line);
    }
}

/// How a compile job resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JobStatus {
    Running,
    Success,
    Failed,
    TimedOut,
    Cancelled,
}

impl JobStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            JobStatus::Running => "running",
            JobStatus::Success => "success",
            JobStatus::Failed => "failed",
            JobStatus::TimedOut => "timed-out",
            JobStatus::Cancelled => "cancelled",
        }
    }
}

/// Finished-job record: what the poll tool returns.
#[derive(Debug, Clone)]
pub struct JobRecord {
    pub status: JobStatus,
    /// Locates the pdf: desktop, an absolute path; hosted, an opaque URL.
    pub pdf_url: Option<String>,
    pub log: String,
    pub lines: Vec<String>,
    /// The dependency the run lacked, when that is why it failed (or the
    /// bundle changed under a clean run).
    pub missing: Option<MissingDependency>,
    pub progress: Progress,
}

/// How far a job got: the latest phase its lines signalled, and the TeX
/// support files it downloaded on the way.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, schemars::JsonSchema)]
pub struct Progress {
    /// `None` until the engine reports a phase.
    pub phase: Option<CompilePhase>,
    /// What the phase is about (a rerun's reason, a tool or file name).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// Files downloaded so far.
    pub fetched: u32,
    /// Downloads that failed so far.
    pub fetch_failed: u32,
}

impl Progress {
    /// Fold one line's signal in.
    pub fn note(&mut self, signal: Option<&LineSignal>) {
        match signal {
            Some(LineSignal::Phase { phase, detail }) => {
                self.phase = Some(*phase);
                self.detail = detail.clone();
            }
            Some(LineSignal::Fetch {
                outcome: FetchOutcome::Fetched,
                ..
            }) => self.fetched += 1,
            Some(LineSignal::Fetch {
                outcome: FetchOutcome::Failed,
                ..
            }) => self.fetch_failed += 1,
            None => {}
        }
    }
}

struct LiveJob {
    /// The running engine; a cancel takes it from here.
    child: Arc<Mutex<Option<Child>>>,
    lines: Vec<String>,
    progress: Progress,
    done_tx: Option<std::sync::mpsc::Sender<JobRecord>>,
    done_rx: Option<std::sync::mpsc::Receiver<JobRecord>>,
    /// The final record once a poll has taken it off the channel, so every
    /// later poll (a second viewer, a model and a View) sees the same result.
    finished: Option<JobRecord>,
}

/// Compile jobs by id, plus the foreground run an adapter streams itself.
#[derive(Default)]
pub(crate) struct Jobs {
    live: Mutex<HashMap<String, LiveJob>>,
    current: Mutex<Option<String>>,
    next: AtomicU64,
}

impl Jobs {
    fn next_id(&self) -> String {
        format!("job-{}", self.next.fetch_add(1, Ordering::Relaxed) + 1)
    }
}

/// The one sentence for a package or class the pinned bundle does not carry:
/// the project folder is the only place it can come from.
pub(crate) fn missing_package_text(file: &str) -> String {
    format!(
        "{file} is not in the TeX bundle: add it to your project folder next to your main file."
    )
}

/// The failure message: the first 500 bytes of the last run's stderr, or
/// the flow's own account when it stopped before spawning. A missing
/// external tool is named instead: the engine's own "No such file or
/// directory" reads as if the engine itself were missing.
pub fn failure_text(c: &engine::Compiled) -> String {
    if let Some(MissingDependency {
        file: Some(tool),
        reason: MissingReason::ExternalTool,
    }) = &c.missing
    {
        return if tool == "biber" {
            String::from(
                "biber is not installed: this document's biblatex uses the biber backend. \
                 Install biber, or load biblatex with \\usepackage[backend=bibtex]{biblatex}, \
                 which compiles from the bundle alone",
            )
        } else {
            format!("{tool} is not installed: the engine runs it to finish this document. Install {tool} and compile again")
        };
    }
    if let Some(MissingDependency {
        file: Some(file),
        reason: MissingReason::NotInBundle,
    }) = &c.missing
    {
        if file.ends_with(".sty") || file.ends_with(".cls") {
            return missing_package_text(file);
        }
    }
    if c.lines.is_empty() {
        return String::from("no TeX support files are cached yet and there is no network");
    }
    let tail = c
        .lines
        .iter()
        .filter(|l| l.stream == maleficium_events::CompileStream::Stderr)
        .map(|l| l.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let mut end = 500.min(tail.len());
    while !tail.is_char_boundary(end) {
        end -= 1;
    }
    format!("bundled tectonic failed: {}", &tail[..end])
}

/// A status line of the compile's own (not the engine's).
fn status(text: String) -> CompileLine {
    CompileLine {
        stream: maleficium_events::CompileStream::Status,
        text,
        signal: None,
    }
}

/// Keep what one compile showed: its engine log, and what it says about
/// the project's offline readiness. Best effort: a failed write only costs
/// the record, never the compile result.
pub fn settle(
    cx: &Core,
    root_id: &str,
    main_rel: &str,
    out: &super::MainOutputs,
    c: &engine::Compiled,
) {
    let lines = c.texts();
    super::write_engine_log(&super::log_file(&out.outdir, &out.main_file), &lines);
    let Ok(root) = super::fs::session_root(cx, root_id) else {
        return;
    };
    let revision = super::structure::file_graph(cx, root_id, main_rel)
        .ok()
        .map(|g| g.revision);
    let outcome = readiness::Outcome {
        status: &c.status,
        missing: &c.missing,
        cached_only: c.cached_only,
        needs: maleficium_structure::external_needs(&lines),
        main: main_rel,
        revision,
    };
    if let Some(r) = readiness::next(readiness::load(&root), outcome) {
        let _ = readiness::store(&root, &r);
    }
}

/// The app-facing report for a finished engine run: the mapping the Tauri
/// command layer owned before. A run cancelled (or still going when its
/// child was reaped) reads as an error, as before.
pub fn report(
    out: &super::MainOutputs,
    c: &engine::Compiled,
    timeout_secs: u64,
) -> Result<CompileReport, String> {
    let failed = |failure, message| CompileReport {
        pdf_url: None,
        failure: Some(failure),
        missing: c.missing.clone(),
        message,
        approvals: Vec::new(),
    };
    Ok(match c.status {
        JobStatus::Success => CompileReport {
            // URLs use forward slashes on every OS.
            pdf_url: Some(
                out.outdir
                    .join(&out.pdf_name)
                    .to_string_lossy()
                    .replace('\\', "/"),
            ),
            failure: None,
            missing: c.missing.clone(),
            message: String::new(),
            approvals: Vec::new(),
        },
        JobStatus::Failed if c.missing.is_some() => {
            failed(CompileFailure::MissingDependency, failure_text(c))
        }
        JobStatus::Failed => failed(CompileFailure::EngineError, failure_text(c)),
        JobStatus::TimedOut => failed(
            CompileFailure::EngineError,
            format!(
                "compile timed out after {}s (engine produced no exit — killed; retry or Cancel, then check the LogStream tail)",
                timeout_secs
            ),
        ),
        JobStatus::Cancelled | JobStatus::Running => {
            return Err(String::from("compile cancelled"))
        }
    })
}

/// One `widget.approval-required` event per html widget the finished
/// compile found waiting for the user.
fn approval_events(cx: &Core, root_id: &str, main_rel: &str, actor: Actor) -> Vec<BusEvent> {
    posters::approvals_needed(cx, root_id, main_rel)
        .iter()
        .map(|r| crate::widget_approval::approval_required_event(root_id, r, actor))
        .collect()
}

/// A foreground compile for adapters that stream lines themselves: resolves
/// the main file, registers the run in the job registry (so
/// `cancel_current` and `shutdown` reach its child), forwards every line to
/// `sink`, settles the engine log and readiness, and reports. Callers await
/// this off the UI thread; a compile can take minutes on a cold cache.
pub fn run_blocking(
    cx: &Core,
    root_id: &str,
    main_rel: &str,
    networked: bool,
    sink: &mut dyn EventSink,
) -> Result<CompileReport, String> {
    // The main file resolves inside the session root; the engine runs in its
    // directory and writes to the app-cache outdir derived from it.
    let out = super::outputs_of(cx, root_id, main_rel)?;
    sink.push(&CompileLine {
        stream: maleficium_events::CompileStream::Status,
        text: format!(
            "sidecar compile {} in {}",
            out.main_file,
            out.dir.to_string_lossy()
        ),
        signal: None,
    });

    posters::before_compile(cx, root_id, main_rel, &mut |text| sink.push(&status(text)));

    let id = cx.jobs().next_id();
    let child = Arc::new(Mutex::new(None));
    cx.jobs().live.lock().unwrap().insert(
        id.clone(),
        LiveJob {
            child: child.clone(),
            lines: Vec::new(),
            progress: Progress::default(),
            done_tx: None,
            done_rx: None,
            finished: None,
        },
    );
    *cx.jobs().current.lock().unwrap() = Some(id.clone());
    let mut c = engine::compile(&out, &child, COMPILE_TIMEOUT_SECS, networked, &mut |l| {
        sink.push(l);
    });
    // A widget new in this compile has no poster yet: render what its widget
    // list now asks for and compile again, so the pdf shows the posters.
    if matches!(&c, Ok(r) if r.status == JobStatus::Success)
        && posters::before_compile(cx, root_id, main_rel, &mut |text| sink.push(&status(text))) > 0
    {
        c = engine::compile(&out, &child, COMPILE_TIMEOUT_SECS, networked, &mut |l| {
            sink.push(l);
        });
    }
    if cx.jobs().current.lock().unwrap().as_deref() == Some(id.as_str()) {
        *cx.jobs().current.lock().unwrap() = None;
    }
    cx.jobs().live.lock().unwrap().remove(&id);
    let c = match c {
        Ok(c) => c,
        Err(message) => {
            return Ok(CompileReport {
                pdf_url: None,
                failure: Some(CompileFailure::SpawnFailed),
                missing: None,
                message,
                approvals: Vec::new(),
            })
        }
    };
    settle(cx, root_id, main_rel, &out, &c);
    let mut approvals = Vec::new();
    if c.status == JobStatus::Success {
        posters::after_compile(cx, root_id, main_rel, &mut |text| sink.push(&status(text)));
        approvals = approval_events(cx, root_id, main_rel, Actor::System);
    }
    let mut rep = report(&out, &c, COMPILE_TIMEOUT_SECS)?;
    rep.approvals = approvals;
    Ok(rep)
}

/// Cancel the foreground compile, if one is running: the adapter's Cancel
/// button names no job id.
pub fn cancel_current(cx: &Core) -> Result<String, String> {
    let id = cx.jobs().current.lock().unwrap().take();
    match id {
        Some(job_id) => cancel(cx, &job_id),
        None => Err(String::from("nothing to cancel")),
    }
}

/// Kill every live compile child, reaping each: the process exit hook calls
/// this with the registry instead of holding its own child slot.
pub fn shutdown(cx: &Core) {
    let ids: Vec<String> = cx.jobs().live.lock().unwrap().keys().cloned().collect();
    for id in ids {
        let _ = cancel(cx, &id);
    }
    *cx.jobs().current.lock().unwrap() = None;
}

/// A project's offline readiness from its record, the engine cache, and
/// this machine's tools and fonts.
pub fn offline_readiness(cx: &Core, root_id: &str) -> Result<OfflineReadiness, String> {
    let root = super::fs::session_root(cx, root_id)?;
    Ok(readiness::assess(
        readiness::load(&root).as_ref(),
        &engine::check_digest(&engine::cache_dir()),
        &readiness::on_path,
        &|f| std::path::Path::new(f).exists(),
    ))
}

/// Start a compile job: resolves paths, then runs the engine on a worker
/// thread. Returns the job id immediately; poll for the record.
pub fn run(
    cx: &Core,
    root_id: &str,
    rel: &str,
    networked: bool,
    timeout_secs: u64,
) -> Result<String, String> {
    let abs = super::fs::resolve_in(cx, root_id, rel)?;
    if !abs.is_file() {
        return Err(format!("not a file: {}", rel));
    }
    let out = super::main_outputs(&abs)?;

    let id = cx.jobs().next_id();
    let child = Arc::new(Mutex::new(None));
    let (tx, rx) = std::sync::mpsc::channel();
    cx.jobs().live.lock().unwrap().insert(
        id.clone(),
        LiveJob {
            child: child.clone(),
            lines: Vec::new(),
            progress: Progress::default(),
            done_tx: Some(tx),
            done_rx: Some(rx),
            finished: None,
        },
    );

    let job_id = id.clone();
    let (root_id, rel) = (root_id.to_string(), rel.to_string());
    let cx = cx.clone();
    std::thread::spawn(move || {
        let cx = &cx;
        let mut on_line = |l: &maleficium_events::CompileLine| {
            if let Some(job) = cx.jobs().live.lock().unwrap().get_mut(&job_id) {
                job.lines.push(l.text.clone());
                job.progress.note(l.signal.as_ref());
            }
        };
        posters::before_compile(cx, &root_id, &rel, &mut |text| on_line(&status(text)));
        let mut compiled = engine::compile(&out, &child, timeout_secs, networked, &mut on_line);
        // A widget new in this compile has no poster yet: render what its
        // widget list now asks for and compile again, so the pdf shows them.
        if matches!(&compiled, Ok(r) if r.status == JobStatus::Success)
            && posters::before_compile(cx, &root_id, &rel, &mut |text| on_line(&status(text))) > 0
        {
            compiled = engine::compile(&out, &child, timeout_secs, networked, &mut on_line);
        }
        let record = match compiled {
            Err(e) => JobRecord {
                status: JobStatus::Failed,
                pdf_url: None,
                log: e,
                lines: Vec::new(),
                missing: None,
                progress: Progress::default(),
            },
            Ok(c) => {
                settle(cx, &root_id, &rel, &out, &c);
                if c.status == JobStatus::Success {
                    posters::after_compile(cx, &root_id, &rel, &mut |text| on_line(&status(text)));
                    // No window hears this run: the log carries the request.
                    let _ =
                        crate::eventlog::append(&approval_events(cx, &root_id, &rel, Actor::Agent));
                }
                // The record keeps the whole stream: every run and status line.
                let (lines, progress) = cx
                    .jobs()
                    .live
                    .lock()
                    .unwrap()
                    .get(&job_id)
                    .map(|j| (j.lines.clone(), j.progress.clone()))
                    .unwrap_or_default();
                let (pdf_url, log) = match c.status {
                    JobStatus::Success => (
                        Some(out.outdir.join(&out.pdf_name).to_string_lossy().to_string()),
                        String::new(),
                    ),
                    JobStatus::Failed => (None, failure_text(&c)),
                    JobStatus::TimedOut => (
                        None,
                        format!(
                            "compile timed out after {}s (engine produced no exit — killed)",
                            timeout_secs
                        ),
                    ),
                    _ => (None, String::from("compile cancelled")),
                };
                JobRecord {
                    status: c.status,
                    pdf_url,
                    log,
                    lines,
                    missing: c.missing,
                    progress,
                }
            }
        };
        if let Some(job) = cx.jobs().live.lock().unwrap().get_mut(&job_id) {
            job.lines = record.lines.clone();
            if let Some(tx) = job.done_tx.take() {
                let _ = tx.send(record);
            }
        }
    });

    Ok(id)
}

/// Non-blocking poll: running jobs report collected lines so far; finished
/// jobs report the final record.
pub fn poll(cx: &Core, job_id: &str, tail_lines: usize) -> Result<JobRecord, String> {
    let mut guard = cx.jobs().live.lock().unwrap();
    let job = guard
        .get_mut(job_id)
        .ok_or_else(|| format!("unknown job: {}", job_id))?;
    if let Some(record) = &job.finished {
        return Ok(record.clone());
    }
    if let Some(rx) = job.done_rx.take() {
        match rx.try_recv() {
            Ok(record) => {
                job.lines = record.lines.clone();
                job.finished = Some(record.clone());
                return Ok(record);
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                let _ = job.done_rx.insert(rx);
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                let record = JobRecord {
                    status: JobStatus::Failed,
                    pdf_url: None,
                    log: String::from("compile worker lost"),
                    lines: job.lines.clone(),
                    missing: None,
                    progress: job.progress.clone(),
                };
                job.finished = Some(record.clone());
                return Ok(record);
            }
        }
    }
    let n = job.lines.len();
    let start = n.saturating_sub(tail_lines.max(1));
    Ok(JobRecord {
        status: JobStatus::Running,
        pdf_url: None,
        log: String::new(),
        lines: job.lines[start..].to_vec(),
        missing: None,
        progress: job.progress.clone(),
    })
}

/// Cancel a running job: takes the child, kills + reaps it.
pub fn cancel(cx: &Core, job_id: &str) -> Result<String, String> {
    let mut guard = cx.jobs().live.lock().unwrap();
    let job = guard
        .get_mut(job_id)
        .ok_or_else(|| format!("unknown job: {}", job_id))?;
    let taken = job.child.lock().unwrap().take();
    match taken {
        Some(mut c) => {
            let _ = c.kill();
            let _ = c.wait();
            Ok(String::from("cancelled"))
        }
        None => Err(String::from("nothing to cancel")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn failed(missing: Option<MissingDependency>) -> engine::Compiled {
        engine::Compiled {
            status: JobStatus::Failed,
            lines: vec![maleficium_events::CompileLine {
                stream: maleficium_events::CompileStream::Stderr,
                text: String::from("error: No such file or directory (os error 2)"),
                signal: None,
            }],
            missing,
            cached_only: false,
        }
    }

    fn tool(name: &str) -> Option<MissingDependency> {
        Some(MissingDependency {
            file: Some(name.into()),
            reason: MissingReason::ExternalTool,
        })
    }

    #[test]
    fn a_missing_biber_is_named_with_the_bibtex_workaround() {
        let text = failure_text(&failed(tool("biber")));
        assert!(text.starts_with("biber is not installed"), "{text}");
        assert!(
            text.contains("\\usepackage[backend=bibtex]{biblatex}"),
            "{text}"
        );
        assert!(!text.contains("bundled tectonic failed"), "{text}");
    }

    fn not_in_bundle(file: &str) -> Option<MissingDependency> {
        Some(MissingDependency {
            file: Some(file.into()),
            reason: MissingReason::NotInBundle,
        })
    }

    #[test]
    fn a_package_the_bundle_lacks_names_the_file_and_the_project_folder() {
        for file in ["maleficium-interactive.sty", "myclass.cls"] {
            assert_eq!(
                failure_text(&failed(not_in_bundle(file))),
                format!(
                    "{file} is not in the TeX bundle: add it to your project folder next to your main file."
                )
            );
        }
        // Other support files the bundle lacks keep the engine's own text.
        let text = failure_text(&failed(not_in_bundle("nofont.tfm")));
        assert!(text.starts_with("bundled tectonic failed"), "{text}");
    }

    #[test]
    fn another_missing_tool_is_named() {
        let text = failure_text(&failed(tool("xindy")));
        assert!(text.starts_with("xindy is not installed"), "{text}");
    }

    #[test]
    fn other_failures_keep_the_engine_stderr() {
        let text = failure_text(&failed(None));
        assert_eq!(
            text,
            "bundled tectonic failed: error: No such file or directory (os error 2)"
        );
    }

    #[test]
    fn poll_unknown_job_fails() {
        let cx = &Core::default();
        assert!(poll(cx, "job-404", 10).is_err());
    }

    fn record(status: JobStatus) -> JobRecord {
        JobRecord {
            status,
            pdf_url: None,
            log: String::new(),
            lines: vec![String::from("done")],
            missing: None,
            progress: Progress::default(),
        }
    }

    fn live_with_channel(cx: &Core, id: &str) -> std::sync::mpsc::Sender<JobRecord> {
        let (tx, rx) = std::sync::mpsc::channel();
        cx.jobs().live.lock().unwrap().insert(
            id.to_string(),
            LiveJob {
                child: Arc::new(Mutex::new(None)),
                lines: Vec::new(),
                progress: Progress::default(),
                done_tx: None,
                done_rx: Some(rx),
                finished: None,
            },
        );
        tx
    }

    /// A model and a View both poll one job: the finished record is the
    /// answer every time, not "worker lost" after the first reader.
    #[test]
    fn a_finished_job_answers_every_poll_the_same() {
        let cx = &Core::default();
        let tx = live_with_channel(cx, "job-1");
        assert_eq!(poll(cx, "job-1", 5).unwrap().status, JobStatus::Running);
        tx.send(record(JobStatus::Success)).unwrap();
        drop(tx);
        for _ in 0..3 {
            let r = poll(cx, "job-1", 5).unwrap();
            assert_eq!((r.status, r.lines.len()), (JobStatus::Success, 1));
            assert!(r.log.is_empty());
        }
    }

    #[test]
    fn a_worker_that_vanishes_fails_the_job_once_and_stays_failed() {
        let cx = &Core::default();
        drop(live_with_channel(cx, "job-2"));
        for _ in 0..2 {
            let r = poll(cx, "job-2", 5).unwrap();
            assert_eq!(r.status, JobStatus::Failed);
            assert_eq!(r.log, "compile worker lost");
        }
    }

    #[test]
    fn cancel_unknown_job_fails() {
        let cx = &Core::default();
        assert!(cancel(cx, "job-404").is_err());
    }

    #[test]
    fn run_rejects_outside_root() {
        let cx = &Core::default();
        let dir = crate::test_scratch::dir("job");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let canon = dunce::canonicalize(&dir).unwrap();
        super::super::fs::grant_root(cx, "job-escape", &canon.to_string_lossy()).unwrap();
        for bad in crate::test_scratch::escapes() {
            assert!(run(cx, "job-escape", bad, false, 5).is_err(), "{bad}");
        }
    }

    fn outputs_at(dir: &std::path::Path) -> super::super::MainOutputs {
        super::super::MainOutputs {
            dir: dir.to_path_buf(),
            main_file: String::from("main.tex"),
            outdir: dir.join("out"),
            pdf_name: String::from("main.pdf"),
        }
    }

    fn compiled(status: JobStatus) -> engine::Compiled {
        engine::Compiled {
            status,
            lines: Vec::new(),
            missing: None,
            cached_only: false,
        }
    }

    #[test]
    fn report_success_carries_the_pdf_path() {
        let dir = std::path::Path::new("/tmp/paper");
        let r = report(&outputs_at(dir), &compiled(JobStatus::Success), 120).unwrap();
        assert_eq!(r.pdf_url, Some(String::from("/tmp/paper/out/main.pdf")));
        assert!(r.failure.is_none());
    }

    #[test]
    fn report_timeout_names_the_wait() {
        let dir = std::path::Path::new("/tmp/paper");
        let r = report(&outputs_at(dir), &compiled(JobStatus::TimedOut), 120).unwrap();
        assert!(r.pdf_url.is_none());
        assert!(r.message.contains("120s"));
    }

    #[test]
    fn report_cancelled_is_an_error() {
        let dir = std::path::Path::new("/tmp/paper");
        assert!(report(&outputs_at(dir), &compiled(JobStatus::Cancelled), 120).is_err());
    }

    #[test]
    fn closures_serve_as_event_sinks() {
        let mut seen = Vec::new();
        let mut sink = |l: &maleficium_events::CompileLine| seen.push(l.text.clone());
        let sink: &mut dyn EventSink = &mut sink;
        sink.push(&maleficium_events::CompileLine {
            stream: maleficium_events::CompileStream::Status,
            text: String::from("hello"),
            signal: None,
        });
        assert_eq!(seen, ["hello"]);
    }

    #[test]
    fn blocking_run_rejects_unknown_roots_before_touching_a_sink() {
        let cx = &Core::default();
        let mut lines = 0;
        let mut sink = |_: &maleficium_events::CompileLine| lines += 1;
        assert!(run_blocking(cx, "nope", "main.tex", false, &mut sink).is_err());
        assert_eq!(lines, 0);
    }

    #[test]
    fn cancel_current_without_a_run_fails() {
        let cx = &Core::default();
        assert!(cancel_current(cx).is_err());
    }

    #[test]
    fn shutdown_without_jobs_is_a_no_op() {
        shutdown(&Core::default());
    }
}
