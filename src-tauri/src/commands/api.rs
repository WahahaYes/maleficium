//! One generic command over the core operation contract. Every JSON
//! operation runs through `dispatch`; the adapter adds nothing core cannot
//! own. All file IO, the index feed, and the file watcher live in core, so
//! no fs-scope grant is minted anymore (the plugin-fs watcher was its last
//! consumer). Streaming compiles and raw history bytes stay dedicated until
//! their services move into core.

use maleficium_core::api::{Request, Response};
use maleficium_core::Core;
use tauri::State;

/// Run one core operation.
#[tauri::command(async)]
pub async fn core_request(cx: State<'_, Core>, req: Request) -> Result<Response, String> {
    maleficium_core::api::dispatch(&cx, req)
}
