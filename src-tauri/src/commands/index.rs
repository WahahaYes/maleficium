//! Project index commands: the desktop adapter over `core::index`. The app
//! builds the index on open, reports watcher batches, and lays its unsaved
//! buffers over it.

use maleficium_index::search::{FileMatch, Query, SearchResult};

use crate::core::{index, search};

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

/// Search the project; files of `main_rel`'s document rank first.
#[tauri::command(async)]
pub fn index_search(
    root_id: String,
    query: Query,
    main_rel: Option<String>,
    max: Option<usize>,
) -> Result<SearchResult, String> {
    search::search(
        &root_id,
        &query,
        main_rel.as_deref(),
        max.unwrap_or(search::MAX_HITS),
    )
}

/// The best files for a finder query.
#[tauri::command(async)]
pub fn index_find_files(
    root_id: String,
    query: String,
    max: Option<usize>,
) -> Result<Vec<FileMatch>, String> {
    search::find_files(&root_id, &query, max.unwrap_or(search::MAX_FILE_MATCHES))
}
