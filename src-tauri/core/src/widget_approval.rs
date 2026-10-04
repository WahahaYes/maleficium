//! Widget approvals. An `html` widget is author code, so it runs only when
//! the user approved its folder at its current content digest; first-party
//! runtime widgets (model, video, table, chart) are exempt.
//!
//! An approval binds the project (its canonical root), the widget folder
//! (canonical, root-relative), the folder's content digest and the origins
//! it declares. Approvals live in the app data dir, keyed by the project
//! path, never in the project: a cloned or downloaded project carries none,
//! and nothing inside a project is read as one. Any edit changes the digest
//! and the widget is unapproved again.
//!
//! Auto-approval is a per-project setting, off by default: content changes
//! are approved without asking, but an origin the last explicit approval
//! did not cover still needs the user, and so does a revoked widget.
//!
//! Who may write: only the app's user actions ([`approve`], [`revoke`],
//! [`set_auto_approve`]), reached through dedicated desktop commands. They
//! are not operations of the shared contract (`api::Request`) and no MCP
//! tool calls them; MCP reads status and the mode only. A malformed or
//! unreadable store fails closed: nothing is approved and auto is off.
//!
//! Check and use share one read: [`check_widget_approval`] returns the
//! snapshot whose bytes it hashed, so the caller runs exactly what was
//! judged, never a second read of the folder.
//!
//! Custom runtimes (`runtimes/<name>@<major>/` in the project) are judged
//! the same way, per runtime ref: allowed or denied by the user at a digest
//! ([`decide_runtime`], user only, like [`approve`]). Auto-approval never
//! covers a first decision, a denial, or a change of licence or vendored
//! libraries; a denial is sticky across digests. An invalid or missing
//! package is never judged. [`check_runtime`] returns the snapshot it
//! judged, and the exporter folds exactly those files.

use crate::runtimes::RuntimeManifest;
use crate::widgets::{Widget, WidgetCsp, WidgetType};
use crate::Core;

pub use maleficium_events::RuntimeDecision;
use maleficium_events::{Actor, AppEvent, BusEvent, EventKind, EventScope, WidgetApprovalCause};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use ts_rs::TS;

/// Where the user approves widgets; every approval_required result names it.
pub const PANEL: &str = "View > Widgets";
/// Domain tag hashed first: a digest of another format can never collide.
const DIGEST_DOMAIN: &[u8] = b"maleficium widget digest 1\0";
/// A widget folder larger than this is refused (never approvable).
pub const MAX_FILES: usize = 4096;
pub const MAX_BYTES: u64 = 64 * 1024 * 1024;
const STORE_FORMAT: u32 = 2;
const STORE_FILE: &str = "store.json";
const BLOBS: &str = "blobs";
/// Review text is shown up to this size; larger or binary files are listed.
const REVIEW_TEXT_MAX: usize = 256 * 1024;

/// What an approval covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalKind {
    /// An author-supplied html widget folder.
    HtmlWidget,
    /// A custom runtime package of the project (`runtimes/<ref>`).
    CustomRuntime,
}

/// How an approved widget got its approval.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum ApprovedVia {
    /// The user approved exactly this digest.
    User,
    /// The project's auto-approval covers it (a content change, no origin
    /// beyond the last explicit approval).
    Auto,
}

/// One html widget to check: its id (for people), its folder, relative to
/// the project root (the binding), and the origins its macro options
/// declare (the folder's `widget.json` adds its own when snapshotted).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct WidgetTarget {
    pub id: String,
    pub path: String,
    #[serde(default)]
    pub option_origins: WidgetCsp,
}

impl WidgetTarget {
    /// The approval target of a listed widget of `main_rel`, or `None` for a
    /// first-party runtime widget, which needs no approval.
    pub fn of(main_rel: &str, w: &Widget) -> Result<Option<Self>, String> {
        if w.kind != WidgetType::Html {
            return Ok(None);
        }
        let bundle = w
            .sources
            .iter()
            .find(|s| s.role == "bundle")
            .ok_or_else(|| format!("widget {}: an html widget records no bundle", w.id))?;
        let dir = Path::new(main_rel)
            .parent()
            .unwrap_or(Path::new(""))
            .join(bundle.path.trim_end_matches('/'));
        Ok(Some(Self {
            id: w.id.clone(),
            path: dir.to_string_lossy().replace('\\', "/"),
            option_origins: crate::widgets::option_origins(&w.options),
        }))
    }
}

/// A widget folder read once: the bytes that were hashed, so the caller
/// uses exactly what was judged.
#[derive(Debug, Clone, PartialEq)]
pub struct WidgetSnapshot {
    /// The canonical folder.
    pub dir: PathBuf,
    /// The canonical folder relative to the root, `/`-separated (`.` for the
    /// root itself): the approval key.
    pub path: String,
    /// Every regular file, by `/`-separated path relative to the folder.
    pub files: BTreeMap<String, Vec<u8>>,
    /// The origins the folder's `widget.json` declares, sorted, deduped.
    pub origins: WidgetCsp,
    pub digest: String,
}

/// Approved: the widget may run, exactly as snapshotted.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct WidgetApproved {
    pub kind: ApprovalKind,
    pub widget: String,
    pub path: String,
    pub digest: String,
    pub via: ApprovedVia,
    pub declared_origins: WidgetCsp,
    /// The digest the user last approved, when it differs (auto-approved).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub approved_digest: Option<String>,
    pub auto_approve: bool,
    /// The package, for a custom runtime.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub runtime: Option<Box<RuntimeInfo>>,
}

/// One vendored library of a runtime, as an approval records it.
#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, schemars::JsonSchema, TS,
)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VendoredId {
    pub name: String,
    pub version: String,
    pub license: String,
}

/// What the user decides on: a custom runtime package's facts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeInfo {
    /// `<name>@<major>`.
    pub reference: String,
    pub version: String,
    pub title: String,
    pub description: String,
    pub authors: Vec<String>,
    pub license: String,
    pub vendored: Vec<VendoredId>,
    pub webgl: bool,
    /// The widgets of the document that use it, in document order.
    pub widgets: Vec<String>,
    /// The static scan's warnings (`<file>:<line>: <message>`).
    pub warnings: Vec<String>,
}

/// Not approved: a normal result (never a tool error) that tells an agent
/// what happened and what only the user can do about it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalRequired {
    pub kind: ApprovalKind,
    pub widget: String,
    pub path: String,
    pub digest: String,
    pub cause: WidgetApprovalCause,
    pub declared_origins: WidgetCsp,
    /// The digest the user last approved, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub approved_digest: Option<String>,
    /// The origins that approval covered, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub approved_origins: Option<WidgetCsp>,
    pub auto_approve: bool,
    /// Where in the app the user approves it.
    pub panel: String,
    /// What the calling operation did instead of running the widget.
    pub what_happens: String,
    pub user_action: String,
    pub agent_must_not: Vec<String>,
    /// Plain language an agent can relay to the user.
    pub message: String,
    /// The package, for a custom runtime.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub runtime: Option<Box<RuntimeInfo>>,
}

/// One html widget's approval state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema, TS)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum WidgetApprovalStatus {
    Approved(WidgetApproved),
    ApprovalRequired(ApprovalRequired),
}

impl WidgetApprovalStatus {
    pub fn is_approved(&self) -> bool {
        matches!(self, Self::Approved(_))
    }
}

/// The verdict on a widget and the snapshot it was reached on.
#[derive(Debug, Clone, PartialEq)]
pub struct Checked {
    pub status: WidgetApprovalStatus,
    pub snapshot: WidgetSnapshot,
}

/// An html widget whose folder cannot be read for approval (a symlink, a
/// special file, too large, missing): it never runs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct WidgetUnavailable {
    pub widget: String,
    pub path: String,
    pub error: String,
}

/// Every html widget of a compiled main file with its approval state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct WidgetsStatus {
    /// The project's auto-approval setting (only the user changes it).
    pub auto_approve: bool,
    /// How many widgets wait for the user.
    pub pending: u32,
    pub widgets: Vec<WidgetApprovalStatus>,
    /// Every custom runtime the document uses, one entry per ref (`widget`
    /// is the first widget using it, `path` is `runtimes/<ref>`).
    pub runtimes: Vec<WidgetApprovalStatus>,
    /// Widget folders and runtime packages that cannot be judged (missing,
    /// invalid, unreadable): they never run.
    pub unavailable: Vec<WidgetUnavailable>,
    /// First-party runtime widgets: they need no approval.
    pub exempt: Vec<String>,
    /// Set when the approval store could not be read: it counts as empty.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub store_error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum FileChange {
    Added,
    Removed,
    Modified,
    Unchanged,
}

/// One file of a widget folder against the last approved content.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct ReviewFile {
    pub path: String,
    pub change: FileChange,
    /// Approved text; absent for an added, binary or large file.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub before: Option<String>,
    /// Current text; absent for a removed, binary or large file.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub after: Option<String>,
    /// Shown without text: not UTF-8, or over the review size.
    pub binary: bool,
}

/// What the user reviews before approving: the current state and each file
/// against the last approved content.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct WidgetReview {
    pub status: WidgetApprovalStatus,
    pub files: Vec<ReviewFile>,
}

/// The user's approval of one widget at the digest they reviewed.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct WidgetApproveParams {
    pub root_id: String,
    pub main_rel: String,
    pub widget: String,
    /// The digest shown for review: approval is refused if the folder no
    /// longer hashes to it.
    pub digest: String,
}

/// The user's revocation of one widget folder (root-relative).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct WidgetRevokeParams {
    pub root_id: String,
    pub path: String,
}

/// The user's decision on one custom runtime at the digest they reviewed.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeDecisionParams {
    pub root_id: String,
    pub main_rel: String,
    /// `<name>@<major>`.
    pub runtime: String,
    /// The digest shown for review: the decision is refused if the package
    /// no longer hashes to it (a denial too).
    pub digest: String,
    pub decision: RuntimeDecision,
}

/// The user's auto-approval setting for one project.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct WidgetAutoApproveParams {
    pub root_id: String,
    pub on: bool,
}

// ---- digest ---------------------------------------------------------------

fn put(h: &mut Sha256, bytes: &[u8]) {
    h.update((bytes.len() as u64).to_le_bytes());
    h.update(bytes);
}

/// The content digest: sha256 over a domain tag, then every file as
/// length-prefixed (relative path, bytes) in sorted path order, then the
/// declared origins per directive, sorted. Length prefixes keep it
/// unambiguous; the origins are covered even when they come from outside
/// the folder.
pub fn digest(files: &BTreeMap<String, Vec<u8>>, origins: &WidgetCsp) -> String {
    let mut h = Sha256::new();
    h.update(DIGEST_DOMAIN);
    h.update((files.len() as u64).to_le_bytes());
    for (path, bytes) in files {
        put(&mut h, path.as_bytes());
        put(&mut h, bytes);
    }
    let o = normalized(origins);
    for list in [&o.connect_domains, &o.resource_domains, &o.frame_domains] {
        h.update((list.len() as u64).to_le_bytes());
        for origin in list {
            put(&mut h, origin.as_bytes());
        }
    }
    hex(&h.finalize())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn normalized(o: &WidgetCsp) -> WidgetCsp {
    let tidy = |v: &Vec<String>| {
        let mut v = v.clone();
        v.sort();
        v.dedup();
        v
    };
    WidgetCsp {
        connect_domains: tidy(&o.connect_domains),
        resource_domains: tidy(&o.resource_domains),
        frame_domains: tidy(&o.frame_domains),
    }
}

/// `now` reaches an origin `then` did not cover, for any directive.
fn widens(now: &WidgetCsp, then: &WidgetCsp) -> bool {
    let beyond = |a: &Vec<String>, b: &Vec<String>| a.iter().any(|o| !b.contains(o));
    beyond(&now.connect_domains, &then.connect_domains)
        || beyond(&now.resource_domains, &then.resource_domains)
        || beyond(&now.frame_domains, &then.frame_domains)
}

fn origin_list(o: &WidgetCsp) -> String {
    let all: Vec<&str> = o
        .connect_domains
        .iter()
        .chain(&o.resource_domains)
        .chain(&o.frame_domains)
        .map(String::as_str)
        .collect();
    all.join(", ")
}

// ---- snapshot -------------------------------------------------------------

/// Read one regular file found by the walk, refusing it if the path now
/// names something else (swapped for a symlink or another file between the
/// listing and the open) or resolves outside the folder.
fn read_pinned(
    path: &Path,
    listed: &std::fs::Metadata,
    dir: &Path,
    noun: &str,
) -> Result<Vec<u8>, String> {
    use std::io::Read as _;
    let shown = path.display();
    let f = std::fs::File::open(path).map_err(|e| format!("cannot read {shown}: {e}"))?;
    let m = f
        .metadata()
        .map_err(|e| format!("cannot read {shown}: {e}"))?;
    if !m.is_file() {
        return Err(format!("{shown} is not a regular file"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt as _;
        if (m.dev(), m.ino()) != (listed.dev(), listed.ino()) {
            return Err(format!("{shown} changed while the folder was read"));
        }
    }
    #[cfg(not(unix))]
    let _ = listed;
    let canon = dunce::canonicalize(path).map_err(|e| format!("cannot resolve {shown}: {e}"))?;
    if !canon.starts_with(dir) {
        return Err(format!("{shown} resolves outside the {noun}"));
    }
    let mut bytes = Vec::new();
    f.take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("cannot read {shown}: {e}"))?;
    Ok(bytes)
}

/// What a walk reads, for its messages.
const WIDGET_FOLDER: &str = "widget folder";
const RUNTIME_FOLDER: &str = "runtime folder";

fn walk(
    dir: &Path,
    cur: &Path,
    prefix: &str,
    files: &mut BTreeMap<String, Vec<u8>>,
    total: &mut u64,
    noun: &str,
) -> Result<(), String> {
    let rd = std::fs::read_dir(cur).map_err(|e| format!("cannot list {}: {e}", cur.display()))?;
    for entry in rd {
        let entry = entry.map_err(|e| format!("cannot list {}: {e}", cur.display()))?;
        let name = entry.file_name();
        let name = name
            .to_str()
            .ok_or_else(|| format!("a file name under {} is not UTF-8", cur.display()))?;
        let rel = if prefix.is_empty() {
            name.to_string()
        } else {
            format!("{prefix}/{name}")
        };
        let path = entry.path();
        let meta =
            std::fs::symlink_metadata(&path).map_err(|e| format!("cannot read {rel}: {e}"))?;
        let t = meta.file_type();
        if t.is_symlink() {
            return Err(format!(
                "the {noun} holds a symlink ({rel}): replace it with a copy so its content can be approved"
            ));
        }
        if t.is_dir() {
            walk(dir, &path, &rel, files, total, noun)?;
        } else if t.is_file() {
            if files.len() >= MAX_FILES {
                return Err(format!("the {noun} holds more than {MAX_FILES} files"));
            }
            let bytes = read_pinned(&path, &meta, dir, noun)?;
            *total += bytes.len() as u64;
            if *total > MAX_BYTES {
                return Err(format!(
                    "the {noun} is larger than {} MiB",
                    MAX_BYTES / (1024 * 1024)
                ));
            }
            files.insert(rel, bytes);
        } else {
            return Err(format!(
                "{rel} in the {noun} is not a regular file or folder"
            ));
        }
    }
    Ok(())
}

/// The canonical folder relative to the canonical root, `/`-separated.
fn rel_key(root: &Path, dir: &Path) -> Result<String, String> {
    let rel = dir
        .strip_prefix(root)
        .map_err(|_| "the widget folder is outside the project".to_string())?;
    let mut parts = Vec::new();
    for c in rel.components() {
        match c {
            std::path::Component::Normal(s) => parts.push(
                s.to_str()
                    .ok_or_else(|| "the widget folder's path is not UTF-8".to_string())?,
            ),
            _ => return Err("the widget folder's path is not plain".to_string()),
        }
    }
    Ok(if parts.is_empty() {
        ".".to_string()
    } else {
        parts.join("/")
    })
}

/// Read `target`'s folder once: confined to the project (no symlink out,
/// no parent escape), every file hashed with the declared origins (its
/// `widget.json` and its macro options together, checked again here).
pub fn snapshot(cx: &Core, root_id: &str, target: &WidgetTarget) -> Result<WidgetSnapshot, String> {
    let id = &target.id;
    let root = crate::fs::session_root(cx, root_id)?;
    let dir = crate::fs::resolve_in(cx, root_id, &target.path)
        .map_err(|e| format!("widget {id}: folder {}: {e}", target.path))?;
    if !dir.is_dir() {
        return Err(format!("widget {id}: {} is not a folder", target.path));
    }
    let path = rel_key(&root, &dir).map_err(|e| format!("widget {id}: {e}"))?;
    let mut files = BTreeMap::new();
    let mut total = 0;
    walk(&dir, &dir, "", &mut files, &mut total, WIDGET_FOLDER)
        .map_err(|e| format!("widget {id}: {e}"))?;
    let manifest = match files.get(crate::widgets::BUNDLE_MANIFEST) {
        Some(bytes) => {
            let text = std::str::from_utf8(bytes).map_err(|_| {
                format!(
                    "widget {id}: {} is not UTF-8",
                    crate::widgets::BUNDLE_MANIFEST
                )
            })?;
            crate::widgets::manifest_csp(id, text)?.unwrap_or_default()
        }
        None => WidgetCsp::default(),
    };
    let origins = normalized(&crate::widgets::declared_origins(
        id,
        &manifest,
        &target.option_origins,
    )?);
    let digest = digest(&files, &origins);
    Ok(WidgetSnapshot {
        dir,
        path,
        files,
        origins,
        digest,
    })
}

// ---- store ----------------------------------------------------------------

/// One approved (or revoked) widget folder of a project.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Record {
    widget: String,
    digest: String,
    origins: WidgetCsp,
    /// sha256 of each approved file, by path: their bytes sit in the blobs
    /// folder so a later change can be reviewed against them.
    files: BTreeMap<String, String>,
    approved_at: u64,
    revoked: bool,
}

/// The user's decision on one custom runtime of a project.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RuntimeRecord {
    decision: RuntimeDecision,
    digest: String,
    license: String,
    /// Sorted.
    vendored: Vec<VendoredId>,
    /// sha256 of each file at the decision, by path (blobs, for review).
    files: BTreeMap<String, String>,
    decided_at: u64,
}

/// One project's approvals.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Store {
    format: u32,
    /// The canonical project root this store belongs to.
    root: String,
    auto_approve: bool,
    widgets: BTreeMap<String, Record>,
    /// Custom runtime decisions by ref.
    runtimes: BTreeMap<String, RuntimeRecord>,
}

impl Store {
    fn empty(root: &Path) -> Self {
        Self {
            format: STORE_FORMAT,
            root: root.to_string_lossy().into_owned(),
            auto_approve: false,
            widgets: BTreeMap::new(),
            runtimes: BTreeMap::new(),
        }
    }
}

/// The approvals home: the OS app-data dir, outside every project.
pub fn store_base() -> PathBuf {
    crate::data_base_dir().join("maleficium-widgets")
}

/// `p` with its longest existing prefix canonicalized: where it would land.
fn resolved(p: &Path) -> PathBuf {
    let mut rest = Vec::new();
    let mut cur = p;
    loop {
        if let Ok(c) = dunce::canonicalize(cur) {
            return rest.iter().rev().fold(c, |acc, part| acc.join(part));
        }
        match (cur.parent(), cur.file_name()) {
            (Some(parent), Some(name)) => {
                rest.push(name.to_owned());
                cur = parent;
            }
            _ => return p.to_path_buf(),
        }
    }
}

/// Refuse a project root that overlaps the approval store (`base`): inside
/// it, or containing it. Granting one would let any tool that writes
/// project files (a replace, a restore) rewrite approvals, so no session
/// root may reach the store.
pub(crate) fn refuse_store_overlap(root: &Path, base: &Path) -> Result<(), String> {
    let store = resolved(base);
    if root.starts_with(&store) || store.starts_with(root) {
        return Err(format!(
            "forbidden path (holds the app's widget approvals): {}",
            root.display()
        ));
    }
    Ok(())
}

/// `<base>/approvals/<sha256(root)[..32]>`. Refused when it would sit
/// inside the project (an app-data dir under the project root): approvals
/// must never live in the project tree.
fn project_dir(base: &Path, root: &Path) -> Result<PathBuf, String> {
    let key = crate::bundle::sha_of(root.as_os_str().as_encoded_bytes());
    let dir = base.join("approvals").join(&key[..32]);
    if resolved(base).starts_with(root) || dir.starts_with(root) {
        return Err(format!(
            "the approval store ({}) is inside the project: approvals are never kept in a project",
            base.display()
        ));
    }
    Ok(dir)
}

/// The project's store. Missing means empty; unreadable, malformed, of
/// another format or another root means empty too, with the reason: it
/// fails closed.
fn load(base: &Path, root: &Path) -> (Store, Option<String>) {
    let dir = match project_dir(base, root) {
        Ok(d) => d,
        Err(e) => return (Store::empty(root), Some(e)),
    };
    let raw = match std::fs::read(dir.join(STORE_FILE)) {
        Ok(raw) => raw,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return (Store::empty(root), None),
        Err(e) => {
            return (
                Store::empty(root),
                Some(format!("approval store unreadable: {e}")),
            )
        }
    };
    match serde_json::from_slice::<Store>(&raw) {
        Ok(s) if s.format != STORE_FORMAT => (
            Store::empty(root),
            Some(format!("approval store has format {}", s.format)),
        ),
        Ok(s) if s.root != root.to_string_lossy() => (
            Store::empty(root),
            Some("approval store belongs to another project".to_string()),
        ),
        Ok(s) => (s, None),
        Err(e) => (
            Store::empty(root),
            Some(format!("approval store is malformed: {e}")),
        ),
    }
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    use std::io::Write as _;
    let dir = path.parent().ok_or("approval store has no folder")?;
    std::fs::create_dir_all(dir).map_err(|e| format!("approval store unreachable: {e}"))?;
    let tmp = dir.join(format!(
        ".{}.{}.{}",
        path.file_name().and_then(|n| n.to_str()).unwrap_or("x"),
        std::process::id(),
        crate::eventlog::now_ms()
    ));
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        opts.mode(0o600);
    }
    let result = (|| {
        let mut f = opts.open(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        std::fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result.map_err(|e| format!("approval store unwritable: {e}"))
}

/// Store the project's approvals and drop blobs no record names.
fn save(base: &Path, root: &Path, store: &Store) -> Result<(), String> {
    let dir = project_dir(base, root)?;
    let json = serde_json::to_vec_pretty(store).map_err(|e| e.to_string())?;
    write_atomic(&dir.join(STORE_FILE), &json)?;
    let keep: std::collections::BTreeSet<&String> = store
        .widgets
        .values()
        .flat_map(|r| r.files.values())
        .chain(store.runtimes.values().flat_map(|r| r.files.values()))
        .collect();
    if let Ok(rd) = std::fs::read_dir(dir.join(BLOBS)) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if !keep.contains(&name) {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
    Ok(())
}

fn valid_sha(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
}

fn put_blobs(
    base: &Path,
    root: &Path,
    files: &BTreeMap<String, Vec<u8>>,
) -> Result<BTreeMap<String, String>, String> {
    let blobs = project_dir(base, root)?.join(BLOBS);
    let mut out = BTreeMap::new();
    for (path, bytes) in files {
        let sha = crate::bundle::sha_of(bytes);
        let at = blobs.join(&sha);
        if !at.is_file() {
            write_atomic(&at, bytes)?;
        }
        out.insert(path.clone(), sha);
    }
    Ok(out)
}

fn get_blob(base: &Path, root: &Path, sha: &str) -> Option<Vec<u8>> {
    if !valid_sha(sha) {
        return None;
    }
    let bytes = std::fs::read(project_dir(base, root).ok()?.join(BLOBS).join(sha)).ok()?;
    (crate::bundle::sha_of(&bytes) == sha).then_some(bytes)
}

/// One writer at a time: the store is read, changed and written whole.
static WRITE: Mutex<()> = Mutex::new(());

// ---- verdict --------------------------------------------------------------

const WHAT_HAPPENS: &str = "Nothing ran: the widget keeps its poster (or a labelled placeholder) until the user approves it. The compile and the PDF are not affected.";

fn agent_must_not(what: &str) -> Vec<String> {
    vec![
        "Do not retry or poll in a loop: this stays approval_required until the user acts in the app.".to_string(),
        format!("Do not try to approve it: no tool approves {what}s or changes auto-approval, and nothing in the project counts as an approval."),
        format!("Do not rename, copy or rewrite the {what} to get around this: approval follows the folder's content digest and declared origins."),
    ]
}

fn required(
    target_id: &str,
    snap: &WidgetSnapshot,
    cause: WidgetApprovalCause,
    rec: Option<&Record>,
    auto: bool,
) -> ApprovalRequired {
    let id = target_id;
    let path = &snap.path;
    let ask = format!(
        "Ask the user to open {PANEL} in Maleficium, review its source and click Approve; until then its poster is shown."
    );
    let message = match cause {
        WidgetApprovalCause::NeverApproved => {
            format!("Widget '{id}' ({path}) has not been approved to run in this project. {ask}")
        }
        WidgetApprovalCause::ChangedSinceApproval => format!(
            "Widget '{id}' ({path}) changed since the user approved it, so it needs approval again. {ask}"
        ),
        WidgetApprovalCause::DeclaredOriginsChanged => format!(
            "Widget '{id}' ({path}) now declares network origins its approval did not cover ({}); that always needs the user's approval, even with auto-approval on. {ask}",
            origin_list(&snap.origins)
        ),
        WidgetApprovalCause::Revoked => format!(
            "The user revoked approval for widget '{id}' ({path}). Only the user can approve it again, in {PANEL}."
        ),
        // A runtime's cause; an html widget never has it.
        WidgetApprovalCause::LicenseOrVendoredChanged => format!(
            "Widget '{id}' ({path}) changed since the user approved it, so it needs approval again. {ask}"
        ),
    };
    ApprovalRequired {
        kind: ApprovalKind::HtmlWidget,
        widget: id.to_string(),
        path: path.clone(),
        digest: snap.digest.clone(),
        cause,
        declared_origins: snap.origins.clone(),
        approved_digest: rec
            .filter(|r| !r.digest.is_empty())
            .map(|r| r.digest.clone()),
        approved_origins: rec
            .filter(|r| !r.digest.is_empty())
            .map(|r| r.origins.clone()),
        auto_approve: auto,
        panel: PANEL.to_string(),
        what_happens: WHAT_HAPPENS.to_string(),
        user_action: format!(
            "In Maleficium open {PANEL}, find '{id}', review the source diff, and click Approve."
        ),
        agent_must_not: agent_must_not("widget"),
        message,
        runtime: None,
    }
}

/// The verdict for a snapshot against a project's store. Pure.
fn judge(store: &Store, id: &str, snap: &WidgetSnapshot) -> WidgetApprovalStatus {
    let auto = store.auto_approve;
    let approved = |via: ApprovedVia, rec: Option<&Record>| {
        WidgetApprovalStatus::Approved(WidgetApproved {
            kind: ApprovalKind::HtmlWidget,
            widget: id.to_string(),
            path: snap.path.clone(),
            digest: snap.digest.clone(),
            via,
            declared_origins: snap.origins.clone(),
            approved_digest: rec
                .filter(|r| r.digest != snap.digest)
                .map(|r| r.digest.clone()),
            auto_approve: auto,
            runtime: None,
        })
    };
    let need =
        |cause, rec| WidgetApprovalStatus::ApprovalRequired(required(id, snap, cause, rec, auto));
    match store.widgets.get(&snap.path) {
        None if auto && snap.origins.is_empty() => approved(ApprovedVia::Auto, None),
        None => need(WidgetApprovalCause::NeverApproved, None),
        Some(r) if r.revoked => need(WidgetApprovalCause::Revoked, Some(r)),
        Some(r) if r.digest == snap.digest => approved(ApprovedVia::User, Some(r)),
        Some(r) if widens(&snap.origins, &r.origins) => {
            need(WidgetApprovalCause::DeclaredOriginsChanged, Some(r))
        }
        Some(r) if auto => approved(ApprovedVia::Auto, Some(r)),
        Some(r) => need(WidgetApprovalCause::ChangedSinceApproval, Some(r)),
    }
}

// ---- operations -----------------------------------------------------------

/// Whether `target` may run, judged on one snapshot of its folder. The
/// renderer calls this and runs `snapshot.files` if approved, never a fresh
/// read. An error (unreadable folder, symlink, escape, too large) means it
/// does not run.
pub fn check_widget_approval(
    cx: &Core,
    root_id: &str,
    target: &WidgetTarget,
) -> Result<Checked, String> {
    check_at(&store_base(), cx, root_id, target)
}

pub(crate) fn check_at(
    base: &Path,
    cx: &Core,
    root_id: &str,
    target: &WidgetTarget,
) -> Result<Checked, String> {
    let root = crate::fs::session_root(cx, root_id)?;
    let snapshot = snapshot(cx, root_id, target)?;
    let (store, _) = load(base, &root);
    Ok(Checked {
        status: judge(&store, &target.id, &snapshot),
        snapshot,
    })
}

/// Every html widget of `main_rel`'s last compile with its approval state.
pub fn widgets_status(cx: &Core, root_id: &str, main_rel: &str) -> Result<WidgetsStatus, String> {
    status_at(&store_base(), cx, root_id, main_rel)
}

pub(crate) fn status_at(
    base: &Path,
    cx: &Core,
    root_id: &str,
    main_rel: &str,
) -> Result<WidgetsStatus, String> {
    let root = crate::fs::session_root(cx, root_id)?;
    let list = crate::widgets::widgets(cx, root_id, main_rel)?;
    let (store, store_error) = load(base, &root);
    let mut out = WidgetsStatus {
        auto_approve: store.auto_approve,
        pending: 0,
        widgets: Vec::new(),
        unavailable: Vec::new(),
        exempt: Vec::new(),
        store_error,
        runtimes: Vec::new(),
    };
    for w in &list.widgets {
        if w.kind == WidgetType::Custom {
            continue;
        }
        let Some(target) = WidgetTarget::of(main_rel, w)? else {
            out.exempt.push(w.id.clone());
            continue;
        };
        match snapshot(cx, root_id, &target) {
            Ok(snap) => {
                let s = judge(&store, &target.id, &snap);
                if !s.is_approved() {
                    out.pending += 1;
                }
                out.widgets.push(s);
            }
            Err(error) => out.unavailable.push(WidgetUnavailable {
                widget: target.id,
                path: target.path,
                error,
            }),
        }
    }
    for (reference, widgets) in runtime_users(&list.widgets) {
        let path = format!("{}/{reference}", crate::runtimes::RUNTIMES_DIR);
        let unavailable = |error: String| WidgetUnavailable {
            widget: widgets[0].clone(),
            path: path.clone(),
            error,
        };
        match snapshot_runtime(cx, root_id, &reference) {
            Ok(snap) => match &snap.manifest {
                Some(m) => {
                    let s = judge_runtime(&store, &reference, &snap, m, &widgets);
                    if !s.is_approved() {
                        out.pending += 1;
                    }
                    out.runtimes.push(s);
                }
                None => out
                    .unavailable
                    .push(unavailable(snap.invalid.clone().unwrap_or_default())),
            },
            Err(error) => out.unavailable.push(unavailable(error)),
        }
    }
    Ok(out)
}

fn target_in(
    cx: &Core,
    root_id: &str,
    main_rel: &str,
    widget: &str,
) -> Result<WidgetTarget, String> {
    let list = crate::widgets::widgets(cx, root_id, main_rel)?;
    let w = list
        .widgets
        .iter()
        .find(|w| w.id == widget)
        .ok_or_else(|| format!("{main_rel} has no widget {widget}"))?;
    WidgetTarget::of(main_rel, w)?
        .ok_or_else(|| format!("widget {widget} runs a first-party runtime: it needs no approval"))
}

/// One widget's state and its files against the last approved content.
pub fn review(
    cx: &Core,
    root_id: &str,
    main_rel: &str,
    widget: &str,
) -> Result<WidgetReview, String> {
    review_at(&store_base(), cx, root_id, main_rel, widget)
}

fn text_of(bytes: &[u8]) -> Option<String> {
    (bytes.len() <= REVIEW_TEXT_MAX)
        .then(|| std::str::from_utf8(bytes).ok().map(str::to_string))
        .flatten()
}

pub(crate) fn review_at(
    base: &Path,
    cx: &Core,
    root_id: &str,
    main_rel: &str,
    widget: &str,
) -> Result<WidgetReview, String> {
    let root = crate::fs::session_root(cx, root_id)?;
    let target = target_in(cx, root_id, main_rel, widget)?;
    let snap = snapshot(cx, root_id, &target)?;
    let (store, _) = load(base, &root);
    let status = judge(&store, &target.id, &snap);
    let approved: BTreeMap<String, String> = store
        .widgets
        .get(&snap.path)
        .map(|r| r.files.clone())
        .unwrap_or_default();
    Ok(WidgetReview {
        status,
        files: diff_files(base, &root, &snap.files, &approved),
    })
}

/// Each file of a folder now against the approved content (`approved`:
/// path to blob sha).
fn diff_files(
    base: &Path,
    root: &Path,
    current: &BTreeMap<String, Vec<u8>>,
    approved: &BTreeMap<String, String>,
) -> Vec<ReviewFile> {
    let mut files = Vec::new();
    let mut paths: std::collections::BTreeSet<&String> = current.keys().collect();
    paths.extend(approved.keys());
    for path in paths {
        let now = current.get(path);
        let then_sha = approved.get(path);
        let then = then_sha.and_then(|s| get_blob(base, root, s));
        let change = match (now, then_sha) {
            (Some(_), None) => FileChange::Added,
            (None, _) => FileChange::Removed,
            (Some(b), Some(s)) if crate::bundle::sha_of(b) == *s => FileChange::Unchanged,
            (Some(_), Some(_)) => FileChange::Modified,
        };
        let before = then.as_deref().and_then(text_of);
        let after = now.and_then(|b| text_of(b));
        let binary = now.is_some_and(|b| text_of(b).is_none())
            || then.as_deref().is_some_and(|b| text_of(b).is_none());
        files.push(ReviewFile {
            path: path.clone(),
            change,
            before,
            after,
            binary,
        });
    }
    files
}

/// The user approves `widget` of `main_rel` at `digest`, the digest they
/// reviewed. Refused if the folder no longer hashes to it. For the app's
/// user action only: no MCP tool and no shared-contract operation reach it.
pub fn approve(cx: &Core, p: &WidgetApproveParams) -> Result<WidgetApprovalStatus, String> {
    approve_at(&store_base(), cx, p)
}

pub(crate) fn approve_at(
    base: &Path,
    cx: &Core,
    p: &WidgetApproveParams,
) -> Result<WidgetApprovalStatus, String> {
    let root = crate::fs::session_root(cx, &p.root_id)?;
    let target = target_in(cx, &p.root_id, &p.main_rel, &p.widget)?;
    let _w = WRITE.lock().map_err(|_| "approval store lock poisoned")?;
    let snap = snapshot(cx, &p.root_id, &target)?;
    if snap.digest != p.digest {
        return Err(format!(
            "widget {} changed since it was shown for review (now {}): review it again",
            p.widget,
            &snap.digest[..12]
        ));
    }
    let (mut store, _) = load(base, &root);
    let files = put_blobs(base, &root, &snap.files)?;
    store.widgets.insert(
        snap.path.clone(),
        Record {
            widget: target.id.clone(),
            digest: snap.digest.clone(),
            origins: snap.origins.clone(),
            files,
            approved_at: crate::eventlog::now_ms(),
            revoked: false,
        },
    );
    save(base, &root, &store)?;
    Ok(judge(&store, &target.id, &snap))
}

/// The user revokes the widget folder at `path` (root-relative): it stays
/// unapproved, auto-approval included, until the user approves it again.
/// For the app's user action only.
pub fn revoke(cx: &Core, p: &WidgetRevokeParams) -> Result<String, String> {
    revoke_at(&store_base(), cx, p)
}

pub(crate) fn revoke_at(base: &Path, cx: &Core, p: &WidgetRevokeParams) -> Result<String, String> {
    let root = crate::fs::session_root(cx, &p.root_id)?;
    let _w = WRITE.lock().map_err(|_| "approval store lock poisoned")?;
    let (mut store, _) = load(base, &root);
    // The canonical key when the folder exists; a stored key as written
    // when the folder is gone.
    let key = match crate::fs::resolve_in(cx, &p.root_id, &p.path) {
        Ok(dir) => rel_key(&root, &dir)?,
        Err(_) if store.widgets.contains_key(&p.path) => p.path.clone(),
        Err(e) => return Err(e),
    };
    let rec = store.widgets.entry(key.clone()).or_insert_with(|| Record {
        widget: String::new(),
        digest: String::new(),
        origins: WidgetCsp::default(),
        files: BTreeMap::new(),
        approved_at: 0,
        revoked: true,
    });
    rec.revoked = true;
    save(base, &root, &store)?;
    Ok(key)
}

/// The user turns the project's auto-approval on or off. For the app's
/// user action only.
pub fn set_auto_approve(cx: &Core, p: &WidgetAutoApproveParams) -> Result<bool, String> {
    set_auto_at(&store_base(), cx, p)
}

pub(crate) fn set_auto_at(
    base: &Path,
    cx: &Core,
    p: &WidgetAutoApproveParams,
) -> Result<bool, String> {
    let root = crate::fs::session_root(cx, &p.root_id)?;
    let _w = WRITE.lock().map_err(|_| "approval store lock poisoned")?;
    let (mut store, _) = load(base, &root);
    store.auto_approve = p.on;
    save(base, &root, &store)?;
    Ok(p.on)
}

// ---- custom runtimes ------------------------------------------------------

/// A runtime package read once: every file that was hashed, the manifest
/// [`crate::runtimes::check_package`] checked on those bytes, or why the
/// package is invalid. The exporter folds exactly these files.
#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeSnapshot {
    /// The canonical folder.
    pub dir: PathBuf,
    /// `runtimes/<ref>`, root-relative: the approval path.
    pub path: String,
    pub files: BTreeMap<String, Vec<u8>>,
    pub digest: String,
    /// The checked manifest; `None` when the package is invalid.
    pub manifest: Option<RuntimeManifest>,
    /// Why the package is invalid (never approvable), if it is.
    pub invalid: Option<String>,
    /// The static scan's warnings, `<file>:<line>: <message>`.
    pub warnings: Vec<String>,
}

/// The message of a runtime folder that is not there.
pub fn not_installed(reference: &str) -> String {
    format!(
        "runtime {reference}: not installed in this project (no {}/{reference}/{})",
        crate::runtimes::RUNTIMES_DIR,
        crate::runtimes::MANIFEST
    )
}

/// Read `runtimes/<ref>` of the project once: the same confined walk as an
/// html widget folder, then the package check and the static scan. `Err`
/// only when the package is not installed; a package that cannot be read
/// or does not pass is a snapshot with `invalid` set.
pub fn snapshot_runtime(
    cx: &Core,
    root_id: &str,
    reference: &str,
) -> Result<RuntimeSnapshot, String> {
    let root = crate::fs::session_root(cx, root_id)?;
    let path = format!("{}/{reference}", crate::runtimes::RUNTIMES_DIR);
    if !crate::runtimes::valid_ref(reference) {
        return Err(format!(
            "runtime `{reference}` is not <name>@<major> (or names a reserved runtime)"
        ));
    }
    let lexical = root.join(&path);
    if std::fs::symlink_metadata(&lexical).is_err()
        || std::fs::symlink_metadata(lexical.join(crate::runtimes::MANIFEST)).is_err()
    {
        return Err(not_installed(reference));
    }
    let invalid = |dir: PathBuf, e: String| RuntimeSnapshot {
        dir,
        path: path.clone(),
        files: BTreeMap::new(),
        digest: digest(&BTreeMap::new(), &WidgetCsp::default()),
        manifest: None,
        invalid: Some(format!("runtime {reference}: {e}")),
        warnings: Vec::new(),
    };
    let dir = match crate::fs::resolve_in(cx, root_id, &path) {
        Ok(d) => d,
        Err(e) => return Ok(invalid(lexical, e)),
    };
    match rel_key(&root, &dir) {
        Ok(k) if k == path && dir.is_dir() => {}
        _ => {
            return Ok(invalid(
                dir,
                format!("{path} is a link or not a folder: install a copy of the runtime there"),
            ))
        }
    }
    let mut files = BTreeMap::new();
    let mut total = 0;
    if let Err(e) = walk(&dir, &dir, "", &mut files, &mut total, RUNTIME_FOLDER) {
        return Ok(invalid(dir, e));
    }
    let digest = digest(&files, &WidgetCsp::default());
    let (manifest, mut invalid_msg, mut warnings) =
        match crate::runtimes::check_package(reference, &files) {
            Ok(m) => (Some(m), None, Vec::new()),
            Err(e) => (None, Some(e), Vec::new()),
        };
    if let Some(m) = &manifest {
        let vendored: std::collections::BTreeSet<String> = m.vendored_files();
        let report = crate::runtimes::scan::scan(&files, &vendored);
        let line =
            |f: &crate::runtimes::scan::Finding| format!("{}:{}: {}", f.file, f.line, f.message);
        if let Some(first) = report.errors.first() {
            invalid_msg = Some(format!("runtime {reference}: {}", line(first)));
        }
        warnings = report.warnings.iter().map(line).collect();
    }
    let manifest = if invalid_msg.is_some() {
        None
    } else {
        manifest
    };
    Ok(RuntimeSnapshot {
        dir,
        path,
        files,
        digest,
        manifest,
        invalid: invalid_msg,
        warnings,
    })
}

/// A runtime package judged on one snapshot: `status` is `None` when the
/// package is invalid (never judged, never approvable).
#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeChecked {
    pub status: Option<WidgetApprovalStatus>,
    pub snapshot: RuntimeSnapshot,
}

fn vendored_ids(m: &RuntimeManifest) -> Vec<VendoredId> {
    let mut v: Vec<VendoredId> = m
        .vendored
        .iter()
        .map(|v| VendoredId {
            name: v.name.clone(),
            version: v.version.clone(),
            license: v.license.clone(),
        })
        .collect();
    v.sort();
    v
}

fn runtime_info(
    reference: &str,
    m: &RuntimeManifest,
    widgets: &[String],
    warnings: &[String],
) -> RuntimeInfo {
    RuntimeInfo {
        reference: reference.to_string(),
        version: m.version.clone(),
        title: m.title.clone(),
        description: m.description.clone(),
        authors: m.authors.clone(),
        license: m.license.clone(),
        vendored: vendored_ids(m),
        webgl: m.capabilities.webgl,
        widgets: widgets.to_vec(),
        warnings: warnings.to_vec(),
    }
}

const RUNTIME_WHAT_HAPPENS: &str = "Nothing ran: the runtime's widgets export as posters only until the user allows it. The compile and the PDF are not affected.";

/// The verdict on a valid runtime package against a project's store, one
/// row of the contract's table per arm. Pure.
fn judge_runtime(
    store: &Store,
    reference: &str,
    snap: &RuntimeSnapshot,
    m: &RuntimeManifest,
    widgets: &[String],
) -> WidgetApprovalStatus {
    let auto = store.auto_approve;
    let info = runtime_info(reference, m, widgets, &snap.warnings);
    let first = widgets.first().cloned().unwrap_or_default();
    let rec = store.runtimes.get(reference);
    let approved = |via: ApprovedVia| {
        WidgetApprovalStatus::Approved(WidgetApproved {
            kind: ApprovalKind::CustomRuntime,
            widget: first.clone(),
            path: snap.path.clone(),
            digest: snap.digest.clone(),
            via,
            declared_origins: WidgetCsp::default(),
            approved_digest: rec
                .filter(|r| r.digest != snap.digest)
                .map(|r| r.digest.clone()),
            auto_approve: auto,
            runtime: Some(Box::new(info.clone())),
        })
    };
    let need = |cause: WidgetApprovalCause| {
        let ask = format!("Ask the user to open {PANEL} in Maleficium, review the runtime and click Allow; until then its widgets export as posters only.");
        let message = match cause {
            WidgetApprovalCause::NeverApproved => format!(
                "Runtime '{reference}' (used by {}) has not been allowed in this project. {ask}",
                widgets.join(", ")
            ),
            WidgetApprovalCause::LicenseOrVendoredChanged => format!(
                "Runtime '{reference}' changed its licence or vendored libraries since the user allowed it; that always needs the user, even with auto-approval on. {ask}"
            ),
            WidgetApprovalCause::Revoked => format!(
                "The user denied runtime '{reference}' in this project. Only the user can allow it, in {PANEL}."
            ),
            WidgetApprovalCause::ChangedSinceApproval
            | WidgetApprovalCause::DeclaredOriginsChanged => format!(
                "Runtime '{reference}' changed since the user allowed it, so it needs approval again. {ask}"
            ),
        };
        WidgetApprovalStatus::ApprovalRequired(ApprovalRequired {
            kind: ApprovalKind::CustomRuntime,
            widget: first.clone(),
            path: snap.path.clone(),
            digest: snap.digest.clone(),
            cause,
            declared_origins: WidgetCsp::default(),
            approved_digest: rec
                .filter(|r| r.decision == RuntimeDecision::Allowed)
                .map(|r| r.digest.clone()),
            approved_origins: None,
            auto_approve: auto,
            panel: PANEL.to_string(),
            what_happens: RUNTIME_WHAT_HAPPENS.to_string(),
            user_action: format!(
                "In Maleficium open {PANEL}, find runtime '{reference}', review its files, and click Allow."
            ),
            agent_must_not: agent_must_not("runtime"),
            message,
            runtime: Some(Box::new(info.clone())),
        })
    };
    match rec {
        None => need(WidgetApprovalCause::NeverApproved),
        Some(r) if r.decision == RuntimeDecision::Denied => need(WidgetApprovalCause::Revoked),
        Some(r) if r.digest == snap.digest => approved(ApprovedVia::User),
        Some(r) if r.license != m.license || r.vendored != vendored_ids(m) => {
            need(WidgetApprovalCause::LicenseOrVendoredChanged)
        }
        Some(_) if auto => approved(ApprovedVia::Auto),
        Some(_) => need(WidgetApprovalCause::ChangedSinceApproval),
    }
}

/// Whether runtime `reference` (used by `widgets` of the document) may run,
/// judged on one snapshot of its package. The exporter folds
/// `snapshot.files` when approved, never a fresh read. `Err` when it is not
/// installed.
pub fn check_runtime(
    cx: &Core,
    root_id: &str,
    reference: &str,
    widgets: &[String],
) -> Result<RuntimeChecked, String> {
    check_runtime_at(&store_base(), cx, root_id, reference, widgets)
}

pub(crate) fn check_runtime_at(
    base: &Path,
    cx: &Core,
    root_id: &str,
    reference: &str,
    widgets: &[String],
) -> Result<RuntimeChecked, String> {
    let root = crate::fs::session_root(cx, root_id)?;
    let snapshot = snapshot_runtime(cx, root_id, reference)?;
    let (store, _) = load(base, &root);
    let status = snapshot
        .manifest
        .as_ref()
        .map(|m| judge_runtime(&store, reference, &snapshot, m, widgets));
    Ok(RuntimeChecked { status, snapshot })
}

/// The custom runtimes `list` uses, by ref, with the widgets using each in
/// document order.
fn runtime_users(list: &[Widget]) -> BTreeMap<String, Vec<String>> {
    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for w in list.iter().filter(|w| w.kind == WidgetType::Custom) {
        if let Some(r) = &w.runtime {
            out.entry(r.clone()).or_default().push(w.id.clone());
        }
    }
    out
}

/// A runtime the document uses, and the widgets that use it.
fn runtime_in(
    cx: &Core,
    root_id: &str,
    main_rel: &str,
    reference: &str,
) -> Result<Vec<String>, String> {
    let list = crate::widgets::widgets(cx, root_id, main_rel)?;
    runtime_users(&list.widgets)
        .remove(reference)
        .ok_or_else(|| format!("{main_rel} uses no runtime {reference}"))
}

/// A snapshot's package, or why it cannot be decided on.
fn valid_package<'a>(
    reference: &str,
    snap: &'a RuntimeSnapshot,
) -> Result<&'a RuntimeManifest, String> {
    snap.manifest.as_ref().ok_or_else(|| {
        let reason = snap.invalid.as_deref().unwrap_or("");
        let reason = reason
            .strip_prefix(&format!("runtime {reference}: "))
            .unwrap_or(reason);
        format!("runtime {reference} is not valid ({reason}); fix it before allowing it")
    })
}

/// The user allows or denies runtime `p.runtime` of `p.main_rel` at
/// `p.digest`, the digest they reviewed. Refused when the document uses no
/// such runtime, the package is missing or invalid, or it no longer hashes
/// to the digest (a denial too). For the app's user action only: no MCP
/// tool and no shared-contract operation reach it.
pub fn decide_runtime(
    cx: &Core,
    p: &RuntimeDecisionParams,
) -> Result<WidgetApprovalStatus, String> {
    decide_runtime_at(&store_base(), cx, p)
}

pub(crate) fn decide_runtime_at(
    base: &Path,
    cx: &Core,
    p: &RuntimeDecisionParams,
) -> Result<WidgetApprovalStatus, String> {
    let r = &p.runtime;
    let root = crate::fs::session_root(cx, &p.root_id)?;
    let widgets = runtime_in(cx, &p.root_id, &p.main_rel, r)?;
    let _w = WRITE.lock().map_err(|_| "approval store lock poisoned")?;
    let snap = match snapshot_runtime(cx, &p.root_id, r) {
        Ok(s) => s,
        Err(e) => {
            let reason = e.strip_prefix(&format!("runtime {r}: ")).unwrap_or(&e);
            return Err(format!(
                "runtime {r} is not valid ({reason}); fix it before allowing it"
            ));
        }
    };
    let m = valid_package(r, &snap)?;
    if snap.digest != p.digest {
        return Err(format!(
            "runtime {r} changed since it was shown for review (now {}): review it again",
            &snap.digest[..12]
        ));
    }
    let (mut store, _) = load(base, &root);
    let files = put_blobs(base, &root, &snap.files)?;
    store.runtimes.insert(
        r.clone(),
        RuntimeRecord {
            decision: p.decision,
            digest: snap.digest.clone(),
            license: m.license.clone(),
            vendored: vendored_ids(m),
            files,
            decided_at: crate::eventlog::now_ms(),
        },
    );
    save(base, &root, &store)?;
    Ok(judge_runtime(&store, r, &snap, m, &widgets))
}

/// Runtime `reference`'s state and its files against the content of the
/// user's last decision.
pub fn review_runtime(
    cx: &Core,
    root_id: &str,
    main_rel: &str,
    reference: &str,
) -> Result<WidgetReview, String> {
    review_runtime_at(&store_base(), cx, root_id, main_rel, reference)
}

pub(crate) fn review_runtime_at(
    base: &Path,
    cx: &Core,
    root_id: &str,
    main_rel: &str,
    reference: &str,
) -> Result<WidgetReview, String> {
    let root = crate::fs::session_root(cx, root_id)?;
    let widgets = runtime_in(cx, root_id, main_rel, reference)?;
    let snap = snapshot_runtime(cx, root_id, reference)?;
    let m = valid_package(reference, &snap)?;
    let (store, _) = load(base, &root);
    let status = judge_runtime(&store, reference, &snap, m, &widgets);
    let decided = store
        .runtimes
        .get(reference)
        .map(|r| r.files.clone())
        .unwrap_or_default();
    Ok(WidgetReview {
        status,
        files: diff_files(base, &root, &snap.files, &decided),
    })
}

// ---- events ---------------------------------------------------------------

fn bus(kind: EventKind, actor: Actor, message: String, event: AppEvent) -> BusEvent {
    BusEvent {
        at: crate::eventlog::now_ms(),
        scope: EventScope::App,
        kind,
        actor,
        message,
        event,
    }
}

/// The user approved a widget.
pub fn approved_event(root_id: &str, s: &WidgetApprovalStatus) -> Option<BusEvent> {
    let WidgetApprovalStatus::Approved(a) = s else {
        return None;
    };
    Some(bus(
        EventKind::Success,
        Actor::User,
        format!("approved widget {}", a.widget),
        AppEvent::WidgetApproved {
            root_id: root_id.to_string(),
            path: a.path.clone(),
            widget: a.widget.clone(),
            digest: a.digest.clone(),
        },
    ))
}

/// The user revoked a widget folder.
pub fn revoked_event(root_id: &str, path: &str) -> BusEvent {
    bus(
        EventKind::Info,
        Actor::User,
        format!("revoked widget approval for {path}"),
        AppEvent::WidgetRevoked {
            root_id: root_id.to_string(),
            path: path.to_string(),
        },
    )
}

/// The user changed the project's auto-approval.
pub fn auto_event(root_id: &str, on: bool) -> BusEvent {
    bus(
        EventKind::Info,
        Actor::User,
        format!("widget auto-approval {}", if on { "on" } else { "off" }),
        AppEvent::WidgetsAutoApprove {
            root_id: root_id.to_string(),
            on,
        },
    )
}

/// A compile found an html widget, or an export a custom runtime, waiting
/// for the user: the window opens its approval prompt from this.
pub fn approval_required_event(root_id: &str, r: &ApprovalRequired, actor: Actor) -> BusEvent {
    if let Some(rt) = &r.runtime {
        return bus(
            EventKind::Warn,
            actor,
            format!("runtime {} needs approval", rt.reference),
            AppEvent::RuntimeApprovalRequired {
                root_id: root_id.to_string(),
                runtime: rt.reference.clone(),
                digest: r.digest.clone(),
                cause: r.cause,
                widgets: rt.widgets.clone(),
            },
        );
    }
    bus(
        EventKind::Warn,
        actor,
        format!("widget {} needs approval", r.widget),
        AppEvent::WidgetApprovalRequired {
            root_id: root_id.to_string(),
            path: r.path.clone(),
            widget: r.widget.clone(),
            digest: r.digest.clone(),
            cause: r.cause,
            connect_domains: r.declared_origins.connect_domains.clone(),
            resource_domains: r.declared_origins.resource_domains.clone(),
            frame_domains: r.declared_origins.frame_domains.clone(),
        },
    )
}

/// The user allowed or denied a custom runtime.
pub fn runtime_decided_event(root_id: &str, p: &RuntimeDecisionParams) -> BusEvent {
    let verb = match p.decision {
        RuntimeDecision::Allowed => "allowed",
        RuntimeDecision::Denied => "denied",
    };
    bus(
        EventKind::Info,
        Actor::User,
        format!("{verb} runtime {}", p.runtime),
        AppEvent::RuntimeDecided {
            root_id: root_id.to_string(),
            runtime: p.runtime.clone(),
            digest: p.digest.clone(),
            decision: p.decision,
        },
    )
}

/// A widget no longer matches its approved digest; `None` otherwise.
/// Observed by whoever checked (`actor`), so the log shows who saw it.
pub fn digest_changed_event(
    root_id: &str,
    s: &WidgetApprovalStatus,
    actor: Actor,
) -> Option<BusEvent> {
    let (path, widget, approved, digest, cause, auto) = match s {
        WidgetApprovalStatus::Approved(a) => (
            &a.path,
            &a.widget,
            a.approved_digest.as_ref()?,
            &a.digest,
            WidgetApprovalCause::ChangedSinceApproval,
            true,
        ),
        WidgetApprovalStatus::ApprovalRequired(r) => match r.cause {
            WidgetApprovalCause::ChangedSinceApproval
            | WidgetApprovalCause::DeclaredOriginsChanged
            | WidgetApprovalCause::LicenseOrVendoredChanged => (
                &r.path,
                &r.widget,
                r.approved_digest.as_ref()?,
                &r.digest,
                r.cause,
                false,
            ),
            _ => return None,
        },
    };
    Some(bus(
        EventKind::Warn,
        actor,
        format!("widget {widget} changed since its approval"),
        AppEvent::WidgetDigestChanged {
            root_id: root_id.to_string(),
            path: path.clone(),
            widget: widget.clone(),
            approved_digest: approved.clone(),
            digest: digest.clone(),
            cause,
            auto_approved: auto,
        },
    ))
}

#[cfg(test)]
mod tests;
