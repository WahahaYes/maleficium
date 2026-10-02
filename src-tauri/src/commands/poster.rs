//! Render one widget's poster: the core builds the job, the one-shot hidden
//! renderer runs it, the core checks the PNG and writes it where the caller
//! asked.

use crate::poster;
use maleficium_core::widgets::poster::cache::PosterRenderer;
use maleficium_core::widgets::poster::{finish, prepare, PosterRendered, PosterRequest};
use maleficium_core::Core;
use std::sync::Arc;
use tauri::{AppHandle, Runtime, State};

/// Prepare, render and write one poster. Blocks a worker thread, never the
/// main thread.
pub fn run<R: Runtime>(
    app: &AppHandle<R>,
    cx: &Core,
    req: &PosterRequest,
) -> Result<PosterRendered, String> {
    let job = Arc::new(prepare(cx, req)?);
    let png = poster::render(app, job.clone())?;
    finish(&job, &png)
}

/// The desktop app's compile-time renderer: the same one-shot hidden
/// window as the command, run in-process.
pub struct AppRenderer<R: Runtime>(pub AppHandle<R>);

impl<R: Runtime> PosterRenderer for AppRenderer<R> {
    fn render(&self, cx: &Core, reqs: &[PosterRequest]) -> Vec<Result<PosterRendered, String>> {
        reqs.iter().map(|r| run(&self.0, cx, r)).collect()
    }
}

#[tauri::command]
pub async fn render_poster<R: Runtime>(
    app: AppHandle<R>,
    cx: State<'_, Core>,
    req: PosterRequest,
) -> Result<PosterRendered, String> {
    let cx = cx.inner().clone();
    tauri::async_runtime::spawn_blocking(move || run(&app, &cx, &req))
        .await
        .map_err(|e| format!("the poster render stopped: {e}"))?
}
