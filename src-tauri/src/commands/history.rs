//! History commands: the desktop adapter over `core::history`. Projects are
//! named by their session root id; bytes come back raw.

use maleficium_events::{BatchFile, RecordOutcome, RetentionInfo, Revision};
use tauri::ipc::Response;

use crate::core::history;

#[tauri::command]
pub fn history_record(root_id: String, rel: String, text: String) -> RecordOutcome {
    history::record(&root_id, &rel, text.as_bytes())
}

#[tauri::command]
pub fn history_list(root_id: String, rel: String) -> Vec<Revision> {
    history::list(&root_id, &rel)
}

/// The revision's exact bytes.
#[tauri::command]
pub fn history_get(root_id: String, rel: String, rev: String) -> Result<Response, String> {
    history::get(&root_id, &rel, &rev)
        .map(Response::new)
        .ok_or_else(|| "revision unavailable".to_string())
}

/// Write the revision back to disk and return its bytes.
#[tauri::command]
pub fn history_restore(root_id: String, rel: String, rev: String) -> Result<Response, String> {
    history::restore(&root_id, &rel, &rev)
        .map(Response::new)
        .ok_or_else(|| "revision unavailable".to_string())
}

#[tauri::command]
pub fn history_retention(root_id: String) -> RetentionInfo {
    history::retention(&root_id)
}

#[tauri::command]
pub fn history_batch_files(root_id: String, batch: String) -> Vec<BatchFile> {
    history::batch_files(&root_id, &batch)
}
