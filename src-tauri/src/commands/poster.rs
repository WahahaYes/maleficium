//! Render one widget's poster: the core prepares the job (checking an html
//! widget's approval on the snapshot the job is built from), the one-shot
//! hidden renderer runs it, the core checks the PNG and writes it where the
//! caller asked. An unapproved html widget comes back as approval_required
//! and no window opens.

use crate::poster;
use maleficium_core::widgets::poster::cache::PosterRenderer;
use maleficium_core::widgets::poster::{PosterOutcome, PosterRequest};
use maleficium_core::Core;
use tauri::{AppHandle, Runtime, State};

/// Prepare, render and write one poster. Blocks a worker thread, never the
/// main thread.
pub fn run<R: Runtime>(
    app: &AppHandle<R>,
    cx: &Core,
    req: &PosterRequest,
) -> Result<PosterOutcome, String> {
    maleficium_core::widgets::poster::run(cx, req, |job| poster::render(app, job))
}

/// The desktop app's compile-time renderer: the same one-shot hidden
/// window as the command, run in-process.
pub struct AppRenderer<R: Runtime>(pub AppHandle<R>);

impl<R: Runtime> PosterRenderer for AppRenderer<R> {
    fn render(&self, cx: &Core, reqs: &[PosterRequest]) -> Vec<Result<PosterOutcome, String>> {
        reqs.iter().map(|r| run(&self.0, cx, r)).collect()
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
