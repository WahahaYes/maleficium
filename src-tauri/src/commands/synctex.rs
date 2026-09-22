//! SyncTeX commands: thin adapters over `core::synctex`. Authorization is the
//! session root the open flow registered; nothing here re-derives it.

use crate::core::{self, ForwardHit, InverseHit};

#[tauri::command]
pub fn forward_sync(
    root_id: String,
    main_rel: String,
    tex_rel: String,
    line: u32,
) -> Result<ForwardHit, String> {
    core::forward(&root_id, &main_rel, &tex_rel, line)
}

#[tauri::command]
pub fn inverse_sync(
    root_id: String,
    main_rel: String,
    page: u32,
    x: f32,
    y: f32,
) -> Result<InverseHit, String> {
    core::inverse(&root_id, &main_rel, page, x, y)
}
