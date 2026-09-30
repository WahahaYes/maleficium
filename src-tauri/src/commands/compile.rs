//! Desktop compile runs: a thin adapter over the core job service. The run
//! itself (registry, timeout, report) lives in `core::compile`; this layer
//! only forwards lines to the window and keeps the command async so the
//! webview paints progress and accepts Cancel meanwhile.

use maleficium_core::Core;
use tauri::{AppHandle, Emitter, State};

use maleficium_core::compile::EventSink;
use maleficium_events::{CompileLine, CompileReport};

struct WindowSink {
    app: AppHandle,
}

impl EventSink for WindowSink {
    fn push(&mut self, line: &CompileLine) {
        let _ = self.app.emit("compile-line", line.clone());
    }
}

/// Runs off the main thread: a compile can take minutes (a cold cache), and
/// the window must keep painting its progress and accept Cancel meanwhile.
#[tauri::command(async)]
pub fn compile_tex(
    cx: State<'_, Core>,
    app: AppHandle,
    root_id: String,
    main_rel: String,
    networked: Option<bool>,
) -> Result<CompileReport, String> {
    let mut sink = WindowSink { app };
    maleficium_core::compile::run_blocking(
        &cx,
        &root_id,
        &main_rel,
        networked.unwrap_or(false),
        &mut sink,
    )
}

#[tauri::command]
pub fn cancel_compile(cx: State<'_, Core>) -> Result<String, String> {
    maleficium_core::compile::cancel_current(&cx)
}
