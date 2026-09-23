use std::sync::Mutex;
use tauri::{AppHandle, Emitter, State};

use maleficium_events::{
    CompileFailure, CompileLine, CompileReport, CompileStream, OfflineReadiness,
};

use crate::core::{self, engine};

pub struct CompileState(pub Mutex<Option<std::process::Child>>);

impl Default for CompileState {
    fn default() -> Self {
        Self(Mutex::new(None))
    }
}

/// Runs off the main thread: a compile can take minutes (a cold cache), and
/// the window must keep painting its progress and accept Cancel meanwhile.
#[tauri::command(async)]
pub fn compile_tex(
    app: AppHandle,
    state: State<'_, CompileState>,
    root_id: String,
    main_rel: String,
    networked: Option<bool>,
) -> Result<CompileReport, String> {
    // The main file resolves inside the session root; the engine runs in its
    // directory and writes to the app-cache outdir derived from it.
    let out = core::outputs_of(&root_id, &main_rel)?;
    let _ = app.emit(
        "compile-line",
        CompileLine {
            stream: CompileStream::Status,
            text: format!(
                "sidecar compile {} in {}",
                out.main_file,
                out.dir.to_string_lossy()
            ),
            signal: None,
        },
    );

    const COMPILE_TIMEOUT_SECS: u64 = 120;
    let mut on_line = |l: &CompileLine| {
        let _ = app.emit("compile-line", l.clone());
    };
    let c = match engine::compile(
        &out,
        &state.0,
        COMPILE_TIMEOUT_SECS,
        engine::online(),
        networked.unwrap_or(false),
        &mut on_line,
    ) {
        Ok(c) => c,
        Err(message) => {
            return Ok(CompileReport {
                pdf_url: None,
                failure: Some(CompileFailure::SpawnFailed),
                missing: None,
                message,
            })
        }
    };
    core::compile::settle(&root_id, &main_rel, &out, &c);
    let failed = |failure, message| CompileReport {
        pdf_url: None,
        failure: Some(failure),
        missing: c.missing.clone(),
        message,
    };
    Ok(match c.status {
        core::JobStatus::Success => CompileReport {
            pdf_url: Some(out.outdir.join(&out.pdf_name).to_string_lossy().to_string()),
            failure: None,
            missing: c.missing.clone(),
            message: String::new(),
        },
        core::JobStatus::Failed if c.missing.is_some() => failed(
            CompileFailure::MissingDependency,
            core::compile::failure_text(&c),
        ),
        core::JobStatus::Failed => failed(CompileFailure::EngineError, core::compile::failure_text(&c)),
        core::JobStatus::TimedOut => failed(
            CompileFailure::EngineError,
            format!(
                "compile timed out after {}s (engine produced no exit — killed; retry or Cancel, then check the LogStream tail)",
                COMPILE_TIMEOUT_SECS
            ),
        ),
        core::JobStatus::Cancelled | core::JobStatus::Running => {
            return Err(String::from("compile cancelled"))
        }
    })
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
pub fn offline_readiness(root_id: String) -> Result<OfflineReadiness, String> {
    core::compile::offline_readiness(&root_id)
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
