//! Compile jobs against explicit session roots: each job runs the engine on
//! a worker thread and is polled or cancelled by id.

use std::collections::HashMap;
use std::process::Child;
use std::sync::{Arc, Mutex, OnceLock};

use maleficium_structure::MissingDependency;

use maleficium_events::OfflineReadiness;

use super::{engine, readiness};

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
}

struct LiveJob {
    /// The running engine; a cancel takes it from here.
    child: Arc<Mutex<Option<Child>>>,
    lines: Vec<String>,
    done_tx: Option<std::sync::mpsc::Sender<JobRecord>>,
    done_rx: Option<std::sync::mpsc::Receiver<JobRecord>>,
}

static JOBS: OnceLock<Mutex<HashMap<String, LiveJob>>> = OnceLock::new();
static NEXT_ID: OnceLock<Mutex<u64>> = OnceLock::new();

fn jobs() -> &'static Mutex<HashMap<String, LiveJob>> {
    JOBS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn next_id() -> String {
    let lock = NEXT_ID.get_or_init(|| Mutex::new(0));
    let mut n = lock.lock().unwrap();
    *n += 1;
    format!("job-{}", *n)
}

/// The failure message: the first 500 bytes of the last run's stderr, or
/// the flow's own account when it stopped before spawning.
pub fn failure_text(c: &engine::Compiled) -> String {
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

/// Keep what one compile showed: its engine log, and what it says about
/// the project's offline readiness. Best effort: a failed write only costs
/// the record, never the compile result.
pub fn settle(root_id: &str, main_rel: &str, out: &super::MainOutputs, c: &engine::Compiled) {
    let lines = c.texts();
    super::write_engine_log(&super::log_file(&out.outdir, &out.main_file), &lines);
    let Ok(root) = super::fs::session_root(root_id) else {
        return;
    };
    let revision = super::structure::file_graph(root_id, main_rel)
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

/// A project's offline readiness from its record, the engine cache, and
/// this machine's tools and fonts.
pub fn offline_readiness(root_id: &str) -> Result<OfflineReadiness, String> {
    let root = super::fs::session_root(root_id)?;
    Ok(readiness::assess(
        readiness::load(&root).as_ref(),
        &engine::check_digest(&engine::cache_dir()),
        &readiness::on_path,
        &|f| std::path::Path::new(f).exists(),
    ))
}

/// Start a compile job: resolves paths, then runs the engine on a worker
/// thread. Returns the job id immediately; poll for the record.
pub fn run(root_id: &str, rel: &str, networked: bool, timeout_secs: u64) -> Result<String, String> {
    let abs = super::fs::resolve_in(root_id, rel)?;
    if !abs.is_file() {
        return Err(format!("not a file: {}", rel));
    }
    let out = super::main_outputs(&abs)?;

    let id = next_id();
    let child = Arc::new(Mutex::new(None));
    let (tx, rx) = std::sync::mpsc::channel();
    jobs().lock().unwrap().insert(
        id.clone(),
        LiveJob {
            child: child.clone(),
            lines: Vec::new(),
            done_tx: Some(tx),
            done_rx: Some(rx),
        },
    );

    let job_id = id.clone();
    let (root_id, rel) = (root_id.to_string(), rel.to_string());
    std::thread::spawn(move || {
        let mut on_line = |l: &maleficium_events::CompileLine| {
            if let Some(job) = jobs().lock().unwrap().get_mut(&job_id) {
                job.lines.push(l.text.clone());
            }
        };
        let record = match engine::compile(&out, &child, timeout_secs, networked, &mut on_line) {
            Err(e) => JobRecord {
                status: JobStatus::Failed,
                pdf_url: None,
                log: e,
                lines: Vec::new(),
                missing: None,
            },
            Ok(c) => {
                settle(&root_id, &rel, &out, &c);
                // The record keeps the whole stream: every run and status line.
                let lines = jobs()
                    .lock()
                    .unwrap()
                    .get(&job_id)
                    .map(|j| j.lines.clone())
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
                }
            }
        };
        if let Some(job) = jobs().lock().unwrap().get_mut(&job_id) {
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
pub fn poll(job_id: &str, tail_lines: usize) -> Result<JobRecord, String> {
    let mut guard = jobs().lock().unwrap();
    let job = guard
        .get_mut(job_id)
        .ok_or_else(|| format!("unknown job: {}", job_id))?;
    if let Some(rx) = job.done_rx.take() {
        match rx.try_recv() {
            Ok(record) => {
                job.lines = record.lines.clone();
                let _ = job.done_rx.insert(rx);
                return Ok(record);
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                let _ = job.done_rx.insert(rx);
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                return Ok(JobRecord {
                    status: JobStatus::Failed,
                    pdf_url: None,
                    log: String::from("compile worker lost"),
                    lines: job.lines.clone(),
                    missing: None,
                });
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
    })
}

/// Cancel a running job: takes the child, kills + reaps it.
pub fn cancel(job_id: &str) -> Result<String, String> {
    let mut guard = jobs().lock().unwrap();
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

/// Test-only: drop all job state between tests.
#[cfg(test)]
pub fn clear_jobs() {
    jobs().lock().unwrap().clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn poll_unknown_job_fails() {
        clear_jobs();
        assert!(poll("job-404", 10).is_err());
    }

    #[test]
    fn cancel_unknown_job_fails() {
        clear_jobs();
        assert!(cancel("job-404").is_err());
    }

    #[test]
    fn run_rejects_outside_root() {
        clear_jobs();
        let dir = crate::test_scratch::dir("job");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let canon = dunce::canonicalize(&dir).unwrap();
        super::super::fs::grant_root("job-escape", &canon.to_string_lossy()).unwrap();
        for bad in crate::test_scratch::escapes() {
            assert!(run("job-escape", bad, false, 5).is_err(), "{bad}");
        }
    }
}
