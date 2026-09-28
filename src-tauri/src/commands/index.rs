//! Project index commands: the desktop adapter over `core::index`. The app
//! builds the index on open, reports watcher batches, and lays its unsaved
//! buffers over it.

use maleficium_core::Core;
use maleficium_index::definition::Lookup;
use maleficium_index::replace::{ReplaceApplied, ReplacePreview};
use maleficium_index::search::{FileMatch, Query, Ranked, SearchResult};
use tauri::State;

use maleficium_core::{index, replace, search};

/// Build the index now; returns the files listed.
#[tauri::command(async)]
pub fn index_open(cx: State<'_, Core>, root_id: String) -> Result<usize, String> {
    index::open(&cx, &root_id)
}

/// Whether a watcher now reports this root's changes.
#[tauri::command]
pub fn index_watched(cx: State<'_, Core>, root_id: String, watched: bool) -> Result<(), String> {
    index::set_watched(&cx, &root_id, watched)
}

/// Apply one watcher batch (absolute paths).
#[tauri::command(async)]
pub fn index_touch(cx: State<'_, Core>, root_id: String, paths: Vec<String>) -> Result<(), String> {
    index::touch(&cx, &root_id, &paths)
}

/// Lay unsaved buffer text over a root-relative file; `null` lifts it.
#[tauri::command]
pub fn index_overlay(
    cx: State<'_, Core>,
    root_id: String,
    rel: String,
    text: Option<String>,
) -> Result<(), String> {
    index::overlay(&cx, &root_id, &rel, text)
}

/// Search the project; files of `main_rel`'s document rank first.
#[tauri::command(async)]
pub fn index_search(
    cx: State<'_, Core>,
    root_id: String,
    query: Query,
    main_rel: Option<String>,
    max: Option<usize>,
) -> Result<SearchResult, String> {
    search::search(
        &cx,
        &root_id,
        &query,
        main_rel.as_deref(),
        max.unwrap_or(search::MAX_HITS),
    )
}

/// The best files for a finder query.
#[tauri::command(async)]
pub fn index_find_files(
    cx: State<'_, Core>,
    root_id: String,
    query: String,
    max: Option<usize>,
) -> Result<Vec<FileMatch>, String> {
    search::find_files(
        &cx,
        &root_id,
        &query,
        max.unwrap_or(search::MAX_FILE_MATCHES),
    )
}

/// Plan a replace across the project; writes nothing.
#[tauri::command(async)]
pub fn index_replace_preview(
    cx: State<'_, Core>,
    root_id: String,
    query: Query,
    replacement: String,
    main_rel: Option<String>,
) -> Result<ReplacePreview, String> {
    replace::preview(&cx, &root_id, &query, &replacement, main_rel.as_deref())
}

/// Apply a previewed replace. Files in `keep_open` (open editor buffers) are
/// not written; their new text comes back for the editor.
#[tauri::command(async)]
pub fn index_replace_apply(
    cx: State<'_, Core>,
    root_id: String,
    token: String,
    keep_open: Vec<String>,
) -> Result<ReplaceApplied, String> {
    replace::apply(&cx, &root_id, &token, &keep_open)
}

/// Rank a caller's list of names by the finder's fuzzy score.
#[tauri::command]
pub fn fuzzy_rank(query: String, items: Vec<String>, max: Option<usize>) -> Vec<Ranked> {
    maleficium_index::search::rank(&query, &items, max.unwrap_or(items.len()))
}

/// What a column of a line the editor holds refers to, and where it is
/// defined; `null` when nothing sits there.
#[tauri::command(async)]
pub fn index_definition_at(
    cx: State<'_, Core>,
    root_id: String,
    line: String,
    col: u32,
    main_rel: Option<String>,
) -> Result<Option<Lookup>, String> {
    search::definition_at(&cx, &root_id, &line, col, main_rel.as_deref())
}
