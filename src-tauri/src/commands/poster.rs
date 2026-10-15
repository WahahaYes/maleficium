//! Render one widget's poster: the core prepares the job (checking an html
//! widget's approval on the snapshot the job is built from), the one-shot
//! hidden renderer runs it, the core checks the PNG and writes it where the
//! caller asked. An unapproved html widget comes back as approval_required
//! and no window opens.
//!
//! A poster seeked from media ([`needs_process`]) renders in the
//! `--render-posters` subprocess instead, whose overrun the caller kills:
//! a wedged webview main thread there cannot freeze the editor.

use crate::poster;
use maleficium_core::widgets::poster::cache::{PosterRenderer, ProcessRenderer};
use maleficium_core::widgets::poster::{needs_process, PosterOutcome, PosterRequest};
use maleficium_core::Core;
use std::process::Child;
use std::sync::atomic::AtomicBool;
use std::sync::Mutex;
use tauri::{AppHandle, Runtime, State};

/// Prepare, render and write one poster in this process. Blocks a worker
/// thread, never the main thread. The `--render-posters` subprocess
/// renders every request this way.
pub fn run_here<R: Runtime>(
    app: &AppHandle<R>,
    cx: &Core,
    req: &PosterRequest,
) -> Result<PosterOutcome, String> {
    maleficium_core::widgets::poster::run(cx, req, |job| poster::render(app, job))
}

/// One poster from the editor: in this process, or in the subprocess
/// (parked in `slot`, stopped by `cancelled`) when it seeks into media.
fn run_in<R: Runtime>(
    app: &AppHandle<R>,
    cx: &Core,
    req: &PosterRequest,
    slot: &Mutex<Option<Child>>,
    cancelled: &AtomicBool,
) -> Result<PosterOutcome, String> {
    if !needs_process(cx, req) {
        return run_here(app, cx, req);
    }
    let Some(p) = ProcessRenderer::this_program() else {
        return Err(format!(
            "widget {}: no poster renderer to run it apart from the editor",
            req.widget_id
        ));
    };
    p.render_cancel(cx, std::slice::from_ref(req), slot, cancelled)
        .pop()
        .unwrap_or_else(|| Err(String::from("the poster renderer gave no answer")))
}

/// Prepare, render and write one poster from the editor. Blocks a worker
/// thread, never the main thread.
pub fn run<R: Runtime>(
    app: &AppHandle<R>,
    cx: &Core,
    req: &PosterRequest,
) -> Result<PosterOutcome, String> {
    run_in(app, cx, req, &Mutex::new(None), &AtomicBool::new(false))
}

/// The desktop app's compile-time renderer: the same one-shot hidden
/// window as the command, run in-process.
pub struct AppRenderer<R: Runtime>(pub AppHandle<R>);

impl<R: Runtime> PosterRenderer for AppRenderer<R> {
    fn render(&self, cx: &Core, reqs: &[PosterRequest]) -> Vec<Result<PosterOutcome, String>> {
        reqs.iter().map(|r| run(&self.0, cx, r)).collect()
    }

    fn render_cancel(
        &self,
        cx: &Core,
        reqs: &[PosterRequest],
        slot: &Mutex<Option<Child>>,
        cancelled: &AtomicBool,
    ) -> Vec<Result<PosterOutcome, String>> {
        // In-process renders stop between posters, so a cancel in the
        // poster phase still skips the compile's second engine run; a
        // subprocess render parks in the slot, where cancel kills it.
        let mut out = Vec::with_capacity(reqs.len());
        for r in reqs {
            if cancelled.load(std::sync::atomic::Ordering::SeqCst) {
                out.push(Err(String::from("compile cancelled")));
            } else {
                out.push(run_in(&self.0, cx, r, slot, cancelled));
            }
        }
        out
    }
}

#[tauri::command]
pub async fn render_poster<R: Runtime>(
    app: AppHandle<R>,
    cx: State<'_, Core>,
    req: PosterRequest,
) -> Result<PosterOutcome, String> {
    let cx = cx.inner().clone();
    tauri::async_runtime::spawn_blocking(move || run(&app, &cx, &req))
        .await
        .map_err(|e| format!("the poster render stopped: {e}"))?
}
