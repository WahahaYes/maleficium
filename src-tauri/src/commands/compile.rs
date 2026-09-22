use std::sync::Mutex;
use tauri::{AppHandle, Emitter, State};

use maleficium_events::{CompileLine, CompileStream};

use crate::core::{
    self,
    engine::{self, DigestCheck},
};

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
        },
    );

    const COMPILE_TIMEOUT_SECS: u64 = 120;
    let mut on_line = |l: &CompileLine| {
        let _ = app.emit("compile-line", l.clone());
    };
    let a = engine::run(&out, &state.0, COMPILE_TIMEOUT_SECS, &mut on_line)?;
    core::write_engine_log(&core::log_file(&out.outdir, &out.main_file), &a.texts());
    if let DigestCheck::Changed(d) = &a.bundle {
        return Err(format!(
            "TeX bundle changed: {} now resolves to {}",
            engine::BUNDLE_URL,
            d
        ));
    }
    match a.status {
        core::JobStatus::TimedOut => Err(format!(
            "compile timed out after {}s (engine produced no exit — killed; retry or Cancel, then check the LogStream tail)",
            COMPILE_TIMEOUT_SECS
        )),
        core::JobStatus::Success => Ok(out.outdir.join(&out.pdf_name).to_string_lossy().to_string()),
        core::JobStatus::Failed => Err(core::compile::failure_text(&a.lines)),
        core::JobStatus::Cancelled | core::JobStatus::Running => {
            Err(String::from("compile cancelled"))
        }
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
