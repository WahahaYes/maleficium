//! The user's widget approval actions: approve a widget at the digest they
//! reviewed, revoke one, and switch a project's auto-approval. They are
//! dedicated desktop commands on purpose, outside the shared operation
//! contract, so nothing that dispatches `api::Request` (and no MCP tool)
//! can reach them. Reading status and reviewing go through the contract.

use maleficium_core::widget_approval::{
    self, RuntimeDecisionParams, WidgetApprovalStatus, WidgetApproveParams,
    WidgetAutoApproveParams, WidgetReview, WidgetRevokeParams,
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

/// Which custom runtime of a document the user reviews.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeReviewParams {
    pub root_id: String,
    pub main_rel: String,
    /// `<name>@<major>`.
    pub runtime: String,
}

/// Which library runtime the user installs into a project.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeInstallParams {
    pub root_id: String,
    /// `<name>@<major>`.
    pub runtime: String,
}

/// Allow or deny one custom runtime at the digest shown for review; refused
/// if its package changed since.
#[tauri::command(async)]
pub fn runtime_decide(
    cx: State<'_, Core>,
    params: RuntimeDecisionParams,
) -> Result<WidgetApprovalStatus, String> {
    let status = widget_approval::decide_runtime(&cx, &params)?;
    log(widget_approval::runtime_decided_event(
        &params.root_id,
        &params,
    ));
    Ok(status)
}

/// One custom runtime's state and its files against the last decided content.
#[tauri::command(async)]
pub fn runtime_review(
    cx: State<'_, Core>,
    params: RuntimeReviewParams,
) -> Result<WidgetReview, String> {
    widget_approval::review_runtime(&cx, &params.root_id, &params.main_rel, &params.runtime)
}

/// Copy the user's library copy of a runtime into the project as
/// `runtimes/<ref>`, so the approval judges a plain project folder. Refused
/// when the library holds no such runtime or the project already has one;
/// returns the installed root-relative path.
#[tauri::command(async)]
pub fn runtime_install(
    cx: State<'_, Core>,
    params: RuntimeInstallParams,
) -> Result<String, String> {
    let reference = params.runtime.as_str();
    if !maleficium_core::runtimes::valid_ref(reference) {
        return Err(format!(
            "runtime `{reference}` is not <name>@<major> (or names a reserved runtime)"
        ));
    }
    let home = maleficium_core::data_base_dir().join("maleficium-runtimes");
    let src = dunce::canonicalize(home.join(reference))
        .map_err(|_| format!("runtime {reference} is not in the runtime library"))?;
    let base = dunce::canonicalize(&home).unwrap_or(home);
    if !src.starts_with(&base) || !src.is_dir() {
        return Err(format!("runtime {reference} is not in the runtime library"));
    }
    let root = maleficium_core::fs::session_root(&cx, &params.root_id)?;
    let rel = format!("{}/{reference}", maleficium_core::runtimes::RUNTIMES_DIR);
    let dest = root.join(&rel);
    if dest.exists() {
        return Err(format!(
            "{rel} already exists in this project: remove it before reinstalling"
        ));
    }
    let parent = root.join(maleficium_core::runtimes::RUNTIMES_DIR);
    if let Ok(meta) = std::fs::symlink_metadata(&parent) {
        if meta.file_type().is_symlink() || !meta.is_dir() {
            return Err(format!(
                "{} is a link or not a folder: install a copy of the runtime there",
                maleficium_core::runtimes::RUNTIMES_DIR
            ));
        }
    }
    std::fs::create_dir_all(&dest).map_err(|e| format!("cannot create {rel}: {e}"))?;
    let canon = dunce::canonicalize(&dest).map_err(|e| format!("cannot create {rel}: {e}"))?;
    if !canon.starts_with(&root) {
        return Err(format!("{rel} resolves outside the project"));
    }
    let mut count = 0usize;
    let mut total = 0u64;
    copy_tree(&src, &dest, &mut count, &mut total)?;
    Ok(rel)
}

/// Copy one library folder into the project: regular files only, no links,
/// within the approval's folder limits.
fn copy_tree(
    src: &std::path::Path,
    dest: &std::path::Path,
    count: &mut usize,
    total: &mut u64,
) -> Result<(), String> {
    let rd = std::fs::read_dir(src).map_err(|e| format!("cannot list {}: {e}", src.display()))?;
    for entry in rd {
        let entry = entry.map_err(|e| format!("cannot list {}: {e}", src.display()))?;
        let name = entry
            .file_name()
            .to_str()
            .ok_or_else(|| format!("a file name under {} is not UTF-8", src.display()))?
            .to_string();
        let from = entry.path();
        let meta =
            std::fs::symlink_metadata(&from).map_err(|e| format!("cannot read {name}: {e}"))?;
        if meta.file_type().is_symlink() {
            return Err(format!(
                "the library copy holds a symlink ({name}): replace it with a copy so its content can be approved"
            ));
        }
        let to = dest.join(&name);
        if meta.is_dir() {
            std::fs::create_dir(&to).map_err(|e| format!("cannot create {name}: {e}"))?;
            copy_tree(&from, &to, count, total)?;
        } else if meta.is_file() {
            if *count >= widget_approval::MAX_FILES {
                return Err(format!(
                    "the runtime holds more than {} files",
                    widget_approval::MAX_FILES
                ));
            }
            *total += meta.len();
            if *total > widget_approval::MAX_BYTES {
                return Err(format!(
                    "the runtime is larger than {} MiB",
                    widget_approval::MAX_BYTES / (1024 * 1024)
                ));
            }
            *count += 1;
            std::fs::copy(&from, &to).map_err(|e| format!("cannot copy {name}: {e}"))?;
        } else {
            return Err(format!(
                "{name} in the library copy is not a regular file or folder"
            ));
        }
    }
    Ok(())
}
