//! One generic command over the core operation contract. Every JSON
//! operation runs through `dispatch`; the adapter only adds what core cannot
//! own: the fs-scope grant beside a project grant. That grant stays until
//! the core watcher lands: plugin-fs `watch` resolves its root through the
//! scope, so dropping the grant now would silently lose external changes.
//! All file IO already goes through core. Streaming compiles and raw
//! history bytes stay dedicated until their services move into core.

use std::path::PathBuf;

use maleficium_core::api::{Request, Response};
use maleficium_core::Core;
use tauri::{AppHandle, State};

use super::guard::allow_granted;

/// Run one core operation. Grant responses also mint the recursive fs-scope
/// grant for the project, as the per-project commands did before.
#[tauri::command(async)]
pub async fn core_request(
    cx: State<'_, Core>,
    app: AppHandle,
    req: Request,
) -> Result<Response, String> {
    let resp = maleficium_core::api::dispatch(&cx, req)?;
    let grant = match &resp {
        Response::GrantProjectAccess(g) | Response::GrantUntitledAccess(g) => Some(g),
        _ => None,
    };
    if let Some(g) = grant {
        allow_granted(&app, PathBuf::from(&g.path), g.root_id.clone())?;
    }
    Ok(resp)
}
