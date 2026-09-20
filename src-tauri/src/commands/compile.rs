use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, State};

use super::guard::require_allowed;
use crate::core;

pub struct CompileState(pub Mutex<Option<std::process::Child>>);

impl Default for CompileState {
    fn default() -> Self {
        Self(Mutex::new(None))
    }
}

fn out_pdf(outdir: &Path, main_file: &str) -> PathBuf {
    let stem = main_file.strip_suffix(".tex").unwrap_or(main_file);
    outdir.join(format!("{}.pdf", stem))
}

#[tauri::command]
pub fn compile_tex(
    app: AppHandle,
    state: State<'_, CompileState>,
    input: String,
    workdir: String,
) -> Result<String, String> {
    // Both strings arrive untrusted: each must resolve inside the live fs
    // scope before anything else. The outdir derives from the app-cache
    // dir server-side. Canonical forms drive the split (no raw-string
    // slicing), so `..` / symlink escapes fail here, not at the spawn.
    let workdir_canon = require_allowed(&app, &workdir)?;
    let input_canon: Option<PathBuf> = if Path::new(&input).is_absolute() {
        Some(require_allowed(&app, &input)?)
    } else {
        None
    };
    let (dir, main_file) = match input_canon {
        Some(canon) => {
            let d = canon
                .parent()
                .map(|d| d.to_path_buf())
                .unwrap_or_else(|| workdir_canon.clone());
            let f = canon
                .file_name()
                .map(|f| f.to_string_lossy().to_string())
                .unwrap_or_else(|| input.clone());
            (d, f)
        }
        None => (workdir_canon.clone(), input.clone()),
    };
    let outdir = core::out_dir_for(&core::out_base_dir(), &dir.to_string_lossy());
    let _ = std::fs::create_dir_all(&outdir);
    let outdir_str = outdir.to_string_lossy().to_string();
    let _ = app.emit(
        "compile-line",
        format!("sidecar compile {} in {}", main_file, dir.to_string_lossy()),
    );

    const COMPILE_TIMEOUT_SECS: u64 = 120;
    let bin = core::sidecar_path_for("tectonic")
        .ok_or_else(|| String::from("bundled tectonic sidecar missing (src-tauri/binaries/)"))?;
    let mut child = std::process::Command::new(&bin)
        .args([
            "-X",
            "compile",
            &main_file,
            "--outdir",
            &outdir_str,
            "--synctex",
        ])
        .current_dir(&dir)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("bundled tectonic spawn failed: {}", e))?;
    let (Some(stdout), Some(stderr)) = (child.stdout.take(), child.stderr.take()) else {
        let _ = child.kill();
        let _ = child.wait();
        return Err(String::from("sidecar pipes unavailable"));
    };
    state.0.lock().unwrap().replace(child);

    let app_clone = app.clone();
    let stdout_handle = std::thread::spawn(move || {
        use std::io::{BufRead, BufReader};
        let reader = BufReader::new(stdout);
        for line in reader.lines().map_while(Result::ok) {
            let _ = app_clone.emit("compile-line", line);
        }
    });

    let mut collected: Vec<String> = Vec::new();
    {
        use std::io::{BufRead, BufReader};
        let reader = BufReader::new(stderr);
        for line in reader.lines().map_while(Result::ok) {
            let _ = app.emit("compile-line", line.clone());
            collected.push(line);
        }
    }
    let _ = stdout_handle.join();
    // Ownership is decided by one take() before the waiter starts: empty
    // means cancel won, full means this waiter solely owns the child.
    // Exactly one owner reaps.
    let outcome = match state.0.lock().unwrap().take() {
        None => core::JobStatus::Cancelled,
        Some(c) => {
            let (tx, rx) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                tx.send(core::wait_for_child(c, COMPILE_TIMEOUT_SECS)).ok();
            });
            match rx.recv().unwrap_or(core::JobOutcome::Cancelled) {
                core::JobOutcome::Cancelled => core::JobStatus::Cancelled,
                core::JobOutcome::TimedOut => core::JobStatus::TimedOut,
                core::JobOutcome::Exited(s) if s.success() => core::JobStatus::Success,
                core::JobOutcome::Exited(_) => core::JobStatus::Failed,
            }
        }
    };
    match outcome {
        core::JobStatus::Cancelled => Err(String::from("compile cancelled")),
        core::JobStatus::TimedOut => Err(format!(
            "compile timed out after {}s (engine produced no exit — killed; retry or Cancel, then check the LogStream tail)",
            COMPILE_TIMEOUT_SECS
        )),
        core::JobStatus::Success => {
            let pdf = out_pdf(&outdir, &main_file);
            Ok(pdf.to_string_lossy().to_string())
        }
        core::JobStatus::Failed => {
            let tail = collected.join("\n");
            let t = &tail[..500.min(tail.len())];
            Err(format!("bundled tectonic failed: {}", t))
        }
        core::JobStatus::Running => Err(String::from("compile cancelled")),
    }
}

#[tauri::command]
pub fn cancel_compile(state: State<'_, CompileState>) -> Result<String, String> {
    match state.0.lock().unwrap().take() {
        Some(mut c) => {
            let _ = c.kill();
            let _ = c.wait();
            Ok(String::from("cancelled"))
        }
        None => Err(String::from("nothing to cancel")),
    }
}
