use std::sync::Mutex;
use tauri::{AppHandle, Emitter, State};

use maleficium_events::{CompileLine, CompileStream};

use crate::core;

pub struct CompileState(pub Mutex<Option<std::process::Child>>);

impl Default for CompileState {
    fn default() -> Self {
        Self(Mutex::new(None))
    }
}

#[tauri::command]
pub fn compile_tex(
    app: AppHandle,
    state: State<'_, CompileState>,
    root_id: String,
    main_rel: String,
) -> Result<String, String> {
    // The main file resolves inside the session root; the engine runs in its
    // directory and writes to the app-cache outdir derived from it.
    let core::MainOutputs {
        dir,
        main_file,
        outdir,
        pdf_name,
    } = core::outputs_of(&root_id, &main_rel)?;
    let _ = std::fs::create_dir_all(&outdir);
    let outdir_str = outdir.to_string_lossy().to_string();
    let _ = app.emit(
        "compile-line",
        CompileLine {
            stream: CompileStream::Status,
            text: format!("sidecar compile {} in {}", main_file, dir.to_string_lossy()),
        },
    );

    const COMPILE_TIMEOUT_SECS: u64 = 120;
    let bin = core::sidecar_path_for("tectonic")?;
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
        let mut lines = Vec::new();
        for line in reader.lines().map_while(Result::ok) {
            let _ = app_clone.emit(
                "compile-line",
                CompileLine {
                    stream: CompileStream::Stdout,
                    text: line.clone(),
                },
            );
            lines.push(line);
        }
        lines
    });

    let mut collected: Vec<String> = Vec::new();
    {
        use std::io::{BufRead, BufReader};
        let reader = BufReader::new(stderr);
        for line in reader.lines().map_while(Result::ok) {
            let _ = app.emit(
                "compile-line",
                CompileLine {
                    stream: CompileStream::Stderr,
                    text: line.clone(),
                },
            );
            collected.push(line);
        }
    }
    let mut console = stdout_handle.join().unwrap_or_default();
    console.extend(collected.iter().cloned());
    core::write_engine_log(&core::log_file(&outdir, &main_file), &console);
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
        core::JobStatus::Success => Ok(outdir.join(&pdf_name).to_string_lossy().to_string()),
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

#[tauri::command]
pub fn engine_log(root_id: String, main_rel: String) -> Result<String, String> {
    core::engine_log(&root_id, &main_rel)
}

#[tauri::command]
pub fn outputs_fresh(root_id: String, main_rel: String) -> Result<bool, String> {
    core::outputs_fresh(&root_id, &main_rel)
}

#[tauri::command]
pub fn clean_outputs(root_id: String, main_rel: String) -> Result<usize, String> {
    core::clean_outputs(&root_id, &main_rel)
}
