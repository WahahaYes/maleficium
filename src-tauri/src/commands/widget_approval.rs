//! The user's widget approval actions: approve a widget at the digest they
//! reviewed, revoke one, and switch a project's auto-approval. They are
//! dedicated desktop commands on purpose, outside the shared operation
//! contract, so nothing that dispatches `api::Request` (and no MCP tool)
//! can reach them. Reading status and reviewing go through the contract.

use maleficium_core::widget_approval::{
    self, WidgetApprovalStatus, WidgetApproveParams, WidgetAutoApproveParams, WidgetRevokeParams,
};
use maleficium_core::Core;
use tauri::State;

fn log(event: maleficium_events::BusEvent) {
    let _ = maleficium_core::eventlog::append(std::slice::from_ref(&event));
}

/// Approve one widget at the digest shown for review; refused if its folder
/// changed since.
#[tauri::command(async)]
pub fn widget_approve(
    cx: State<'_, Core>,
    params: WidgetApproveParams,
) -> Result<WidgetApprovalStatus, String> {
    let status = widget_approval::approve(&cx, &params)?;
    if let Some(e) = widget_approval::approved_event(&params.root_id, &status) {
        log(e);
    }
    Ok(status)
}

/// Revoke one widget folder; returns its canonical root-relative path.
#[tauri::command(async)]
pub fn widget_revoke(cx: State<'_, Core>, params: WidgetRevokeParams) -> Result<String, String> {
    let path = widget_approval::revoke(&cx, &params)?;
    log(widget_approval::revoked_event(&params.root_id, &path));
    Ok(path)
}

/// Turn the project's auto-approval on or off.
#[tauri::command(async)]
pub fn widget_auto_approve(
    cx: State<'_, Core>,
    params: WidgetAutoApproveParams,
) -> Result<bool, String> {
    let on = widget_approval::set_auto_approve(&cx, &params)?;
    log(widget_approval::auto_event(&params.root_id, on));
    Ok(on)
}
