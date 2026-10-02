//! Running a paper's widgets live in the app: the per-project approvals and
//! what a widget host needs to run each widget (its folded document, its
//! policy, its source bytes). A project runs nothing until the user approves
//! running its widgets, and no widget reaches the network until the user
//! separately approves network access. Both approvals are stored per
//! project outside it. Only the app's own user action sets them: no
//! operation of the shared contract and no automation tool writes one.

use crate::bundle::{self, fold};
use crate::widgets::{Widget, WidgetType};
use crate::Core;

pub use maleficium_events::WidgetApprovalScope;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use ts_rs::TS;

/// A project's widget approvals.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct WidgetApproval {
    /// Widgets may run live (otherwise they stay posters).
    pub run: bool,
    /// Live widgets may reach the origins they declare.
    pub network: bool,
}

fn store_path() -> PathBuf {
    crate::data_base_dir()
        .join("maleficium-widgets")
        .join("approvals.json")
}

fn load(path: &Path) -> HashMap<String, WidgetApproval> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

fn store(path: &Path, map: &HashMap<String, WidgetApproval>) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("approval store unreachable: {e}"))?;
    }
    let json = serde_json::to_string_pretty(map).map_err(|e| e.to_string())?;
    std::fs::write(path, json).map_err(|e| format!("approval store unwritable: {e}"))
}

fn key(cx: &Core, root_id: &str) -> Result<String, String> {
    Ok(crate::fs::session_root(cx, root_id)?
        .to_string_lossy()
        .into_owned())
}

fn get_at(path: &Path, key: &str) -> WidgetApproval {
    load(path).remove(key).unwrap_or_default()
}

fn set_at(
    path: &Path,
    key: String,
    scope: WidgetApprovalScope,
    granted: bool,
) -> Result<WidgetApproval, String> {
    let mut map = load(path);
    let a = map.entry(key).or_default();
    match scope {
        WidgetApprovalScope::Run => a.run = granted,
        WidgetApprovalScope::Network => a.network = granted,
    }
    let out = *a;
    map.retain(|_, a| a.run || a.network);
    store(path, &map)?;
    Ok(out)
}

/// The project's approvals; none until the user grants one.
pub fn approval(cx: &Core, root_id: &str) -> Result<WidgetApproval, String> {
    Ok(get_at(&store_path(), &key(cx, root_id)?))
}

/// Grant or revoke one approval for the project and store the result. For
/// the app's user action only.
pub fn set_approval(
    cx: &Core,
    root_id: &str,
    scope: WidgetApprovalScope,
    granted: bool,
) -> Result<WidgetApproval, String> {
    set_at(&store_path(), key(cx, root_id)?, scope, granted)
}

/// The bus event for an approval change.
pub fn approval_event(
    root_id: &str,
    scope: WidgetApprovalScope,
    granted: bool,
    a: &WidgetApproval,
) -> maleficium_events::BusEvent {
    use maleficium_events::{Actor, AppEvent, BusEvent, EventKind, EventScope};
    let what = match scope {
        WidgetApprovalScope::Run => "running widgets",
        WidgetApprovalScope::Network => "widget network access",
    };
    BusEvent {
        at: crate::eventlog::now_ms(),
        scope: EventScope::App,
        kind: EventKind::Info,
        actor: Actor::User,
        message: format!("{} {what}", if granted { "approved" } else { "revoked" }),
        event: AppEvent::WidgetsApproval {
            root_id: root_id.to_string(),
            scope,
            granted,
            run: a.run,
            network: a.network,
        },
    }
}

/// One runtime input as bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveSource {
    pub role: String,
    pub name: String,
    pub mime: String,
    pub sha256: String,
    pub bytes: Vec<u8>,
}

/// One widget, ready for a widget host.
#[derive(Debug, Clone, PartialEq)]
pub struct LiveWidget {
    pub id: String,
    /// One inline document, its policy meta first.
    pub document: String,
    /// The same policy, for the response header.
    pub csp: String,
    /// `table@1`, or empty for an author bundle.
    pub runtime: String,
    pub alt: String,
    pub options: Value,
    pub sources: Vec<LiveSource>,
    /// The widget may reach its declared origins (it declares some and the
    /// project approved network access).
    pub network: bool,
}

/// Why a widget shows its poster instead of running.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum PosterReason {
    /// The project has not approved running widgets.
    NotApproved,
    /// This build has no runtime for the widget's type.
    NoRuntime,
}

/// One widget as the preview shows it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct WidgetPlanEntry {
    pub id: String,
    /// Absent when the widget can run.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub poster: Option<PosterReason>,
    /// The widget declares origins it wants to reach.
    pub declares_network: bool,
    /// It may reach them (declared and approved).
    pub network: bool,
}

/// What the preview needs to show a paper's widgets: the approvals, each
/// widget's mode, and the widget-host capabilities missing here (any
/// missing one keeps every widget a poster).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct WidgetSession {
    pub approval: WidgetApproval,
    pub entries: Vec<WidgetPlanEntry>,
    pub unavailable: Vec<String>,
}

/// The widgets of a compiled main file, split into what runs and what stays
/// a poster.
#[derive(Debug, Clone, PartialEq)]
pub struct LivePlan {
    pub approval: WidgetApproval,
    pub entries: Vec<WidgetPlanEntry>,
    pub live: Vec<LiveWidget>,
}

fn source(
    cx: &Core,
    root_id: &str,
    main_dir_rel: &Path,
    role: String,
    rel: &str,
) -> Result<LiveSource, String> {
    let path = crate::fs::resolve_in(cx, root_id, &main_dir_rel.join(rel).to_string_lossy())?;
    let bytes = std::fs::read(&path).map_err(|e| format!("cannot read {rel}: {e}"))?;
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    Ok(LiveSource {
        role,
        name: path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        mime: fold::mime_for(ext).to_string(),
        sha256: bundle::sha_of(&bytes),
        bytes,
    })
}

/// The live form of one widget, or `None` when this build cannot run it.
fn live_widget(
    cx: &Core,
    root_id: &str,
    main_dir_rel: &Path,
    w: &Widget,
    network: bool,
) -> Result<Option<LiveWidget>, String> {
    let policy = fold::widget_policy(if network { w.csp.as_ref() } else { None });
    let (document, runtime, sources) = if w.kind == WidgetType::Html {
        let dir = bundle::bundle_folder(cx, root_id, main_dir_rel, w)?;
        let folded = fold::fold_bundle(&dir, &dir.join("index.html"), &policy)
            .map_err(|e| format!("widget {}: {e}", w.id))?;
        (folded.html, String::new(), Vec::new())
    } else {
        let Some(host) = w.runtime.as_deref().and_then(bundle::runtime_host) else {
            return Ok(None);
        };
        let mut sources = Vec::new();
        for (role, s) in bundle::role_keys(w)? {
            sources.push(
                source(cx, root_id, main_dir_rel, role, &s.path)
                    .map_err(|e| format!("widget {}: {e}", w.id))?,
            );
        }
        (
            fold::with_policy(host, &policy),
            w.runtime.clone().unwrap_or_default(),
            sources,
        )
    };
    Ok(Some(LiveWidget {
        id: w.id.clone(),
        document,
        csp: policy,
        runtime,
        alt: w.alt.clone(),
        options: Value::Object(bundle::runtime_options(w)),
        sources,
        network,
    }))
}

/// What the preview runs for `main_rel`'s last compile, given the
/// project's approvals.
pub fn live_plan(cx: &Core, root_id: &str, main_rel: &str) -> Result<LivePlan, String> {
    live_plan_with(cx, root_id, main_rel, approval(cx, root_id)?)
}

fn live_plan_with(
    cx: &Core,
    root_id: &str,
    main_rel: &str,
    approval: WidgetApproval,
) -> Result<LivePlan, String> {
    let list = crate::widgets::widgets(cx, root_id, main_rel)?;
    let main_dir_rel = Path::new(main_rel)
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_default();
    let mut entries = Vec::new();
    let mut live = Vec::new();
    for w in &list.widgets {
        let declares_network = w.csp.is_some();
        let network = approval.run && approval.network && declares_network;
        let lw = if approval.run {
            live_widget(cx, root_id, &main_dir_rel, w, network)?
        } else {
            None
        };
        let poster = match (&lw, approval.run) {
            (_, false) => Some(PosterReason::NotApproved),
            (None, true) => Some(PosterReason::NoRuntime),
            (Some(_), true) => None,
        };
        entries.push(WidgetPlanEntry {
            id: w.id.clone(),
            poster,
            declares_network,
            network: network && lw.is_some(),
        });
        live.extend(lw);
    }
    Ok(LivePlan {
        approval,
        entries,
        live,
    })
}

/// The house theme's tokens for a colour mode, as widgets receive them.
pub fn house_tokens(dark: bool) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for rule in bundle::THEME_CSS.split('}') {
        let Some((selector, body)) = rule.split_once('{') else {
            continue;
        };
        if selector.contains("dark") != dark && selector.contains("dark") {
            continue;
        }
        for decl in body.split(';') {
            if let Some((k, v)) = decl.split_once(':') {
                let k = k.trim();
                if k.starts_with("--m-") {
                    out.insert(k.to_string(), v.trim().to_string());
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests;
