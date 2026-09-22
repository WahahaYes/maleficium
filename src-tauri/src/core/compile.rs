//! Compile orchestration against explicit session roots: sidecar
//! resolution, outdir sharding, hang guard, job table.

use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::{Mutex, OnceLock};

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
}

struct LiveJob {
    child: Option<Child>,
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

/// Spawn a compile job: resolves paths, spawns the sidecar, pumps output on
/// a worker thread. Returns the job id immediately; poll for the record.
pub fn run(root_id: &str, rel: &str, timeout_secs: u64) -> Result<String, String> {
    let abs = super::fs::resolve_in(root_id, rel)?;
    if !abs.is_file() {
        return Err(format!("not a file: {}", rel));
    }
    let super::MainOutputs {
        dir,
        main_file,
        outdir,
        pdf_name,
    } = super::main_outputs(&abs)?;
    std::fs::create_dir_all(&outdir).map_err(|e| format!("outdir unreachable: {}", e))?;
    let outdir_str = outdir.to_string_lossy().to_string();
    let bin = super::sidecar_path_for("tectonic")
        .ok_or_else(|| String::from("bundled tectonic sidecar missing (src-tauri/binaries/)"))?;

    let mut child = Command::new(&bin)
        .args([
            "-X",
            "compile",
            &main_file,
            "--outdir",
            &outdir_str,
            "--synctex",
        ])
        .current_dir(&dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("bundled tectonic spawn failed: {}", e))?;

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let (stdout, stderr) = match (stdout, stderr) {
        (Some(o), Some(e)) => (o, e),
        _ => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(String::from("sidecar pipes unavailable"));
        }
    };

    let id = next_id();
    let (tx, rx) = std::sync::mpsc::channel();
    jobs().lock().unwrap().insert(
        id.clone(),
        LiveJob {
            child: Some(child),
            lines: Vec::new(),
            done_tx: Some(tx),
            done_rx: Some(rx),
        },
    );

    let job_id = id.clone();
    let log_file = super::log_file(&outdir, &main_file);
    std::thread::spawn(move || {
        let reader = BufReader::new(stdout);
        let pump_lines: Vec<String> = reader.lines().map_while(Result::ok).collect();
        let err_reader = BufReader::new(stderr);
        let mut collected: Vec<String> = Vec::new();
        for line in err_reader.lines().map_while(Result::ok) {
            collected.push(line);
        }
        let mut all = pump_lines;
        all.extend(collected.iter().cloned());

        let outcome = {
            let mut guard = jobs().lock().unwrap();
            match guard.get_mut(&job_id).and_then(|j| j.child.take()) {
                None => JobStatus::Cancelled,
                Some(c) => {
                    drop(guard);
                    match super::wait_for_child(c, timeout_secs) {
                        super::JobOutcome::Cancelled => JobStatus::Cancelled,
                        super::JobOutcome::TimedOut => JobStatus::TimedOut,
                        super::JobOutcome::Exited(s) if s.success() => JobStatus::Success,
                        super::JobOutcome::Exited(_) => JobStatus::Failed,
                    }
                }
            }
        };

        let (pdf_url, log) = match outcome {
            JobStatus::Success => {
                let pdf = outdir.join(&pdf_name);
                (Some(pdf.to_string_lossy().to_string()), String::new())
            }
            JobStatus::Failed => {
                let tail = collected.join("\n");
                let t = &tail[..500.min(tail.len())];
                (None, format!("bundled tectonic failed: {}", t))
            }
            JobStatus::TimedOut => (
                None,
                format!(
                    "compile timed out after {}s (engine produced no exit — killed)",
                    timeout_secs
                ),
            ),
            JobStatus::Cancelled => (None, String::from("compile cancelled")),
            JobStatus::Running => (None, String::new()),
        };
        super::write_engine_log(&log_file, &all);
        let record = JobRecord {
            status: outcome,
            pdf_url,
            log,
            lines: all,
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
    })
}

/// Cancel a running job: takes the child, kills + reaps it.
pub fn cancel(job_id: &str) -> Result<String, String> {
    let mut guard = jobs().lock().unwrap();
    let job = guard
        .get_mut(job_id)
        .ok_or_else(|| format!("unknown job: {}", job_id))?;
    match job.child.take() {
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
        let dir = std::env::temp_dir().join(format!("maleficium-job-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let canon = dir.canonicalize().unwrap();
        super::super::fs::grant_root("job-escape", &canon.to_string_lossy()).unwrap();
        assert!(run("job-escape", "../outside.tex", 5).is_err());
        assert!(run("job-escape", "/etc/hostname", 5).is_err());
    }
}
