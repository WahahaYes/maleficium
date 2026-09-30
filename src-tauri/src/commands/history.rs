//! History byte reads: the desktop adapter over `core::history`. Record,
//! list, retention, and batch files run through the operation contract;
//! projects are named by their session root id and bytes come back raw.

use maleficium_core::Core;
use tauri::ipc::Response;
use tauri::State;

use maleficium_core::history;

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
