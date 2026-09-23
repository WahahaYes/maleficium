//! Project index commands: the desktop adapter over `core::index`. The app
//! builds the index on open, reports watcher batches, and lays its unsaved
//! buffers over it.

use crate::core::index;

/// Build the index now; returns the files listed.
#[tauri::command(async)]
pub fn index_open(root_id: String) -> Result<usize, String> {
    index::open(&root_id)
}

/// Whether a watcher now reports this root's changes.
#[tauri::command]
pub fn index_watched(root_id: String, watched: bool) -> Result<(), String> {
    index::set_watched(&root_id, watched)
}

/// Apply one watcher batch (absolute paths).
#[tauri::command(async)]
pub fn index_touch(root_id: String, paths: Vec<String>) -> Result<(), String> {
    index::touch(&root_id, &paths)
}

/// Lay unsaved buffer text over a root-relative file; `null` lifts it.
#[tauri::command]
pub fn index_overlay(root_id: String, rel: String, text: Option<String>) -> Result<(), String> {
    index::overlay(&root_id, &rel, text)
}
