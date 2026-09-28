//! History commands: the desktop adapter over `core::history`. Projects are
//! named by their session root id; bytes come back raw.

use maleficium_core::Core;
use maleficium_events::{BatchFile, RecordOutcome, RetentionInfo, Revision};
use tauri::ipc::Response;
use tauri::State;

use maleficium_core::history;

#[tauri::command]
pub fn history_record(
    cx: State<'_, Core>,
    root_id: String,
    rel: String,
    text: String,
) -> RecordOutcome {
    history::record(&cx, &root_id, &rel, text.as_bytes())
}

#[tauri::command]
pub fn history_list(cx: State<'_, Core>, root_id: String, rel: String) -> Vec<Revision> {
    history::list(&cx, &root_id, &rel)
}

/// The revision's exact bytes.
#[tauri::command]
pub fn history_get(
    cx: State<'_, Core>,
    root_id: String,
    rel: String,
    rev: String,
) -> Result<Response, String> {
    history::get(&cx, &root_id, &rel, &rev)
        .map(Response::new)
        .ok_or_else(|| "revision unavailable".to_string())
}

/// Write the revision back to disk and return its bytes.
#[tauri::command]
pub fn history_restore(
    cx: State<'_, Core>,
    root_id: String,
    rel: String,
    rev: String,
) -> Result<Response, String> {
    history::restore(&cx, &root_id, &rel, &rev)
        .map(Response::new)
        .ok_or_else(|| "revision unavailable".to_string())
}

#[tauri::command]
pub fn history_retention(cx: State<'_, Core>, root_id: String) -> RetentionInfo {
    history::retention(&cx, &root_id)
}

#[tauri::command]
pub fn history_batch_files(cx: State<'_, Core>, root_id: String, batch: String) -> Vec<BatchFile> {
    history::batch_files(&cx, &root_id, &batch)
}
