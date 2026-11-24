//! Automation API over stdio: every tool validates its session root and
//! rejects escapes before touching the fs.
//!
//! Failures come back as tool errors (`isError: true`, the reason as text);
//! successes carry the typed record as `structuredContent` (the model and
//! text-only hosts read the same JSON in `content`).

use rmcp::{
    ServiceExt,
    handler::server::wrapper::{Json, Parameters},
    tool, tool_handler, tool_router,
    transport::stdio,
};
use serde::{Deserialize, Serialize};

use maleficium_core as core;

/// The snippet View's `ui://` resource. A contract change ships `/v2` and
/// deletes `/v1` in the same change.
const SNIPPET_VIEW_URI: &str = "ui://maleficium/snippet/v1";
const SNIPPET_VIEW_HTML: &str = include_str!("../views/snippet.html");
/// The compile dashboard View: a `paper` start job's status, phase, log
/// tail, findings and a cancel button, polled through `paper` poll.
const COMPILE_VIEW_URI: &str = "ui://maleficium/compile/v1";
const COMPILE_VIEW_HTML: &str = include_str!("../views/compile.html");
/// Every View, by the URI its tools name.
const VIEWS: &[(&str, &str)] = &[
    (SNIPPET_VIEW_URI, SNIPPET_VIEW_HTML),
    (COMPILE_VIEW_URI, COMPILE_VIEW_HTML),
];
/// The MCP Apps extension id and the MIME type its Views are served as.
const UI_EXTENSION: &str = "io.modelcontextprotocol/ui";
const UI_MIME: &str = "text/html;profile=mcp-app";

/// `_meta.ui` for a tool: the View it renders in, and who may call it
/// (`["app"]` hides a tool from the model).
fn ui_meta(resource_uri: Option<&str>, visibility: &[&str]) -> rmcp::model::MetaObject {
    let mut ui = serde_json::Map::new();
    if let Some(uri) = resource_uri {
        ui.insert("resourceUri".into(), uri.into());
    }
    ui.insert("visibility".into(), visibility.into());
    let mut meta = serde_json::Map::new();
    meta.insert("ui".into(), ui.into());
    meta.into()
}

/// The `resources/read` answer for a View's URI: its html as an MCP App
/// page. Any other URI is not found.
fn view_resource(uri: &str) -> Result<rmcp::model::ResourceContents, rmcp::ErrorData> {
    let Some(&(uri, html)) = VIEWS.iter().find(|(known, _)| *known == uri) else {
        return Err(rmcp::ErrorData::resource_not_found(
            format!("no resource {uri}"),
            None,
        ));
    };
    let mut contents = rmcp::model::ResourceContents::text(html, uri);
    if let rmcp::model::ResourceContents::TextResourceContents {
        mime_type, meta, ..
    } = &mut contents
    {
        *mime_type = Some(UI_MIME.to_string());
        // No csp: the host's most restrictive default applies.
        *meta = serde_json::json!({ "ui": { "prefersBorder": true } })
            .as_object()
            .cloned()
            .map(Into::into);
    }
    Ok(contents)
}

/// The MCP adapter: one `Core` for the life of the stdio session.
#[derive(Clone, Default)]
struct Maleficium {
    cx: core::Core,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
enum SessionAction {
    Grant,
    Info,
}

impl<'de> Deserialize<'de> for SessionAction {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        match s.as_str() {
            "grant" => Ok(Self::Grant),
            "info" => Ok(Self::Info),
            other => Err(serde::de::Error::custom(unknown_action(
                other,
                &["grant", "info"],
            ))),
        }
    }
}

/// Unknown fields are refused: a session call names a root and an action,
/// nothing that could approve, fetch or write.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct SessionParams {
    root_id: String,
    action: SessionAction,
    /// Required iff action is grant: the absolute folder to grant.
    root: Option<String>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct PathOut {
    path: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
enum FileAction {
    Read,
    List,
}

impl<'de> Deserialize<'de> for FileAction {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        match s.as_str() {
            "read" => Ok(Self::Read),
            "list" => Ok(Self::List),
            other => Err(serde::de::Error::custom(unknown_action(
                other,
                &["read", "list"],
            ))),
        }
    }
}

/// Unknown fields are refused: a file call names a project, an action and
/// a path, nothing that could approve, fetch or write.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct FileParams {
    root_id: String,
    action: FileAction,
    /// Required iff action is read; the level to list iff action is list
    /// (defaults to the root).
    rel: Option<String>,
}

/// The `file` result for one action, verbatim: `read` the file's text,
/// `list` the level's entries.
#[derive(Debug, Serialize)]
#[serde(untagged)]
enum FileOut {
    Text(TextOut),
    Listing(ListOut),
}

// An untagged union has no object schema of its own; name the object shape
// explicitly so `opencode mcp list` keeps accepting the server.
impl schemars::JsonSchema for FileOut {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "FileOut".into()
    }
    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "object",
            "description": "Per-action result, verbatim: `read` returns the file's text, `list` returns the level's entries.",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
enum FileMutateAction {
    Delete,
    Undo,
}

impl<'de> Deserialize<'de> for FileMutateAction {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        match s.as_str() {
            "delete" => Ok(Self::Delete),
            "undo" => Ok(Self::Undo),
            other => Err(serde::de::Error::custom(unknown_action(
                other,
                &["delete", "undo"],
            ))),
        }
    }
}

/// Unknown fields are refused: a mutation names a project, an action and
/// the path that action needs, nothing that could approve or fetch.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct FileMutateParams {
    root_id: String,
    action: FileMutateAction,
    /// Required iff action is delete.
    rel: Option<String>,
    /// Iff action is delete: the absolute path from the refused first call.
    confirm: Option<String>,
    /// Required iff action is undo: the trash path from a `delete`.
    trash_path: Option<String>,
}

/// The `file_mutate` result for one action, verbatim: `delete` the trash
/// path, `undo` the restored path.
#[derive(Debug, Serialize)]
#[serde(untagged)]
enum FileMutateOut {
    Deleted(DeleteOut),
    Restored(PathOut),
}

// Same object-shape naming as `FileOut`: the union serializes as the
// bare per-action record.
impl schemars::JsonSchema for FileMutateOut {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "FileMutateOut".into()
    }
    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "object",
            "description": "Per-action result, verbatim: `delete` returns the trash path, `undo` returns the restored path.",
        })
    }
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct EntryOut {
    name: String,
    entry_type: String,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct ListOut {
    entries: Vec<EntryOut>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
enum StructureAction {
    Outline,
    FileGraph,
    LabelsRefs,
    Citations,
    DocumentStructure,
    Definition,
}

impl<'de> Deserialize<'de> for StructureAction {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        match s.as_str() {
            "outline" => Ok(Self::Outline),
            "file_graph" => Ok(Self::FileGraph),
            "labels_refs" => Ok(Self::LabelsRefs),
            "citations" => Ok(Self::Citations),
            "document_structure" => Ok(Self::DocumentStructure),
            "definition" => Ok(Self::Definition),
            other => Err(serde::de::Error::custom(unknown_action(
                other,
                &[
                    "outline",
                    "file_graph",
                    "labels_refs",
                    "citations",
                    "document_structure",
                    "definition",
                ],
            ))),
        }
    }
}

/// Unknown fields are refused: a structure call names a project, an action
/// and the file or key that action needs, nothing that could approve,
/// fetch or write.
#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct StructureParams {
    root_id: String,
    action: StructureAction,
    /// Required iff action is outline, or for the at-position spelling of
    /// definition (with `line` and `col`).
    rel: Option<String>,
    /// Required iff action is file_graph, labels_refs, citations or
    /// document_structure; optional for definition (inputs resolve from its
    /// directory, else from the root).
    main_rel: Option<String>,
    /// Iff action is definition, with `key`: the reference kind (label,
    /// citation, macro or input).
    kind: Option<maleficium_index::definition::RefKind>,
    /// Iff action is definition, with `kind`.
    key: Option<String>,
    /// Iff action is definition, with `rel` and `col`: the 1-based line.
    line: Option<u64>,
    /// Iff action is definition, with `rel` and `line`: the UTF-16 column.
    col: Option<u64>,
}

/// The `structure` result for one action, verbatim: each action returns
/// what its old tool returned.
#[derive(Debug, Serialize)]
#[serde(untagged)]
enum StructureOut {
    Outline(core::structure::OutlineDoc),
    FileGraph(core::structure::FileGraph),
    LabelsRefs(core::structure::LabelsRefs),
    Citations(core::structure::Citations),
    Document(core::document::DocumentStructure),
    Definition(DefinitionOut),
}

// An untagged union has no object schema of its own; name the object shape
// explicitly so `opencode mcp list` keeps accepting the server.
impl schemars::JsonSchema for StructureOut {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "StructureOut".into()
    }
    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "object",
            "description": "Per-action result, verbatim: `outline` the file's sections, `file_graph` its inputs, `labels_refs` its labels and uses, `citations` its cites, `document_structure` the conversion, `definition` the lookup.",
        })
    }
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct TextOut {
    text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
enum PaperAction {
    Start,
    Poll,
    Cancel,
    Diagnostics,
    Prechecks,
    Readiness,
    Output,
    Log,
}

impl<'de> Deserialize<'de> for PaperAction {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        match s.as_str() {
            "start" => Ok(Self::Start),
            "poll" => Ok(Self::Poll),
            "cancel" => Ok(Self::Cancel),
            "diagnostics" => Ok(Self::Diagnostics),
            "prechecks" => Ok(Self::Prechecks),
            "readiness" => Ok(Self::Readiness),
            "output" => Ok(Self::Output),
            "log" => Ok(Self::Log),
            other => Err(serde::de::Error::custom(unknown_action(
                other,
                &[
                    "start",
                    "poll",
                    "cancel",
                    "diagnostics",
                    "prechecks",
                    "readiness",
                    "output",
                    "log",
                ],
            ))),
        }
    }
}

/// Unknown fields are refused: a paper call names an action and the fields
/// that action needs. `start` compiles (optionally fetching first), `poll`
/// and `cancel` name a job, the readings name a project and main file, and
/// `readiness` names only a project. `log` tails the engine log shard for
/// the main file's dir.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct PaperParams {
    action: PaperAction,
    /// Required iff action is start, diagnostics, prechecks, readiness or
    /// output.
    root_id: Option<String>,
    /// Required iff action is poll or cancel: the job id from `start`.
    job_id: Option<String>,
    /// Iff action is start: the main file; absent resolves the same main
    /// file the app would. Required iff action is diagnostics, prechecks,
    /// output or log.
    main_rel: Option<String>,
    /// Iff action is start: fetch everything online first, then prove it
    /// compiles from the cache alone (the app's Make Available Offline).
    networked: Option<bool>,
    /// Iff action is poll: how many lines back to report (default 50).
    tail_lines: Option<usize>,
    /// Iff action is poll: wait up to this long (ms, at most 60000) for a
    /// running job to finish before answering.
    wait_ms: Option<u64>,
    /// Iff action is diagnostics or log: cap rows (default 100 for
    /// diagnostics, 50 for log).
    max: Option<usize>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct CompileRunOut {
    job_id: String,
    /// The main file the job compiles (root-relative).
    main_rel: String,
}

/// The longest a poll waits for a running job, and how often it looks.
const MAX_POLL_WAIT_MS: u64 = 60_000;
const POLL_STEP: std::time::Duration = std::time::Duration::from_millis(200);

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct CompilePollOut {
    status: String,
    pdf_url: Option<String>,
    log: String,
    lines: Vec<String>,
    /// The dependency a finished run lacked, and why.
    missing: Option<maleficium_structure::MissingDependency>,
    /// The phase the run is in (or last reached) and the files it downloaded.
    progress: core::Progress,
}

/// The `paper` result for one action, verbatim: `start` the job id and main
/// file, `poll` the job's status and lines so far, `cancel` what stopped,
/// `diagnostics` the last compile's errors and warnings, `prechecks` the
/// blockers before compiling, `readiness` the offline state, `output` the
/// compiled pdf's stamp (or null), `log` the engine log tail.
#[derive(Debug, Serialize)]
#[serde(untagged)]
enum PaperOut {
    Started(CompileRunOut),
    Polled(CompilePollOut),
    Cancelled(CancelOut),
    Diagnostics(core::structure::Diagnostics),
    Prechecks(core::structure::Precheck),
    Readiness(maleficium_events::OfflineReadiness),
    Stamped(OutputStampOut),
    Logged(TextOut),
}

// An untagged union has no object schema of its own; name the object shape
// explicitly so `opencode mcp list` keeps accepting the server.
impl schemars::JsonSchema for PaperOut {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "PaperOut".into()
    }
    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "object",
            "description": "Per-action result, verbatim: `start` returns the job id and main file, `poll` the job's status and lines, `cancel` what stopped, `diagnostics` the last compile's errors, `prechecks` the blockers, `readiness` the offline state, `output` the pdf stamp or null, `log` the engine log tail.",
        })
    }
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct OutputStampOut {
    stamp: Option<core::OutputStamp>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
enum ExportReadAction {
    Stamp,
    Status,
    Cancel,
}

impl<'de> Deserialize<'de> for ExportReadAction {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        match s.as_str() {
            "stamp" => Ok(Self::Stamp),
            "status" => Ok(Self::Status),
            "cancel" => Ok(Self::Cancel),
            other => Err(serde::de::Error::custom(unknown_action(
                other,
                &["stamp", "status", "cancel"],
            ))),
        }
    }
}

/// Unknown fields are refused: the read takes a project, a main file and an
/// action, nothing that could approve, fetch or write.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct ExportReadParams {
    root_id: String,
    action: ExportReadAction,
    /// Required iff action is stamp or status.
    main_rel: Option<String>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct ExportStatusOut {
    /// Whether a previous compile left a pdf for the main file.
    fresh: bool,
}

/// The `export_read` result for one action, verbatim: `stamp` the pdf's
/// stamp (or null), `status` whether a pdf waits, `cancel` what stopped.
#[derive(Debug, Serialize)]
#[serde(untagged)]
enum ExportReadOut {
    Stamp(OutputStampOut),
    Status(ExportStatusOut),
    Cancelled(CancelOut),
}

// An untagged union has no object schema of its own; name the object shape
// explicitly so `opencode mcp list` keeps accepting the server.
impl schemars::JsonSchema for ExportReadOut {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "ExportReadOut".into()
    }
    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "object",
            "description": "Per-action result, verbatim: `stamp` returns the pdf's stamp or null, `status` whether a pdf waits, `cancel` what stopped.",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
enum ScaffoldAction {
    List,
    Create,
}

impl<'de> Deserialize<'de> for ScaffoldAction {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        match s.as_str() {
            "list" => Ok(Self::List),
            "create" => Ok(Self::Create),
            other => Err(serde::de::Error::custom(unknown_action(
                other,
                &["list", "create"],
            ))),
        }
    }
}

/// Unknown fields are refused: a scaffold call names an action and the
/// template it needs, nothing that could approve, fetch or write elsewhere.
/// No session root: templates are global, and `create` takes an absolute
/// parent folder.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct ScaffoldParams {
    action: ScaffoldAction,
    /// A template id from `list` (or "welcome"). Required iff action is create.
    template: Option<String>,
    /// Absolute folder the new project goes in. Required iff action is create.
    parent_dir: Option<String>,
    /// The new project's folder name. Required iff action is create.
    name: Option<String>,
}

/// The `scaffold` result for one action, verbatim: `list` the templates,
/// `create` the new project.
#[derive(Debug, Serialize)]
#[serde(untagged)]
enum ScaffoldOut {
    List(core::templates::TemplateList),
    Created(core::templates::Created),
}

// An untagged union has no object schema of its own; name the object shape
// explicitly so `opencode mcp list` keeps accepting the server.
impl schemars::JsonSchema for ScaffoldOut {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "ScaffoldOut".into()
    }
    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "object",
            "description": "Per-action result, verbatim: `list` returns the templates, `create` returns the new project.",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
enum WidgetAction {
    List,
    Status,
    Check,
}

impl<'de> Deserialize<'de> for WidgetAction {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        match s.as_str() {
            "list" => Ok(Self::List),
            "status" => Ok(Self::Status),
            "check" => Ok(Self::Check),
            other => Err(serde::de::Error::custom(unknown_action(
                other,
                &["list", "status", "check"],
            ))),
        }
    }
}

/// Unknown fields are refused: there is no approval argument for an agent
/// to pass, so an `approve` (or anything like it) is an error. A widget
/// call names a project, an action and the main file that action reads;
/// only `check` names one widget, as `list` lists it.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct WidgetParams {
    root_id: String,
    action: WidgetAction,
    main_rel: String,
    /// Required iff action is check: the widget id, as `list` lists it.
    widget: Option<String>,
}

/// The `widget` result for one action, verbatim: `list` the widgets in
/// document order, `status` the approval state of each, `check` whether one
/// widget may run (or approval_required as a normal result).
#[derive(Debug, Serialize)]
#[serde(untagged)]
enum WidgetOut {
    Listed(core::widgets::WidgetList),
    Status(Box<core::widget_approval::WidgetsStatus>),
    Checked(Box<WidgetCheckOut>),
}

// An untagged union has no object schema of its own; name the object shape
// explicitly so `opencode mcp list` keeps accepting the server.
impl schemars::JsonSchema for WidgetOut {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "WidgetOut".into()
    }
    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "object",
            "description": "Per-action result, verbatim: `list` returns the widgets, `status` their approval state, `check` whether one widget may run.",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
enum RuntimeReadAction {
    Scaffold,
    Validate,
}

impl<'de> Deserialize<'de> for RuntimeReadAction {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        match s.as_str() {
            "scaffold" => Ok(Self::Scaffold),
            "validate" => Ok(Self::Validate),
            other => Err(serde::de::Error::custom(unknown_action(
                other,
                &["scaffold", "validate"],
            ))),
        }
    }
}

/// Unknown fields are refused: a runtime read names an action and the draft
/// or package it needs, nothing that could approve, install or allow.
/// `scaffold` takes a name and an optional `--from`, never a session root:
/// drafts are global. `validate` takes the package ref, and with it the
/// project's installed copy, without it the library's.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct RuntimeReadParams {
    action: RuntimeReadAction,
    /// Required iff action is scaffold: the new draft's name (`<name>` of
    /// `<name>@1`).
    name: Option<String>,
    /// Iff action is scaffold: fork a built-in: only `model@1` (its viewer
    /// as an editable `viewer.js` over vendored three.js).
    from: Option<String>,
    /// Required iff action is validate: the package ref (`<name>@<major>`).
    reference: Option<String>,
    /// Iff action is validate: with it, the project's installed copy;
    /// without it, the user library's.
    root_id: Option<String>,
}

/// The `runtime_read` result for one action, verbatim: `scaffold` the new
/// draft's reference and path, `validate` the package's errors and warnings.
#[derive(Debug, Serialize)]
#[serde(untagged)]
enum RuntimeReadOut {
    Scaffolded(RuntimeScaffoldOut),
    Validated(RuntimeValidateOut),
}

// An untagged union has no object schema of its own; name the object shape
// explicitly so `opencode mcp list` keeps accepting the server.
impl schemars::JsonSchema for RuntimeReadOut {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "RuntimeReadOut".into()
    }
    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "object",
            "description": "Per-action result, verbatim: `scaffold` returns the draft's reference and path, `validate` returns its errors and warnings.",
        })
    }
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct RuntimeScaffoldOut {
    reference: String,
    /// Absolute path of the draft folder in the user library.
    path: String,
}

// Flatten keeps the wire shape (`status` at top level) as an object schema.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct WidgetCheckOut {
    #[serde(flatten)]
    pub status: core::widget_approval::WidgetApprovalStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
enum PresenceAction {
    Windows,
    RequestOpen,
}

impl<'de> Deserialize<'de> for PresenceAction {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        match s.as_str() {
            "windows" => Ok(Self::Windows),
            "request_open" => Ok(Self::RequestOpen),
            other => Err(serde::de::Error::custom(unknown_action(
                other,
                &["windows", "request_open"],
            ))),
        }
    }
}

/// Unknown fields are refused: a presence call names an action and the
/// prompt it needs, nothing that could approve, fetch or write. No session
/// root: windows are the user's, not the session's.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct PresenceParams {
    action: PresenceAction,
    /// The project folder, absolute. Required iff action is request_open.
    project: Option<String>,
    /// Iff action is request_open: a file in it to show.
    file: Option<String>,
    /// Iff action is request_open: the window to ask (from windows);
    /// needed when several are open.
    pid: Option<u64>,
    /// Iff action is request_open: how long to wait for the user's answer,
    /// in ms (default 30000, at most 120000). 0 returns at once.
    wait_ms: Option<u64>,
}

/// The `presence` result for one action, verbatim: `windows` the open
/// windows and what to do, `request_open` the ask and its outcome.
#[derive(Debug, Serialize)]
#[serde(untagged)]
enum PresenceOut {
    Windows(AppWindowsOut),
    Asked(AppRequestOpenOut),
}

// An untagged union has no object schema of its own; name the object shape
// explicitly so `opencode mcp list` keeps accepting the server.
impl schemars::JsonSchema for PresenceOut {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "PresenceOut".into()
    }
    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "object",
            "description": "Per-action result, verbatim: `windows` returns the open windows and what to do, `request_open` returns the ask and its outcome.",
        })
    }
}

/// Unknown fields are refused: an introspection names a tool, nothing more.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct DescribeParams {
    /// A tool name, e.g. `structure` or `sync_view`.
    cluster: String,
    /// One of the cluster's actions, e.g. `outline`.
    action: Option<String>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct DescribeOut {
    cluster: String,
    action: Option<String>,
    description: String,
    input_schema: serde_json::Value,
    hints: Vec<String>,
    examples: Vec<serde_json::Value>,
    /// How calls thread state: the tokens, confirms and batches to keep.
    token_flow: String,
}

/// The valid actions of every grouped tool, in wire spelling. `describe`
/// validates its `action` against this list; the per-action tests pin each
/// entry to its tool.
fn cluster_actions(cluster: &str) -> Option<&'static [&'static str]> {
    match cluster {
        "session" => Some(&["grant", "info"]),
        "file" => Some(&["read", "list"]),
        "file_mutate" => Some(&["delete", "undo"]),
        "structure" => Some(&[
            "outline",
            "file_graph",
            "labels_refs",
            "citations",
            "document_structure",
            "definition",
        ]),
        "sync_view" => Some(&["forward", "inverse", "snippet"]),
        "sync_render" => Some(&["render"]),
        "scaffold" => Some(&["list", "create"]),
        "presence" => Some(&["windows", "request_open"]),
        "search" => Some(&["search", "find_files"]),
        "replace" => Some(&["preview", "apply", "undo"]),
        "export_read" => Some(&["stamp", "status", "cancel"]),
        "export_write" => Some(&["pdf", "zip", "bundle", "preview", "open"]),
        "widget" => Some(&["list", "status", "check"]),
        "runtime_read" => Some(&["scaffold", "validate"]),
        "paper" => Some(&[
            "start",
            "poll",
            "cancel",
            "diagnostics",
            "prechecks",
            "readiness",
            "output",
            "log",
        ]),
        _ => None,
    }
}

/// One action's calling rules: the fields it needs and a minimal call.
fn action_notes(cluster: &str, action: &str) -> Option<(&'static str, serde_json::Value)> {
    use serde_json::json;
    let note = match (cluster, action) {
        ("session", "grant") => (
            "grant needs `root`: the absolute project folder. Reuse the root_id for every later call.",
            json!({"root_id": "agent", "action": "grant", "root": "/abs/proj"}),
        ),
        ("session", "info") => (
            "info takes no `root`: it shows the folder already granted as this root_id.",
            json!({"root_id": "agent", "action": "info"}),
        ),
        ("file", "read") => (
            "read needs `rel`: the project-relative file, returned as UTF-8 text.",
            json!({"root_id": "agent", "action": "read", "rel": "main.tex"}),
        ),
        ("file", "list") => (
            "list takes an optional `rel` (defaults to the root) and lists one level, depth 1.",
            json!({"root_id": "agent", "action": "list", "rel": "chapters"}),
        ),
        ("file_mutate", "delete") => (
            "delete needs `rel`. Without `confirm` it is refused with the file's absolute path; pass that back as `confirm` to delete.",
            json!({"root_id": "agent", "action": "delete", "rel": "draft-old.tex"}),
        ),
        ("file_mutate", "undo") => (
            "undo takes only `trash_path`: the trash path from a `delete`.",
            json!({"root_id": "agent", "action": "undo", "trash_path": "/tmp/trash/draft-old.tex"}),
        ),
        ("structure", "outline") => (
            "outline takes only `rel`: sections of one file with labels, figures and tables.",
            json!({"root_id": "agent", "action": "outline", "rel": "main.tex"}),
        ),
        ("structure", "file_graph") => (
            "file_graph takes only `main_rel`: every file reachable over inputs, missing ones flagged.",
            json!({"root_id": "agent", "action": "file_graph", "main_rel": "main.tex"}),
        ),
        ("structure", "labels_refs") => (
            "labels_refs takes only `main_rel`: each label with file and line, each ref-use with whether its key is defined.",
            json!({"root_id": "agent", "action": "labels_refs", "main_rel": "main.tex"}),
        ),
        ("structure", "citations") => (
            "citations takes only `main_rel`: each cite key with file, line, and whether a bib entry defines it.",
            json!({"root_id": "agent", "action": "citations", "main_rel": "main.tex"}),
        ),
        ("structure", "document_structure") => (
            "document_structure takes only `main_rel`: the conversion's sections, theorems, equations, figures and widget placeholders.",
            json!({"root_id": "agent", "action": "document_structure", "main_rel": "main.tex"}),
        ),
        ("structure", "definition") => (
            "definition takes kind with key, or rel with line and col (never both); main_rel is optional. Keys use the source spelling: macros carry their backslash.",
            json!({"root_id": "agent", "action": "definition", "kind": "label", "key": "sec:hi"}),
        ),
        ("sync_view", "forward") => (
            "forward needs `main_rel`, `tex_rel` and `line`: the PDF page showing that source line.",
            json!({"root_id": "agent", "action": "forward", "main_rel": "main.tex", "tex_rel": "chapters/method.tex", "line": 10}),
        ),
        ("sync_view", "inverse") => (
            "inverse needs `main_rel` and `page`; `x` and `y` default to 0: the source file and line at that PDF position.",
            json!({"root_id": "agent", "action": "inverse", "main_rel": "main.tex", "page": 1, "x": 100.0, "y": 600.0}),
        ),
        ("sync_view", "snippet") => (
            "snippet needs `main_rel` and exactly one target: tex_rel with line, label, or page. `with_image` adds the region as a PNG; images cost context.",
            json!({"root_id": "agent", "action": "snippet", "main_rel": "main.tex", "label": "tab:results", "with_image": true}),
        ),
        ("sync_render", "render") => (
            "render needs `main_rel` and `page`; `region` crops to a band, `scale` defaults to 2. App-only refresh for the View.",
            json!({"root_id": "agent", "action": "render", "main_rel": "main.tex", "page": 1}),
        ),
        ("scaffold", "list") => (
            "list takes nothing: the bundled set plus the user's own, each with its main file.",
            json!({"action": "list"}),
        ),
        ("scaffold", "create") => (
            "create needs `template`, `parent_dir` (absolute) and `name`. Refused when the folder exists and is not empty.",
            json!({"action": "create", "template": "article", "parent_dir": "/abs/work", "name": "paper"}),
        ),
        ("presence", "windows") => (
            "windows takes nothing: every open window with its project, files and root_id, most recently active first.",
            json!({"action": "windows"}),
        ),
        ("presence", "request_open") => (
            "request_open needs `project` (absolute); `file`, `pid` and `wait_ms` are optional. Nothing opens until the user clicks Open.",
            json!({"action": "request_open", "project": "/abs/proj", "wait_ms": 0}),
        ),
        ("search", "search") => (
            "search needs `pattern` (regex when `regex` is true); hits reachable from `main_rel` rank first. Cap with `max`.",
            json!({"root_id": "agent", "action": "search", "pattern": "figure", "main_rel": "main.tex"}),
        ),
        ("search", "find_files") => (
            "find_files takes only `query` and `max`: fuzzy file-name matches.",
            json!({"root_id": "agent", "action": "find_files", "query": "meth"}),
        ),
        ("replace", "preview") => (
            "preview needs `pattern` and `replacement` (same flags as search) and writes nothing; it returns per-file revisions plus a token.",
            json!({"root_id": "agent", "action": "preview", "pattern": "sec:method", "replacement": "sec:approach"}),
        ),
        ("replace", "apply") => (
            "apply takes only `token`: writes the previewed plan whole, or refuses it whole when a file changed. A token applies once.",
            json!({"root_id": "agent", "action": "apply", "token": "preview-token"}),
        ),
        ("replace", "undo") => (
            "undo takes only `batch`: the batch id from `apply`. Restores exact bytes.",
            json!({"root_id": "agent", "action": "undo", "batch": "batch-id"}),
        ),
        ("export_read", "stamp") => (
            "stamp needs `main_rel`: the compiled pdf's stamp, or null before any compile.",
            json!({"root_id": "agent", "action": "stamp", "main_rel": "main.tex"}),
        ),
        ("export_read", "status") => (
            "status needs `main_rel`: whether a previous compile left a pdf for it.",
            json!({"root_id": "agent", "action": "status", "main_rel": "main.tex"}),
        ),
        ("export_read", "cancel") => (
            "cancel takes nothing: it stops the running bundle export's conversion.",
            json!({"root_id": "agent", "action": "cancel"}),
        ),
        ("export_write", "pdf") => (
            "pdf needs `main_rel` and `dest` (absolute, outside the project): copies the compiled pdf.",
            json!({"root_id": "agent", "action": "pdf", "main_rel": "main.tex", "dest": "/abs/paper.pdf"}),
        ),
        ("export_write", "zip") => (
            "zip takes only `dest` (absolute, outside the project): packs the sources.",
            json!({"root_id": "agent", "action": "zip", "dest": "/abs/proj.zip"}),
        ),
        ("export_write", "bundle") => (
            "bundle needs `main_rel`, `dest` and `profile` (folder, single-file or hosted).",
            json!({"root_id": "agent", "action": "bundle", "main_rel": "main.tex", "dest": "/abs/bundle", "profile": "single-file"}),
        ),
        ("export_write", "preview") => (
            "preview takes only `main_rel`: writes the single-file bundle to a scratch folder and returns its path, opening nothing.",
            json!({"root_id": "agent", "action": "preview", "main_rel": "main.tex"}),
        ),
        ("export_write", "open") => (
            "open takes only `main_rel`: previews, then opens the path in the OS browser.",
            json!({"root_id": "agent", "action": "open", "main_rel": "main.tex"}),
        ),
        ("widget", "list") => (
            "list takes only `main_rel`: every widget of the last compile in document order, empty when the document declares none.",
            json!({"root_id": "agent", "action": "list", "main_rel": "main.tex"}),
        ),
        ("widget", "status") => (
            "status takes only `main_rel`: the approval state of each html widget and runtime, so you can ask the user about all of them at once.",
            json!({"root_id": "agent", "action": "status", "main_rel": "main.tex"}),
        ),
        ("widget", "check") => (
            "check needs `main_rel` and `widget` (the id, as list shows it): whether it may run, or approval_required as a normal result. Do not poll it: it only changes when the user acts.",
            json!({"root_id": "agent", "action": "check", "main_rel": "main.tex", "widget": "fig-demo"}),
        ),
        ("runtime_read", "scaffold") => (
            "scaffold needs `name` (the draft's name); `from` is optional, only model@1: it writes only to the user library, never to a project.",
            json!({"action": "scaffold", "name": "orbit-view"}),
        ),
        ("runtime_read", "validate") => (
            "validate needs `reference` (the package ref); with `root_id` the project's installed copy, without it the user library's. Read-only: it never approves or installs.",
            json!({"action": "validate", "reference": "orbit-view@1"}),
        ),
        ("paper", "start") => (
            "start needs `root_id`; `main_rel` is optional (without it the job compiles the same main file the app would). Returns the job id; poll it.",
            json!({"root_id": "agent", "action": "start", "main_rel": "main.tex"}),
        ),
        ("paper", "poll") => (
            "poll needs only `job_id`; `tail_lines` and `wait_ms` (up to 60000) are optional. A running job reports lines so far; poll again or wait.",
            json!({"action": "poll", "job_id": "job-1"}),
        ),
        ("paper", "cancel") => (
            "cancel takes only `job_id`: it stops the running job. A settled job reports nothing to cancel.",
            json!({"action": "cancel", "job_id": "job-1"}),
        ),
        ("paper", "diagnostics") => (
            "diagnostics needs `main_rel`: the last compile's errors and warnings, capped by `max` (default 100).",
            json!({"root_id": "agent", "action": "diagnostics", "main_rel": "main.tex"}),
        ),
        ("paper", "prechecks") => (
            "prechecks needs only `main_rel`: the blockers before compiling (missing packages, biber, shell-escape, fonts).",
            json!({"root_id": "agent", "action": "prechecks", "main_rel": "main.tex"}),
        ),
        ("paper", "readiness") => (
            "readiness takes only `root_id`: whether the project compiles offline (ready, needs-network, needs-tool, needs-font, blocked or unverified).",
            json!({"root_id": "agent", "action": "readiness"}),
        ),
        ("paper", "output") => (
            "output needs only `main_rel`: the compiled pdf's stamp, or null before any compile; it changes whenever anyone recompiles.",
            json!({"root_id": "agent", "action": "output", "main_rel": "main.tex"}),
        ),
        ("paper", "log") => (
            "log needs only `main_rel`: the engine log tail for its dir, capped by `max` (default 50).",
            json!({"root_id": "agent", "action": "log", "main_rel": "main.tex"}),
        ),
        _ => return None,
    };
    Some(note)
}

/// One cluster's hints, examples and token-flow notes. Every grouped tool
/// has an entry; single-purpose tools share the generic fallback.
fn cluster_notes(cluster: &str) -> (Vec<String>, Vec<serde_json::Value>, String) {
    use serde_json::json;
    let (hints, examples, token_flow) = match cluster {
        "session" => (
            vec![
                "Call it first: every other tool needs the root_id it registers.",
                "Grant the folder presence shows as open so the user sees your edits.",
            ],
            vec![json!({"root_id": "agent", "action": "grant", "root": "/abs/proj"})],
            "No tokens or batches: grant registers the root_id, info echoes its path. Re-granting a root_id moves it.".to_string(),
        ),
        "file" => (
            vec![
                "Read one file or list one level; escapes from the root are refused.",
                "list defaults rel to the root and stays one level deep.",
            ],
            vec![json!({"root_id": "agent", "action": "read", "rel": "main.tex"})],
            "Stateless: every call stands alone.".to_string(),
        ),
        "file_mutate" => (
            vec![
                "The only way to remove project files; replaces never go through here.",
                "A refused first delete names the absolute path: pass it back as confirm.",
            ],
            vec![json!({"root_id": "agent", "action": "delete", "rel": "draft-old.tex"})],
            "Two calls to delete: the first refusal names the path to pass back as confirm. undo takes the trash_path from a delete. A replace batch is separate: file_mutate cannot undo a replace.".to_string(),
        ),
        "structure" => (
            vec![
                "The read-only way to learn what a document holds; needs no compile.",
                "Reads saved files on disk, never the editor buffers; undefined keys return empty, not errors.",
            ],
            vec![json!({"root_id": "agent", "action": "outline", "rel": "main.tex"})],
            "Stateless readings. definition has two spellings, not a flow: kind with key, or rel with line and col.".to_string(),
        ),
        "sync_view" => (
            vec![
                "Compile first (paper start, then poll): forward and inverse need the synctex file, snippet needs the pdf.",
                "Prefer label targets for figures and tables; use with_image only for layout questions.",
            ],
            vec![json!({"root_id": "agent", "action": "snippet", "main_rel": "main.tex", "label": "tab:results"})],
            "Compile first, then read: forward and inverse need the synctex file, snippet needs the pdf. stale is true when sources changed after the compile; recompile and read again.".to_string(),
        ),
        "sync_render" => (
            vec![
                "The snippet View's own refresh; the model has no other use for it.",
                "Scale clamps to 0.5..3; region crops to a full-width band in pdf points.",
            ],
            vec![json!({"root_id": "agent", "action": "render", "main_rel": "main.tex", "page": 1})],
            "No state: renders the named page at call time. The model reads page images through sync_view snippet with_image instead.".to_string(),
        ),
        "scaffold" => (
            vec![
                "The way to create a project; create refuses a folder that is in use.",
                "Grant the returned root (session with action grant) to work in it.",
            ],
            vec![json!({"action": "create", "template": "article", "parent_dir": "/abs/work", "name": "paper"})],
            "create returns root and main; session-grant the root to work in it. list carries no state.".to_string(),
        ),
        "presence" => (
            vec![
                "Call windows at the start of a session to meet the user where they are.",
                "Never resend a dismissed ask; a waiting ask stays up until the user answers.",
            ],
            vec![json!({"action": "windows"})],
            "windows tells you which project is open; session-grant its folder. request_open outcomes: opened (work there), dismissed (do not resend), waiting (tell the user the prompt is up; ask windows again later).".to_string(),
        ),
        "search" => (
            vec![
                "Hunts saved files; the file listing and caches are out of scope.",
                "Hits reachable from main_rel rank first.",
            ],
            vec![json!({"root_id": "agent", "action": "search", "pattern": "figure"})],
            "Stateless: pattern with regex, case_sensitive and whole_word, or find_files with query. Cap runs with max.".to_string(),
        ),
        "replace" => (
            vec![
                "Always preview before apply; keep the batch to undo.",
                "A spent token or a plan over changed files fails whole and writes nothing.",
            ],
            vec![json!({"root_id": "agent", "action": "preview", "pattern": "sec:method", "replacement": "sec:approach"})],
            "Three calls: preview returns a token, apply spends it whole (refused when any file changed since), undo takes the batch.".to_string(),
        ),
        "export_read" => (
            vec![
                "Reading state never writes; cancelling with no conversion running is an error.",
                "stamp and status need main_rel; cancel takes nothing.",
            ],
            vec![json!({"root_id": "agent", "action": "status", "main_rel": "main.tex"})],
            "Stateless readings.".to_string(),
        ),
        "export_write" => (
            vec![
                "Writes outside the project, never into it; an inside dest is refused.",
                "preview opens nothing; open previews then opens it in the OS browser.",
            ],
            vec![json!({"root_id": "agent", "action": "pdf", "main_rel": "main.tex", "dest": "/abs/paper.pdf"})],
            "Each action writes outside the project. preview writes to a scratch folder and returns its path; open previews then opens it.".to_string(),
        ),
        "widget" => (
            vec![
                "Three views of one reading: compile first (paper start, then poll); the list reads the records, status and check read the approval store.",
                "check returns approval_required as a normal result, never an error: relay its message and userAction, do not poll it.",
                "Review-like reads map here: list and status show what the user reviews; validate a runtime package with runtime_read, and only the user approves in View > Widgets.",
            ],
            vec![json!({"root_id": "agent", "action": "status", "main_rel": "main.tex"})],
            "Stateless readings of the last compile. status covers every widget and runtime at once; check covers one widget by id.".to_string(),
        ),
        "runtime_read" => (
            vec![
                "The non-destructive half of runtime authoring: drafts live in the library, reviewed in the app before anything installs.",
                "scaffold takes only name and from (no session root); validate takes reference, with root_id for a project's installed copy.",
                "Validate before install (runtime_install copies only a draft that validates, and installing never approves: ask the user to allow it in View > Widgets).",
            ],
            vec![json!({"action": "validate", "reference": "orbit-view@1"})],
            "scaffold returns reference and library path; validate by reference, or by reference with root_id for the installed copy. install stays a separate trust-gated tool.".to_string(),
        ),
        "paper" => (
            vec![
                "One cluster for the whole compile loop: start a job, poll or cancel it, then read the last run back without compiling.",
                "start returns job_id and main_rel; poll by job_id (wait_ms up to 60000) until the status leaves running; cancel by job_id.",
                "The readings map the old tools one-to-one: diagnostics reads the last compile's errors, prechecks the blockers, readiness the offline state, output the pdf stamp, log the engine log tail.",
            ],
            vec![json!({"root_id": "agent", "action": "start", "main_rel": "main.tex"})],
            "start returns a job_id to poll or cancel. poll waits up to wait_ms for a running job; the readings (diagnostics, prechecks, readiness, output, log) are stateless.".to_string(),
        ),
        _ => (
            vec!["Single-purpose tool: its description and schema are the whole contract."],
            vec![],
            "No grouped flow.".to_string(),
        ),
    };
    (
        hints.into_iter().map(Into::into).collect(),
        examples,
        token_flow,
    )
}

/// One running Maleficium window and what it holds.
#[derive(Debug, Serialize, schemars::JsonSchema)]
struct AppWindow {
    pid: u32,
    /// The open project folder (absolute); absent when none is open.
    project: Option<String>,
    /// The document's main file, relative to the project.
    main_rel: Option<String>,
    /// The file in the user's editor, relative to the project.
    active_rel: Option<String>,
    /// The root id this server already granted for the project, if any.
    root_id: Option<String>,
    /// The open request this window holds, if any: waiting (no outcome),
    /// opened or dismissed.
    request: Option<serde_json::Value>,
    /// Unix ms of the window's last update (a heartbeat every 30 s).
    updated_ms: u64,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct AppRequestOpenOut {
    request_id: String,
    pid: u32,
    project: String,
    file: Option<String>,
    /// `opened`, `dismissed`, or `waiting` (the user has not answered yet).
    outcome: String,
    /// What to do next.
    hint: String,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct AppWindowsOut {
    /// Running windows, most recently active first.
    windows: Vec<AppWindow>,
    /// What to do with them.
    hint: String,
}

/// Unknown fields are refused: the install copies one validated draft,
/// and nothing it takes could approve, allow or decide.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct RuntimeInstallParams {
    /// The project to install into.
    root_id: String,
    /// The library draft's ref (`<name>@<major>`).
    reference: String,
    /// Swap out a different copy already at `runtimes/<ref>/`.
    replace: Option<bool>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct RuntimeInstallOut {
    reference: String,
    /// The project-relative folder: `runtimes/<ref>`.
    path: String,
    files: usize,
    /// `installed`, `replaced`, or `unchanged` (already exactly these files).
    outcome: String,
    /// Whether this copy may run now: the user allowed this exact content
    /// before, or auto-approval covers it.
    approved: bool,
    /// The user's auto-approval setting for this project.
    auto_approve: bool,
    /// True when the user has to allow it in the app: ask them to.
    ask_user: bool,
    /// What happens next, written for the agent to act on or relay.
    hint: String,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct RuntimeValidateOut {
    reference: String,
    path: String,
    valid: bool,
    errors: Vec<String>,
    warnings: Vec<String>,
    manifest: Option<serde_json::Value>,
}

/// The write half of an export: which file the action writes, and where.
/// `preview` and `open` write the single-file bundle to a scratch folder
/// under the app data dir (never the project); `open` then hands that path
/// to the OS browser, like the app's Preview in Browser.
#[derive(Debug, Clone, Copy, PartialEq, Eq, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
enum ExportWriteAction {
    Pdf,
    Zip,
    Bundle,
    Preview,
    Open,
}

impl<'de> Deserialize<'de> for ExportWriteAction {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        match s.as_str() {
            "pdf" => Ok(Self::Pdf),
            "zip" => Ok(Self::Zip),
            "bundle" => Ok(Self::Bundle),
            "preview" => Ok(Self::Preview),
            "open" => Ok(Self::Open),
            other => Err(serde::de::Error::custom(unknown_action(
                other,
                &["pdf", "zip", "bundle", "preview", "open"],
            ))),
        }
    }
}

/// Unknown fields are refused: the export has no approval argument for an
/// agent to pass, so an `approved_fetch` (or anything like it) is an error,
/// not something quietly ignored.
#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct ExportWriteParams {
    root_id: String,
    action: ExportWriteAction,
    /// Required iff action is pdf, bundle, preview or open.
    main_rel: Option<String>,
    /// Required iff action is pdf, zip or bundle: the absolute path outside
    /// the project to write (a new or empty folder for folder and hosted, the
    /// file to write for pdf, zip and single-file).
    dest: Option<String>,
    /// Required iff action is bundle.
    profile: Option<maleficium_events::BundleProfile>,
    /// Single-file warning threshold in bytes; default 50 MiB (bundle only).
    size_cap_bytes: Option<u64>,
}

/// The `export_write` result for one action, verbatim: `pdf` and `zip` the
/// file written, `bundle` and `open` the bundle written, `preview` the
/// scratch path to open.
#[derive(Debug, Serialize)]
#[serde(untagged)]
enum ExportWriteOut {
    Exported(core::export::Exported),
    Bundle(core::bundle::BundleExported),
    Preview(PreviewBundleOut),
}

// Same object-shape naming as `SearchOut`: the union serializes as the
// bare per-action record.
impl schemars::JsonSchema for ExportWriteOut {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "ExportWriteOut".into()
    }
    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "object",
            "description": "Per-action result, verbatim: `pdf` and `zip` return the file written, `bundle` and `open` the bundle written, `preview` the scratch path.",
        })
    }
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct PreviewBundleOut {
    /// Absolute path of the single-file bundle's index.html, outside the project.
    path: String,
    bytes: u64,
    widgets: u32,
    warnings: Vec<core::bundle::BundleWarning>,
    /// How to view it: nothing was opened.
    hint: String,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct CancelOut {
    status: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
enum SyncViewAction {
    Forward,
    Inverse,
    Snippet,
}

impl<'de> Deserialize<'de> for SyncViewAction {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        match s.as_str() {
            "forward" => Ok(Self::Forward),
            "inverse" => Ok(Self::Inverse),
            "snippet" => Ok(Self::Snippet),
            other => Err(serde::de::Error::custom(unknown_action(
                other,
                &["forward", "inverse", "snippet"],
            ))),
        }
    }
}

/// Unknown fields are refused: a view call names a project, an action and
/// the target that action needs, nothing more.
#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct SyncViewParams {
    root_id: String,
    action: SyncViewAction,
    main_rel: String,
    /// Iff action is forward, or the line target of snippet: with `line`.
    tex_rel: Option<String>,
    /// Iff action is forward, or the line target of snippet: with `tex_rel`.
    line: Option<u64>,
    /// Or the label target of snippet.
    label: Option<String>,
    /// Iff action is inverse, or the page target of snippet.
    page: Option<u64>,
    /// Iff action is inverse: the horizontal PDF point (defaults to 0).
    x: Option<f32>,
    /// Iff action is inverse: the vertical PDF point (defaults to 0).
    y: Option<f32>,
    /// Snippet only: also return the region as a PNG (default false:
    /// images cost context).
    with_image: Option<bool>,
}

impl SyncViewParams {
    fn target(&self) -> Result<core::snippet::Target, String> {
        use core::snippet::Target;
        match (&self.tex_rel, self.line, &self.label, self.page) {
            (Some(tex_rel), Some(line), None, None) => Ok(Target::Line {
                tex_rel: tex_rel.clone(),
                line: narrow_u32(line, "line")?,
            }),
            (None, None, Some(label), None) => Ok(Target::Label {
                label: label.clone(),
            }),
            (None, None, None, Some(page)) => Ok(Target::Page {
                page: narrow_u32(page, "page")?,
            }),
            _ => Err("give exactly one target: tex_rel with line, or label, or page".to_string()),
        }
    }

    /// The per-action extras the schema cannot see die here, before the
    /// project is touched.
    fn reject(&self, names: &[(&str, bool)], action: &str) -> Result<(), String> {
        for (name, set) in names {
            if *set {
                return Err(format!(
                    "{action} takes no `{name}` (pass it under the action that needs it)"
                ));
            }
        }
        Ok(())
    }
}

/// Marker for sync_view's per-action record, verbatim: `forward` the
/// SyncTeX hit, `inverse` the source position, `snippet` the region (plus a
/// PNG when asked).
#[derive(Debug, Serialize, schemars::JsonSchema)]
struct SyncViewOut {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
enum SyncRenderAction {
    Render,
}

impl<'de> Deserialize<'de> for SyncRenderAction {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        match s.as_str() {
            "render" => Ok(Self::Render),
            other => Err(serde::de::Error::custom(unknown_action(other, &["render"]))),
        }
    }
}

/// Unknown fields are refused: a re-render names a project and a page,
/// nothing more.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct SyncRenderParams {
    root_id: String,
    action: SyncRenderAction,
    main_rel: String,
    /// The page to render (1-based).
    page: u64,
    /// Crop to a full-width band around this region (pdf points from the
    /// page's top left); omit for the whole page.
    region: Option<core::snippet::Region>,
    /// Pixels per point before the size caps (default 2, clamped to 0.5..3).
    scale: Option<f32>,
}

/// Marker for sync_render's `render` record: the page re-rendered as a PNG.
#[derive(Debug, Serialize, schemars::JsonSchema)]
struct SyncRenderOut {}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct DeleteOut {
    trash_path: String,
}

/// A u64 param narrowed for the core, which counts lines, columns, pages
/// and pids in u32.
fn narrow_u32(value: u64, name: &str) -> Result<u32, String> {
    u32::try_from(value).map_err(|_| format!("`{name}` is out of range"))
}

/// Edit distance over action names, for did-you-mean suggestions.
fn edit_distance(a: &str, b: &str) -> usize {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, &ca) in a.iter().enumerate() {
        let mut prev = i;
        row[0] = i + 1;
        for (j, &cb) in b.iter().enumerate() {
            let cost = usize::from(ca != cb);
            let next = (row[j + 1] + 1).min((row[j] + 1).min(prev + cost));
            prev = row[j + 1];
            row[j + 1] = next;
        }
    }
    row[b.len()]
}

/// The closest valid action name within distance 2, if any.
fn did_you_mean<'a>(got: &str, valid: &[&'a str]) -> Option<&'a str> {
    valid
        .iter()
        .copied()
        .filter(|v| edit_distance(got, v) <= 2)
        .min_by_key(|v| edit_distance(got, v))
}

fn unknown_action(got: &str, valid: &[&str]) -> String {
    let mut msg = format!(
        "unknown action `{got}`; valid actions: [{}]",
        valid.join(", ")
    );
    if let Some(s) = did_you_mean(got, valid) {
        msg.push_str(&format!("; did you mean `{s}`?"));
    }
    msg
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
enum SearchAction {
    Search,
    FindFiles,
}

impl<'de> Deserialize<'de> for SearchAction {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        match s.as_str() {
            "search" => Ok(Self::Search),
            "find_files" => Ok(Self::FindFiles),
            other => Err(serde::de::Error::custom(unknown_action(
                other,
                &["search", "find_files"],
            ))),
        }
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct SearchParams {
    root_id: String,
    action: SearchAction,
    /// Required iff action == search. Literal text, or regex when `regex`.
    pattern: Option<String>,
    /// Required iff action == find_files. Fuzzy-name query string.
    query: Option<String>,
    regex: Option<bool>,
    case_sensitive: Option<bool>,
    whole_word: Option<bool>,
    /// Files reachable from this main file rank first (search only).
    main_rel: Option<String>,
    /// Hit cap. Defaults: 1000 (search), 50 (find_files).
    max: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
enum ReplaceAction {
    Preview,
    Apply,
    Undo,
}

impl<'de> Deserialize<'de> for ReplaceAction {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        match s.as_str() {
            "preview" => Ok(Self::Preview),
            "apply" => Ok(Self::Apply),
            "undo" => Ok(Self::Undo),
            other => Err(serde::de::Error::custom(unknown_action(
                other,
                &["preview", "apply", "undo"],
            ))),
        }
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct ReplaceParams {
    root_id: String,
    action: ReplaceAction,
    /// Required iff preview. Same pattern and flags as `search`; no cap.
    pattern: Option<String>,
    /// Required iff preview. `$1` / `${name}` expand under regex=true.
    replacement: Option<String>,
    regex: Option<bool>,
    case_sensitive: Option<bool>,
    whole_word: Option<bool>,
    main_rel: Option<String>,
    /// Required iff apply. The token from `preview`.
    token: Option<String>,
    /// Required iff undo. The batch id from `apply`.
    batch: Option<String>,
}

/// The `search` result for one action, verbatim: `search` returns the hit
/// list, `find_files` the file list.
#[derive(Debug, Serialize)]
#[serde(untagged)]
enum SearchOut {
    Hits(maleficium_index::search::SearchResult),
    Files(FindFilesOut),
}

// An untagged union has no object schema of its own; name the object shape
// explicitly so `opencode mcp list` keeps accepting the server.
impl schemars::JsonSchema for SearchOut {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "SearchOut".into()
    }
    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "object",
            "description": "Per-action result, verbatim: `search` returns the hit list, `find_files` returns the file list.",
        })
    }
}

/// The `replace` result for one action, verbatim: `preview` the plan plus
/// token, `apply` the batch plus files written, `undo` the files restored.
#[derive(Debug, Serialize)]
#[serde(untagged)]
enum ReplaceOut {
    Preview(maleficium_index::replace::ReplacePreview),
    Applied(maleficium_index::replace::ReplaceApplied),
    Undone(ReplaceUndoOut),
}

// Same object-shape naming as `SearchOut`: the union serializes as the
// bare per-action record.
impl schemars::JsonSchema for ReplaceOut {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "ReplaceOut".into()
    }
    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "object",
            "description": "Per-action result, verbatim: `preview` returns the plan plus token, `apply` the batch plus files written, `undo` the files restored.",
        })
    }
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct ReplaceUndoOut {
    restored: Vec<maleficium_events::BatchFile>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct DefinitionOut {
    /// `None` when nothing referable sits at the position.
    lookup: Option<maleficium_index::definition::Lookup>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct FindFilesOut {
    files: Vec<maleficium_index::search::FileMatch>,
}

fn path_string(p: std::path::PathBuf) -> String {
    p.to_string_lossy().to_string()
}

/// One event per tool call, actor `agent`, through the shared writer. The
/// call name and its outcome are the payload; a dropped log write never
/// fails the tool.
fn mcp_event<T>(tool: &str, r: &Result<T, String>) -> maleficium_events::BusEvent {
    use maleficium_events::{Actor, AppEvent, BusEvent, EventKind, EventScope};
    let (ok, error) = match r {
        Ok(_) => (true, None),
        Err(e) => (false, Some(e.chars().take(200).collect())),
    };
    BusEvent {
        at: core::eventlog::now_ms(),
        scope: EventScope::App,
        kind: if ok {
            EventKind::Success
        } else {
            EventKind::Error
        },
        actor: Actor::Agent,
        message: if ok {
            format!("mcp {tool} ok")
        } else {
            format!("mcp {tool} failed")
        },
        event: AppEvent::McpCall {
            tool: tool.to_string(),
            ok,
            error,
        },
    }
}

impl Maleficium {
    /// The bundle domain event is best-effort observability: the export
    /// already ran, so a dropped log write is noted here and never fails
    /// the tool.
    fn log_bundle_event(
        main_rel: &str,
        profile: maleficium_events::BundleProfile,
        r: &Result<core::bundle::BundleExported, String>,
    ) {
        if core::eventlog::append(std::slice::from_ref(&core::bundle::event(
            main_rel,
            profile,
            r,
            maleficium_events::Actor::Agent,
        )))
        .is_err()
        {
            // Nothing to fall back to: the log line is lost, the export stands.
        }
    }

    fn tool<T>(&self, name: &str, f: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
        let r = f();
        let _ = core::eventlog::append(std::slice::from_ref(&mcp_event(name, &r)));
        r
    }

    async fn tool_async<T>(
        &self,
        name: &str,
        f: impl std::future::Future<Output = Result<T, String>>,
    ) -> Result<T, String> {
        let r = f.await;
        let _ = core::eventlog::append(std::slice::from_ref(&mcp_event(name, &r)));
        r
    }
}

#[tool_router]
impl Maleficium {
    #[tool(
        description = "Grant or inspect a session project root: call it first so later tools stay inside the user's project. `grant` registers an absolute folder as root_id and returns its path; `info` shows the folder already granted as root_id.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    fn session(&self, Parameters(p): Parameters<SessionParams>) -> Result<Json<PathOut>, String> {
        self.tool("session", || match p.action {
            SessionAction::Grant => {
                let root = p.root.as_deref().ok_or_else(|| {
                    "grant requires `root`: the absolute project folder to grant as this root"
                        .to_string()
                })?;
                let path = path_string(core::grant_root(&self.cx, &p.root_id, root)?);
                Ok(Json(PathOut { path }))
            }
            SessionAction::Info => {
                if p.root.is_some() {
                    return Err(
                        "info takes no `root`: it shows the folder already granted as this root"
                            .to_string(),
                    );
                }
                let path = path_string(core::session_root(&self.cx, &p.root_id)?);
                Ok(Json(PathOut { path }))
            }
        })
    }

    #[tool(
        description = "Read a project file or list one directory level: the read half of working in a granted root. `read` returns a file as UTF-8 text; `list` lists one level, with rel defaulting to the root. Both stay inside the root: escapes are refused.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    fn file(&self, Parameters(p): Parameters<FileParams>) -> Result<Json<FileOut>, String> {
        self.tool("file", || match p.action {
            FileAction::Read => {
                let rel = p.rel.as_deref().ok_or_else(|| {
                    "read requires `rel`: the project-relative file to read".to_string()
                })?;
                let text = core::read_text(&self.cx, &p.root_id, rel)?;
                Ok(Json(FileOut::Text(TextOut { text })))
            }
            FileAction::List => {
                let entries =
                    core::list_dir(&self.cx, &p.root_id, p.rel.as_deref().unwrap_or("."))?
                        .into_iter()
                        .map(|e| EntryOut {
                            name: e.name,
                            entry_type: e.entry_type,
                        })
                        .collect();
                Ok(Json(FileOut::Listing(ListOut { entries })))
            }
        })
    }

    #[tool(
        description = "Compile main_rel and read back everything about the run, in one place. `start` begins a job (defaulting to the app's main file when main_rel is absent) and returns its job id; `poll` waits for or reports the running job's lines so far; `cancel` stops a running job. `diagnostics`, `prechecks`, `readiness`, `output` and `log` read the last run's errors, pre-compile blockers, offline state, pdf stamp and engine log tail without compiling.",
        meta = ui_meta(Some(COMPILE_VIEW_URI), &["model", "app"])
    ,
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = true)
    )]
    async fn paper(
        &self,
        Parameters(p): Parameters<PaperParams>,
    ) -> Result<Json<PaperOut>, String> {
        self.tool_async("paper", async {
            match p.action {
                PaperAction::Start => {
                    if p.job_id.is_some() {
                        return Err(
                            "start takes no `job_id`: it begins a job and returns one".to_string(),
                        );
                    }
                    if p.tail_lines.is_some() {
                        return Err(
                            "start takes no `tail_lines`: it begins a job; `poll` reports its lines"
                                .to_string(),
                        );
                    }
                    if p.wait_ms.is_some() {
                        return Err(
                            "start takes no `wait_ms`: it begins a job; `poll` waits on it".to_string(),
                        );
                    }
                    if p.max.is_some() {
                        return Err(
                            "start takes no `max`: it begins a job; `diagnostics` caps its rows"
                                .to_string(),
                        );
                    }
                    let root_id = p.root_id.as_deref().ok_or_else(|| {
                        "start requires `root_id`: the granted project to compile".to_string()
                    })?;
                    let rel = match p.main_rel {
                        Some(r) => r,
                        None => {
                            let resolved = core::mainfile::resolve(&self.cx, root_id, None)?;
                            let abs = resolved
                                .main
                                .ok_or_else(|| format!("no main file resolved for root {root_id}"))?;
                            let root = core::session_root(&self.cx, root_id)?;
                            std::path::Path::new(&abs)
                                .strip_prefix(&root)
                                .map_err(|_| format!("resolved main file is outside the project: {abs}"))?
                                .to_string_lossy()
                                .to_string()
                        }
                    };
                    let job_id = core::run_job(
                        &self.cx,
                        root_id,
                        &rel,
                        p.networked.unwrap_or(false),
                        core::compile::COMPILE_TIMEOUT_SECS,
                    )?;
                    Ok(Json(PaperOut::Started(CompileRunOut {
                        job_id,
                        main_rel: rel,
                    })))
                }
                PaperAction::Poll => {
                    for (name, set) in [
                        ("root_id", p.root_id.is_some()),
                        ("main_rel", p.main_rel.is_some()),
                        ("networked", p.networked.is_some()),
                        ("max", p.max.is_some()),
                    ] {
                        if set {
                            return Err(format!(
                                "poll takes only `job_id` with `tail_lines` and `wait_ms`; remove `{name}`"
                            ));
                        }
                    }
                    let job_id = p.job_id.as_deref().ok_or_else(|| {
                        "poll requires `job_id`: the job id from `start`".to_string()
                    })?;
                    let tail = p.tail_lines.unwrap_or(50);
                    let wait =
                        std::time::Duration::from_millis(p.wait_ms.unwrap_or(0).min(MAX_POLL_WAIT_MS));
                    let deadline = tokio::time::Instant::now() + wait;
                    let mut r = core::poll_job(&self.cx, job_id, tail)?;
                    while r.status == core::JobStatus::Running
                        && tokio::time::Instant::now() < deadline
                    {
                        tokio::time::sleep_until(deadline.min(tokio::time::Instant::now() + POLL_STEP))
                            .await;
                        r = core::poll_job(&self.cx, job_id, tail)?;
                    }
                    Ok(Json(PaperOut::Polled(CompilePollOut {
                        status: r.status.as_str().to_string(),
                        pdf_url: r.pdf_url,
                        log: r.log,
                        lines: r.lines,
                        missing: r.missing,
                        progress: r.progress,
                    })))
                }
                PaperAction::Cancel => {
                    for (name, set) in [
                        ("root_id", p.root_id.is_some()),
                        ("main_rel", p.main_rel.is_some()),
                        ("networked", p.networked.is_some()),
                        ("tail_lines", p.tail_lines.is_some()),
                        ("wait_ms", p.wait_ms.is_some()),
                        ("max", p.max.is_some()),
                    ] {
                        if set {
                            return Err(format!(
                                "cancel takes only `job_id`; remove `{name}`"
                            ));
                        }
                    }
                    let job_id = p.job_id.as_deref().ok_or_else(|| {
                        "cancel requires `job_id`: the job id from `start`".to_string()
                    })?;
                    let status = core::cancel_job(&self.cx, job_id)?;
                    Ok(Json(PaperOut::Cancelled(CancelOut { status })))
                }
                PaperAction::Diagnostics => {
                    for (name, set) in [
                        ("job_id", p.job_id.is_some()),
                        ("networked", p.networked.is_some()),
                        ("tail_lines", p.tail_lines.is_some()),
                        ("wait_ms", p.wait_ms.is_some()),
                    ] {
                        if set {
                            return Err(format!(
                                "diagnostics takes only `root_id`, `main_rel` and `max`; remove `{name}`"
                            ));
                        }
                    }
                    let root_id = p.root_id.as_deref().ok_or_else(|| {
                        "diagnostics requires `root_id`: the granted project to read".to_string()
                    })?;
                    let main_rel = p.main_rel.as_deref().ok_or_else(|| {
                        "diagnostics requires `main_rel`: the main file whose last compile to read"
                            .to_string()
                    })?;
                    Ok(Json(PaperOut::Diagnostics(core::structure::diagnostics(
                        &self.cx,
                        root_id,
                        main_rel,
                        p.max.unwrap_or(100),
                    )?)))
                }
                PaperAction::Prechecks => {
                    for (name, set) in [
                        ("job_id", p.job_id.is_some()),
                        ("networked", p.networked.is_some()),
                        ("tail_lines", p.tail_lines.is_some()),
                        ("wait_ms", p.wait_ms.is_some()),
                        ("max", p.max.is_some()),
                    ] {
                        if set {
                            return Err(format!(
                                "prechecks takes only `root_id` and `main_rel`; remove `{name}`"
                            ));
                        }
                    }
                    let root_id = p.root_id.as_deref().ok_or_else(|| {
                        "prechecks requires `root_id`: the granted project to check".to_string()
                    })?;
                    let main_rel = p.main_rel.as_deref().ok_or_else(|| {
                        "prechecks requires `main_rel`: the main file to check".to_string()
                    })?;
                    Ok(Json(PaperOut::Prechecks(
                        core::structure::precompile_checks(&self.cx, root_id, main_rel)?,
                    )))
                }
                PaperAction::Readiness => {
                    for (name, set) in [
                        ("job_id", p.job_id.is_some()),
                        ("main_rel", p.main_rel.is_some()),
                        ("networked", p.networked.is_some()),
                        ("tail_lines", p.tail_lines.is_some()),
                        ("wait_ms", p.wait_ms.is_some()),
                        ("max", p.max.is_some()),
                    ] {
                        if set {
                            return Err(format!(
                                "readiness takes only `root_id`; remove `{name}`"
                            ));
                        }
                    }
                    let root_id = p.root_id.as_deref().ok_or_else(|| {
                        "readiness requires `root_id`: the granted project to assess".to_string()
                    })?;
                    Ok(Json(PaperOut::Readiness(core::compile::offline_readiness(
                        &self.cx, root_id,
                    )?)))
                }
                PaperAction::Output => {
                    for (name, set) in [
                        ("job_id", p.job_id.is_some()),
                        ("networked", p.networked.is_some()),
                        ("tail_lines", p.tail_lines.is_some()),
                        ("wait_ms", p.wait_ms.is_some()),
                        ("max", p.max.is_some()),
                    ] {
                        if set {
                            return Err(format!(
                                "output takes only `root_id` and `main_rel`; remove `{name}`"
                            ));
                        }
                    }
                    let root_id = p.root_id.as_deref().ok_or_else(|| {
                        "output requires `root_id`: the granted project to read".to_string()
                    })?;
                    let main_rel = p.main_rel.as_deref().ok_or_else(|| {
                        "output requires `main_rel`: the main file whose compiled pdf to stamp"
                            .to_string()
                    })?;
                    Ok(Json(PaperOut::Stamped(OutputStampOut {
                        stamp: core::output_stamp(&self.cx, root_id, main_rel)?,
                    })))
                }
                PaperAction::Log => {
                    for (name, set) in [
                        ("job_id", p.job_id.is_some()),
                        ("networked", p.networked.is_some()),
                        ("tail_lines", p.tail_lines.is_some()),
                        ("wait_ms", p.wait_ms.is_some()),
                    ] {
                        if set {
                            return Err(format!(
                                "log takes only `root_id`, `main_rel` and `max`; remove `{name}`"
                            ));
                        }
                    }
                    let root_id = p.root_id.as_deref().ok_or_else(|| {
                        "log requires `root_id`: the granted project whose log to read".to_string()
                    })?;
                    let main_rel = p.main_rel.as_deref().ok_or_else(|| {
                        "log requires `main_rel`: the main file whose engine log to tail"
                            .to_string()
                    })?;
                    Ok(Json(PaperOut::Logged(TextOut {
                        text: core::log_tail(&self.cx, root_id, main_rel, p.max.unwrap_or(50))?,
                    })))
                }
            }
        })
        .await
    }

    #[tool(
        description = "Read export state without writing anything: `stamp` reports main_rel's compiled-pdf stamp (mtimeMs, bytes; null before any compile), `status` reports whether a previous compile left a pdf for main_rel, `cancel` stops a running bundle export's conversion. Needs main_rel except for cancel, which takes nothing else.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    fn export_read(
        &self,
        Parameters(p): Parameters<ExportReadParams>,
    ) -> Result<Json<ExportReadOut>, String> {
        self.tool("export_read", || match p.action {
            ExportReadAction::Stamp => {
                let main_rel = p.main_rel.as_deref().ok_or_else(|| {
                    "stamp requires `main_rel`: the main file whose compiled pdf to stamp"
                        .to_string()
                })?;
                Ok(Json(ExportReadOut::Stamp(OutputStampOut {
                    stamp: core::output_stamp(&self.cx, &p.root_id, main_rel)?,
                })))
            }
            ExportReadAction::Status => {
                let main_rel = p.main_rel.as_deref().ok_or_else(|| {
                    "status requires `main_rel`: the main file whose compiled pdf to look for"
                        .to_string()
                })?;
                Ok(Json(ExportReadOut::Status(ExportStatusOut {
                    fresh: core::outputs_fresh(&self.cx, &p.root_id, main_rel)?,
                })))
            }
            ExportReadAction::Cancel => {
                if p.main_rel.is_some() {
                    return Err(
                        "cancel takes no `main_rel`: it stops the running bundle export's conversion"
                            .to_string(),
                    );
                }
                Ok(Json(ExportReadOut::Cancelled(CancelOut {
                    status: core::reflow::convert::cancel(&self.cx)?,
                })))
            }
        })
    }

    #[tool(
        description = "Start from a bundled or user template: the way to create a project. `list` shows every template with its main file; `create` builds parent_dir/name from one (refused when that folder exists and is not empty) and returns the new root and main file. Grant the returned root to work in it.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    fn scaffold(
        &self,
        Parameters(p): Parameters<ScaffoldParams>,
    ) -> Result<Json<ScaffoldOut>, String> {
        self.tool("scaffold", || match p.action {
            ScaffoldAction::List => {
                for (name, set) in [
                    ("template", p.template.is_some()),
                    ("parent_dir", p.parent_dir.is_some()),
                    ("name", p.name.is_some()),
                ] {
                    if set {
                        return Err(format!(
                            "list takes no `{name}` (pass it under action `create`)"
                        ));
                    }
                }
                Ok(Json(ScaffoldOut::List(core::templates::list())))
            }
            ScaffoldAction::Create => {
                let template = p.template.as_deref().ok_or_else(|| {
                    "create requires `template`: a template id from `list` (or \"welcome\")"
                        .to_string()
                })?;
                let parent_dir = p.parent_dir.as_deref().ok_or_else(|| {
                    "create requires `parent_dir`: the absolute folder the new project goes in"
                        .to_string()
                })?;
                let name = p.name.as_deref().ok_or_else(|| {
                    "create requires `name`: the new project's folder name".to_string()
                })?;
                Ok(Json(ScaffoldOut::Created(core::templates::instantiate(
                    template, parent_dir, name,
                )?)))
            }
        })
    }

    #[tool(
        description = "Three views of one thing: the interactive widgets of main_rel's last compile. `list` shows every widget in document order; `status` reports the approval state of each html widget and runtime so you can ask the user about all of them at once; `check` reports whether one widget may run. Read-only: only the user approves, in the app's View > Widgets.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    fn widget(&self, Parameters(p): Parameters<WidgetParams>) -> Result<Json<WidgetOut>, String> {
        self.tool("widget", || match p.action {
            WidgetAction::List => {
                if p.widget.is_some() {
                    return Err(
                        "list takes no `widget`: it lists every widget; `check` reads one"
                            .to_string(),
                    );
                }
                let r = core::widgets::widgets(&self.cx, &p.root_id, &p.main_rel);
                let _ = core::eventlog::append(std::slice::from_ref(&core::widgets::event(
                    &p.main_rel,
                    &r,
                    maleficium_events::Actor::Agent,
                )));
                Ok(Json(WidgetOut::Listed(r?)))
            }
            WidgetAction::Status => {
                if p.widget.is_some() {
                    return Err(
                        "status takes no `widget`: it reports every widget; `check` reads one"
                            .to_string(),
                    );
                }
                let s = core::widget_approval::widgets_status(&self.cx, &p.root_id, &p.main_rel)?;
                let changed: Vec<_> = s
                    .widgets
                    .iter()
                    .chain(s.runtimes.iter())
                    .filter_map(|w| {
                        core::widget_approval::digest_changed_event(
                            &p.root_id,
                            w,
                            maleficium_events::Actor::Agent,
                        )
                    })
                    .collect();
                let _ = core::eventlog::append(&changed);
                Ok(Json(WidgetOut::Status(Box::new(s))))
            }
            WidgetAction::Check => {
                let widget = p.widget.as_deref().ok_or_else(|| {
                    "check requires `widget`: the widget id, as `list` lists it".to_string()
                })?;
                let list = core::widgets::widgets(&self.cx, &p.root_id, &p.main_rel)?;
                let w = list
                    .widgets
                    .iter()
                    .find(|w| w.id == widget)
                    .ok_or_else(|| format!("{} has no widget {}", p.main_rel, widget))?;
                let target =
                    core::widget_approval::WidgetTarget::of(&p.main_rel, w)?.ok_or_else(|| {
                        format!("widget {widget} runs a first-party runtime: it needs no approval")
                    })?;
                let checked =
                    core::widget_approval::check_widget_approval(&self.cx, &p.root_id, &target)?;
                if let Some(e) = core::widget_approval::digest_changed_event(
                    &p.root_id,
                    &checked.status,
                    maleficium_events::Actor::Agent,
                ) {
                    let _ = core::eventlog::append(std::slice::from_ref(&e));
                }
                Ok(Json(WidgetOut::Checked(Box::new(WidgetCheckOut {
                    status: checked.status,
                }))))
            }
        })
    }

    #[tool(
        description = "The non-destructive half of custom widget runtime authoring: drafts live in the user library, never in a project. `scaffold` starts a draft from scratch or forks the built-in model viewer; `validate` checks a package's manifest, entry, vendored files and static scan, in the library or in a project's installed copy. Nothing here approves or installs: the user reviews and allows the draft in the app (View > Widgets), and only a draft that validates installs.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    fn runtime_read(
        &self,
        Parameters(p): Parameters<RuntimeReadParams>,
    ) -> Result<Json<RuntimeReadOut>, String> {
        self.tool("runtime_read", || match p.action {
            RuntimeReadAction::Scaffold => {
                for (name, set) in [
                    ("reference", p.reference.is_some()),
                    ("root_id", p.root_id.is_some()),
                ] {
                    if set {
                        return Err(format!(
                            "scaffold takes only `name` and `from`; remove `{name}` (use action `validate` to check a package)"
                        ));
                    }
                }
                let name = p.name.as_deref().ok_or_else(|| {
                    "scaffold requires `name`: the new draft's name".to_string()
                })?;
                let dir = core::runtime_author::scaffold(name, p.from.as_deref())?;
                let reference = dir
                    .file_name()
                    .map(|f| f.to_string_lossy().to_string())
                    .unwrap_or_default();
                Ok(Json(RuntimeReadOut::Scaffolded(RuntimeScaffoldOut {
                    reference,
                    path: path_string(dir),
                })))
            }
            RuntimeReadAction::Validate => {
                for (name, set) in [
                    ("name", p.name.is_some()),
                    ("from", p.from.is_some()),
                ] {
                    if set {
                        return Err(format!(
                            "validate takes only `reference` and `root_id`; remove `{name}` (use action `scaffold` to start a draft)"
                        ));
                    }
                }
                let reference = p.reference.as_deref().ok_or_else(|| {
                    "validate requires `reference`: the package ref".to_string()
                })?;
                let v = match &p.root_id {
                    Some(root_id) => {
                        core::runtime_author::validate_project(&self.cx, root_id, reference)?
                    }
                    None => core::runtime_author::validate_library(reference)?,
                };
                let valid = v.valid();
                Ok(Json(RuntimeReadOut::Validated(RuntimeValidateOut {
                    reference: v.reference,
                    path: v.path,
                    valid,
                    errors: v.errors,
                    warnings: v.warnings,
                    manifest: v
                        .manifest
                        .map(|m| serde_json::to_value(&m).unwrap_or(serde_json::Value::Null)),
                })))
            }
        })
    }

    #[tool(
        description = "See the user's open Maleficium windows, or ask one to open a project: the read-only way to meet them where they are. `windows` lists every window with its project, files and root_id, most recently active first. `request_open` shows the user a prompt naming a project folder; nothing opens until they click Open.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    fn presence(
        &self,
        Parameters(p): Parameters<PresenceParams>,
    ) -> Result<Json<PresenceOut>, String> {
        self.tool("presence", || match p.action {
            PresenceAction::Windows => {
                for (name, set) in [
                    ("project", p.project.is_some()),
                    ("file", p.file.is_some()),
                    ("pid", p.pid.is_some()),
                    ("wait_ms", p.wait_ms.is_some()),
                ] {
                    if set {
                        return Err(format!(
                            "windows takes no `{name}` (pass it under action `request_open`)"
                        ));
                    }
                }
                let requests = core::presence::requests();
                let windows: Vec<AppWindow> = core::presence::list()
                    .into_iter()
                    .map(|p| AppWindow {
                        request: requests
                            .iter()
                            .find(|r| r.pid == p.pid)
                            .and_then(|r| serde_json::to_value(r).ok()),
                        root_id: p
                            .project
                            .as_deref()
                            .and_then(|d| core::presence::granted_as(&self.cx, d)),
                        pid: p.pid,
                        project: p.project,
                        main_rel: p.main_rel,
                        active_rel: p.active_rel,
                        updated_ms: p.updated_ms,
                    })
                    .collect();
                let with_project = windows.iter().filter(|w| w.project.is_some()).count();
                let hint = match (windows.len(), with_project) {
                    (0, _) => "No Maleficium window is open. Ask the user to open the app and their project, or grant a folder they name.".to_string(),
                    (_, 0) => "Maleficium is open but holds no project folder. Ask the user which folder to work in, or to open it in the app.".to_string(),
                    (_, 1) => "Grant the open project's folder (session with action grant and root = project) unless it has a root_id already; work there so the user sees your edits and compiles.".to_string(),
                    _ => "Several projects are open: ask the user which one to work in before granting it; the first is the most recently active.".to_string(),
                };
                Ok(Json(PresenceOut::Windows(AppWindowsOut { windows, hint })))
            }
            PresenceAction::RequestOpen => {
                let project = p.project.as_deref().ok_or_else(|| {
                    "request_open requires `project`: the absolute folder to ask the window to open"
                        .to_string()
                })?;
                let pid = match p.pid {
                    Some(pid) => Some(narrow_u32(pid, "pid")?),
                    None => None,
                };
                let r = core::presence::request_open(pid, project, p.file.as_deref())?;
                let wait = std::time::Duration::from_millis(p.wait_ms.unwrap_or(30_000).min(120_000));
                let start = std::time::Instant::now();
                let mut now = r.clone();
                while now.outcome.is_none() && start.elapsed() < wait {
                    std::thread::sleep(std::time::Duration::from_millis(250));
                    match core::presence::request_state(r.pid, &r.id) {
                        Some(s) => now = s,
                        None => break,
                    }
                }
                let (outcome, hint) = match now.outcome {
                    Some(core::presence::OpenOutcome::Opened) => (
                        "opened",
                        match core::presence::granted_as(&self.cx, &now.project) {
                            Some(id) => format!("The user opened it. Work in it as root_id {id}; they see your edits and compiles."),
                            None => "The user opened it. Grant the folder (session with action grant and root = project) to work in it; they see your edits and compiles.".to_string(),
                        },
                    ),
                    Some(core::presence::OpenOutcome::Dismissed) => (
                        "dismissed",
                        "The user dismissed it. Do not ask again unless they say so.".to_string(),
                    ),
                    None => (
                        "waiting",
                        "The user has not answered: the prompt stays in their window. Tell them it is there; presence with action windows shows their answer later. Do not send it again.".to_string(),
                    ),
                };
                Ok(Json(PresenceOut::Asked(AppRequestOpenOut {
                    request_id: r.id,
                    pid: r.pid,
                    project: r.project,
                    file: r.file,
                    outcome: outcome.to_string(),
                    hint,
                })))
            }
        })
    }

    #[tool(
        description = "Look up one tool cluster's contract: its JSON schema, hints, examples and token-flow notes. Pass cluster (a tool name); add action for one action's calling rules. Read-only; start here instead of guessing a grouped tool's fields.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    fn describe(
        &self,
        Parameters(p): Parameters<DescribeParams>,
    ) -> Result<Json<DescribeOut>, String> {
        self.tool("describe", || {
            let tools = Maleficium::tool_router().list_all();
            let names: Vec<&str> = tools.iter().map(|t| t.name.as_ref()).collect();
            let t = tools.iter().find(|t| t.name == p.cluster).ok_or_else(|| {
                let mut msg = format!(
                    "unknown cluster `{}`; valid clusters: [{}]",
                    p.cluster,
                    names.join(", ")
                );
                if let Some(s) = did_you_mean(&p.cluster, &names) {
                    msg.push_str(&format!("; did you mean `{s}`?"));
                }
                msg
            })?;
            if let Some(action) = p.action.as_deref() {
                let valid = cluster_actions(&p.cluster).ok_or_else(|| {
                    format!(
                        "cluster `{}` takes no action; call describe with cluster alone",
                        p.cluster
                    )
                })?;
                if !valid.contains(&action) {
                    let mut msg = format!(
                        "unknown action `{action}` for cluster `{}`; valid actions: [{}]",
                        p.cluster,
                        valid.join(", ")
                    );
                    if let Some(s) = did_you_mean(action, valid) {
                        msg.push_str(&format!("; did you mean `{s}`?"));
                    }
                    return Err(msg);
                }
            }
            let (mut hints, mut examples, token_flow) = cluster_notes(&p.cluster);
            if let Some((hint, example)) = p
                .action
                .as_deref()
                .and_then(|action| action_notes(&p.cluster, action))
            {
                hints.insert(0, hint.to_string());
                examples.insert(0, example);
            }
            Ok(Json(DescribeOut {
                cluster: p.cluster,
                action: p.action,
                description: t.description.as_deref().unwrap_or("").to_string(),
                input_schema: serde_json::to_value(&*t.input_schema)
                    .unwrap_or(serde_json::Value::Null),
                hints,
                examples,
                token_flow,
            }))
        })
    }

    #[tool(
        description = "Install a custom widget runtime from the user library into a project: copies maleficium-runtimes/<ref>/ to the project's runtimes/<ref>/, which \\interactiveruntime[runtime=<ref>] uses. Only a draft that validates installs (run runtime_validate first). A different copy already in the project is kept unless replace is true; the same files report unchanged. Installing never approves. When the result has ask_user true, ask the user to allow the runtime in the app (View > Widgets) as its hint says: auto-approval covers only an update to a runtime the user already allowed, never a first install.",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    fn runtime_install(
        &self,
        Parameters(p): Parameters<RuntimeInstallParams>,
    ) -> Result<Json<RuntimeInstallOut>, String> {
        self.tool("runtime_install", || {
            let done = core::runtime_author::install(
                &self.cx,
                &p.root_id,
                &p.reference,
                p.replace.unwrap_or(false),
            )?;
            let status =
                core::widget_approval::check_runtime(&self.cx, &p.root_id, &p.reference, &[])?
                    .status
                    .ok_or_else(|| {
                        format!("{} installed but does not read as a runtime", p.reference)
                    })?;
            let next = core::runtime_author::after_install(&p.reference, &status);
            Ok(Json(RuntimeInstallOut {
                reference: done.reference,
                path: done.rel,
                files: done.files,
                outcome: done.outcome.as_str().to_string(),
                approved: next.approved,
                auto_approve: next.auto_approve,
                ask_user: next.ask_user,
                hint: next.hint,
            }))
        })
    }

    #[tool(
        description = "Write an export outside the project, never into it: `pdf` copies main_rel's compiled pdf to dest, `zip` packs the sources to dest, `bundle` writes a paper bundle at dest, `preview` writes the single-file bundle to a scratch folder and returns its path without opening anything, `open` previews then opens that path in the OS browser. Every dest is absolute and outside the project; re-exporting over an earlier bundle replaces it, a foreign folder is refused.",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    fn export_write(
        &self,
        Parameters(p): Parameters<ExportWriteParams>,
    ) -> Result<Json<ExportWriteOut>, String> {
        self.tool("export_write", || match p.action {
            ExportWriteAction::Pdf => {
                let main_rel = p.main_rel.as_deref().ok_or_else(|| {
                    "pdf requires `main_rel`: the main file whose compiled pdf to copy".to_string()
                })?;
                let dest = p.dest.as_deref().ok_or_else(|| {
                    "pdf requires `dest`: the absolute path outside the project to write"
                        .to_string()
                })?;
                for (name, set) in [
                    ("profile", p.profile.is_some()),
                    ("size_cap_bytes", p.size_cap_bytes.is_some()),
                ] {
                    if set {
                        return Err(format!(
                            "pdf takes only `main_rel` and `dest`; remove `{name}` (use action `bundle` for a paper bundle)"
                        ));
                    }
                }
                Ok(Json(ExportWriteOut::Exported(core::export::export_pdf(
                    &self.cx,
                    &p.root_id,
                    main_rel,
                    dest,
                )?)))
            }
            ExportWriteAction::Zip => {
                let dest = p.dest.as_deref().ok_or_else(|| {
                    "zip requires `dest`: the absolute path outside the project to write".to_string()
                })?;
                for (name, set) in [
                    ("main_rel", p.main_rel.is_some()),
                    ("profile", p.profile.is_some()),
                    ("size_cap_bytes", p.size_cap_bytes.is_some()),
                ] {
                    if set {
                        return Err(format!(
                            "zip takes only `dest`; remove `{name}` (use action `pdf` or `bundle` for a compiled output)"
                        ));
                    }
                }
                Ok(Json(ExportWriteOut::Exported(core::export::export_zip(
                    &self.cx, &p.root_id, dest,
                )?)))
            }
            ExportWriteAction::Bundle => {
                let main_rel = p.main_rel.as_deref().ok_or_else(|| {
                    "bundle requires `main_rel`: the main file whose last compile to bundle"
                        .to_string()
                })?;
                let dest = p.dest.as_deref().ok_or_else(|| {
                    "bundle requires `dest`: the absolute path outside the project to write"
                        .to_string()
                })?;
                let profile = p.profile.ok_or_else(|| {
                    "bundle requires `profile`: folder, single-file or hosted".to_string()
                })?;
                let r = core::bundle::export_bundle(
                    &self.cx,
                    &p.root_id,
                    main_rel,
                    dest,
                    profile,
                    p.size_cap_bytes,
                );
                Self::log_bundle_event(main_rel, profile, &r);
                Ok(Json(ExportWriteOut::Bundle(r?)))
            }
            ExportWriteAction::Preview => {
                let main_rel = p.main_rel.as_deref().ok_or_else(|| {
                    "preview requires `main_rel`: the main file whose last compile to preview"
                        .to_string()
                })?;
                for (name, set) in [
                    ("dest", p.dest.is_some()),
                    ("profile", p.profile.is_some()),
                    ("size_cap_bytes", p.size_cap_bytes.is_some()),
                ] {
                    if set {
                        return Err(format!(
                            "preview takes only `main_rel`; remove `{name}` (the core picks the scratch folder)"
                        ));
                    }
                }
                let r = core::bundle::preview_bundle(&self.cx, &p.root_id, main_rel);
                Self::log_bundle_event(
                    main_rel,
                    core::bundle::BundleProfile::SingleFile,
                    &r,
                );
                let r = r?;
                Ok(Json(ExportWriteOut::Preview(PreviewBundleOut {
                    hint: format!("open {} in a browser to view it", r.path),
                    path: r.path,
                    bytes: r.bytes,
                    widgets: r.widgets,
                    warnings: r.warnings,
                })))
            }
            ExportWriteAction::Open => {
                let main_rel = p.main_rel.as_deref().ok_or_else(|| {
                    "open requires `main_rel`: the main file whose last compile to show in the browser"
                        .to_string()
                })?;
                for (name, set) in [
                    ("dest", p.dest.is_some()),
                    ("profile", p.profile.is_some()),
                    ("size_cap_bytes", p.size_cap_bytes.is_some()),
                ] {
                    if set {
                        return Err(format!(
                            "open takes only `main_rel`; remove `{name}` (use action `preview` to only get the path)"
                        ));
                    }
                }
                let r = core::bundle::preview_bundle(&self.cx, &p.root_id, main_rel);
                Self::log_bundle_event(
                    main_rel,
                    core::bundle::BundleProfile::SingleFile,
                    &r,
                );
                let r = r?;
                open::that_detached(&r.path)
                    .map_err(|e| format!("cannot open {} in the browser: {e}", r.path))?;
                Ok(Json(ExportWriteOut::Bundle(r)))
            }
        })
    }

    #[tool(
        description = "Look at the compiled PDF through SyncTeX or a rendered region: how the output shows a source position. `forward` maps a source line (tex_rel with line) to its PDF page; `inverse` maps a PDF position (page with x and y) to its source line; `snippet` returns the page and region where a source line, label or page landed, with its source lines (and a PNG when with_image is true). Needs a compiled main_rel; stale is true when sources changed after the compile.",
        output_schema = rmcp::handler::server::common::schema_for_output::<SyncViewOut>(),
        meta = ui_meta(Some(SNIPPET_VIEW_URI), &["model", "app"])
    ,
        annotations(read_only_hint = true, destructive_hint = false, idempotent_hint = true, open_world_hint = false)
    )]
    fn sync_view(
        &self,
        Parameters(p): Parameters<SyncViewParams>,
    ) -> Result<rmcp::model::CallToolResult, String> {
        self.tool("sync_view", || {
            use base64::Engine as _;
            let value = match p.action {
                SyncViewAction::Forward => {
                    let tex_rel = p.tex_rel.as_deref().ok_or_else(|| {
                        "forward requires `tex_rel`: the source file whose line to map".to_string()
                    })?;
                    let line = p.line.ok_or_else(|| {
                        "forward requires `line`: the 1-based source line to map".to_string()
                    })?;
                    p.reject(
                        &[
                            ("label", p.label.is_some()),
                            ("page", p.page.is_some()),
                            ("x", p.x.is_some()),
                            ("y", p.y.is_some()),
                            ("with_image", p.with_image.is_some()),
                        ],
                        "forward",
                    )?;
                    let hit = core::forward(
                        &self.cx,
                        &p.root_id,
                        &p.main_rel,
                        tex_rel,
                        narrow_u32(line, "line")?,
                    )?;
                    serde_json::to_value(&hit).map_err(|e| e.to_string())?
                }
                SyncViewAction::Inverse => {
                    let page = p.page.ok_or_else(|| {
                        "inverse requires `page`: the 1-based PDF page to map back".to_string()
                    })?;
                    p.reject(
                        &[
                            ("tex_rel", p.tex_rel.is_some()),
                            ("line", p.line.is_some()),
                            ("label", p.label.is_some()),
                            ("with_image", p.with_image.is_some()),
                        ],
                        "inverse",
                    )?;
                    let hit = core::inverse(
                        &self.cx,
                        &p.root_id,
                        &p.main_rel,
                        narrow_u32(page, "page")?,
                        p.x.unwrap_or(0.0),
                        p.y.unwrap_or(0.0),
                    )?;
                    serde_json::to_value(&hit).map_err(|e| e.to_string())?
                }
                SyncViewAction::Snippet => {
                    p.reject(&[("x", p.x.is_some()), ("y", p.y.is_some())], "snippet")?;
                    let target = p.target()?;
                    let s = core::snippet::snippet(
                        &self.cx,
                        &p.root_id,
                        &p.main_rel,
                        &target,
                        p.with_image.unwrap_or(false),
                    )?;
                    let value = serde_json::to_value(&s).map_err(|e| e.to_string())?;
                    if let Some(png) = &s.png {
                        let data = base64::engine::general_purpose::STANDARD.encode(png);
                        let mut result = rmcp::model::CallToolResult::structured(value);
                        result
                            .content
                            .insert(0, rmcp::model::ContentBlock::image(data, "image/png"));
                        return Ok(result);
                    }
                    value
                }
            };
            Ok(rmcp::model::CallToolResult::structured(value))
        })
    }

    #[tool(
        description = "Re-render a page of main_rel's compiled PDF, or a band of it, as a PNG at another scale: the View's own refresh. `render` returns the PNG for page (or its region band); scale defaults to 2 and clamps to 0.5..3. App-only: the model reads page images through sync_view's snippet instead.",
        output_schema = rmcp::handler::server::common::schema_for_output::<SyncRenderOut>(),
        meta = ui_meta(Some(SNIPPET_VIEW_URI), &["app"])
    ,
        annotations(read_only_hint = true, destructive_hint = false, idempotent_hint = true, open_world_hint = false)
    )]
    fn sync_render(
        &self,
        Parameters(p): Parameters<SyncRenderParams>,
    ) -> Result<rmcp::model::CallToolResult, String> {
        self.tool("sync_render", || match p.action {
            SyncRenderAction::Render => {
                use base64::Engine as _;
                let r = core::snippet::render_page(
                    &self.cx,
                    &p.root_id,
                    &p.main_rel,
                    narrow_u32(p.page, "page")?,
                    p.region,
                    p.scale,
                )?;
                let value = serde_json::to_value(&r).map_err(|e| e.to_string())?;
                let mut result = rmcp::model::CallToolResult::structured(value);
                let data = base64::engine::general_purpose::STANDARD.encode(&r.png);
                result
                    .content
                    .insert(0, rmcp::model::ContentBlock::image(data, "image/png"));
                Ok(result)
            }
        })
    }

    #[tool(
        description = "Delete a project file to the app-local trash, or restore one: the only way to remove project files. `delete` needs no confirm on the first call: without confirm it is refused with the file's absolute path; call again with that path as confirm to delete. `undo` restores a trashed file by its trash_path.",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    fn file_mutate(
        &self,
        Parameters(p): Parameters<FileMutateParams>,
    ) -> Result<Json<FileMutateOut>, String> {
        self.tool("file_mutate", || match p.action {
            FileMutateAction::Delete => {
                let rel = p.rel.as_deref().ok_or_else(|| {
                    "delete requires `rel`: the project-relative file to trash".to_string()
                })?;
                if p.trash_path.is_some() {
                    return Err(
                        "delete takes no `trash_path`: it trashes by `rel`, and `undo` restores by `trash_path`"
                            .to_string(),
                    );
                }
                let confirm = p.confirm.as_deref().unwrap_or("");
                let trash_path = core::trash_file(&self.cx, &p.root_id, rel, confirm)?;
                Ok(Json(FileMutateOut::Deleted(DeleteOut { trash_path })))
            }
            FileMutateAction::Undo => {
                let trash_path = p.trash_path.as_deref().ok_or_else(|| {
                    "undo requires `trash_path`: the trash path from a `delete`".to_string()
                })?;
                for (name, set) in [
                    ("rel", p.rel.is_some()),
                    ("confirm", p.confirm.is_some()),
                ] {
                    if set {
                        return Err(format!(
                            "undo takes only `trash_path`; remove `{name}` (delete trashes by `rel`, undo restores by `trash_path`)"
                        ));
                    }
                }
                let path = core::undo_trash(&self.cx, &p.root_id, trash_path)?;
                Ok(Json(FileMutateOut::Restored(PathOut { path })))
            }
        })
    }

    #[tool(
        description = "Map a document's structure without compiling: the read-only way to learn what a project holds. `outline` sections of one file; `file_graph`, `labels_refs` and `citations` the inputs, labels and cites reachable from main_rel; `document_structure` the conversion's sections, theorems and figures; `definition` where a label, cite key, macro or input is defined. Reads saved files on disk, never the editor buffers.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    fn structure(
        &self,
        Parameters(p): Parameters<StructureParams>,
    ) -> Result<Json<StructureOut>, String> {
        self.tool("structure", || match p.action {
            StructureAction::Outline => {
                let rel = p.rel.as_deref().ok_or_else(|| {
                    "outline requires `rel`: the project-relative file to outline".to_string()
                })?;
                for (name, set) in [
                    ("main_rel", p.main_rel.is_some()),
                    ("kind", p.kind.is_some()),
                    ("key", p.key.is_some()),
                    ("line", p.line.is_some()),
                    ("col", p.col.is_some()),
                ] {
                    if set {
                        return Err(format!(
                            "outline takes only `rel`; remove `{name}` (use action `definition` to look a key up)"
                        ));
                    }
                }
                Ok(Json(StructureOut::Outline(
                    core::structure::outline_of(&self.cx, &p.root_id, rel)?,
                )))
            }
            StructureAction::FileGraph => {
                let main_rel = p.main_rel.as_deref().ok_or_else(|| {
                    "file_graph requires `main_rel`: the main file whose inputs to walk"
                        .to_string()
                })?;
                for (name, set) in [
                    ("rel", p.rel.is_some()),
                    ("kind", p.kind.is_some()),
                    ("key", p.key.is_some()),
                    ("line", p.line.is_some()),
                    ("col", p.col.is_some()),
                ] {
                    if set {
                        return Err(format!(
                            "file_graph takes only `main_rel`; remove `{name}`"
                        ));
                    }
                }
                Ok(Json(StructureOut::FileGraph(core::structure::file_graph(
                    &self.cx,
                    &p.root_id,
                    main_rel,
                )?)))
            }
            StructureAction::LabelsRefs => {
                let main_rel = p.main_rel.as_deref().ok_or_else(|| {
                    "labels_refs requires `main_rel`: the main file whose labels to list"
                        .to_string()
                })?;
                for (name, set) in [
                    ("rel", p.rel.is_some()),
                    ("kind", p.kind.is_some()),
                    ("key", p.key.is_some()),
                    ("line", p.line.is_some()),
                    ("col", p.col.is_some()),
                ] {
                    if set {
                        return Err(format!(
                            "labels_refs takes only `main_rel`; remove `{name}`"
                        ));
                    }
                }
                Ok(Json(StructureOut::LabelsRefs(core::structure::labels_refs(
                    &self.cx,
                    &p.root_id,
                    main_rel,
                )?)))
            }
            StructureAction::Citations => {
                let main_rel = p.main_rel.as_deref().ok_or_else(|| {
                    "citations requires `main_rel`: the main file whose cites to check"
                        .to_string()
                })?;
                for (name, set) in [
                    ("rel", p.rel.is_some()),
                    ("kind", p.kind.is_some()),
                    ("key", p.key.is_some()),
                    ("line", p.line.is_some()),
                    ("col", p.col.is_some()),
                ] {
                    if set {
                        return Err(format!(
                            "citations takes only `main_rel`; remove `{name}`"
                        ));
                    }
                }
                Ok(Json(StructureOut::Citations(core::structure::citations(
                    &self.cx,
                    &p.root_id,
                    main_rel,
                )?)))
            }
            StructureAction::DocumentStructure => {
                let main_rel = p.main_rel.as_deref().ok_or_else(|| {
                    "document_structure requires `main_rel`: the main file to convert"
                        .to_string()
                })?;
                for (name, set) in [
                    ("rel", p.rel.is_some()),
                    ("kind", p.kind.is_some()),
                    ("key", p.key.is_some()),
                    ("line", p.line.is_some()),
                    ("col", p.col.is_some()),
                ] {
                    if set {
                        return Err(format!(
                            "document_structure takes only `main_rel`; remove `{name}`"
                        ));
                    }
                }
                Ok(Json(StructureOut::Document(
                    core::document::document_structure(&self.cx, &p.root_id, main_rel)?,
                )))
            }
            StructureAction::Definition => {
                use core::search::DefinitionTarget;
                let by_key = p.kind.is_some() || p.key.is_some();
                let at_position = p.rel.is_some() || p.line.is_some() || p.col.is_some();
                if by_key && at_position {
                    return Err(
                        "definition takes kind and key, or rel, line and col: not both"
                            .to_string(),
                    );
                }
                let target = match (p.kind, p.key, p.rel, p.line, p.col) {
                    (Some(kind), Some(key), _, _, _) => {
                        let command = (kind == maleficium_index::definition::RefKind::Input)
                            .then(|| "input".to_string());
                        DefinitionTarget::Ref(maleficium_index::definition::RefAt {
                            kind,
                            key,
                            command,
                        })
                    }
                    (_, _, Some(rel), Some(line), Some(col)) => DefinitionTarget::At {
                        rel,
                        line: narrow_u32(line, "line")?,
                        col: narrow_u32(col, "col")?,
                    },
                    _ => return Err("pass kind and key, or rel, line and col".to_string()),
                };
                Ok(Json(StructureOut::Definition(DefinitionOut {
                    lookup: core::search::definition(
                        &self.cx,
                        &p.root_id,
                        target,
                        p.main_rel.as_deref(),
                    )?,
                })))
            }
        })
    }

    #[tool(
        description = "Search project text or find files by name: `search` hunts a label, reference or string across saved files, `find_files` locates a file by fuzzy name. Hits reachable from `main_rel` rank first for `search`. Cap results with `max`.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    fn search(&self, Parameters(p): Parameters<SearchParams>) -> Result<Json<SearchOut>, String> {
        self.tool("search", || {
            match p.action {
                SearchAction::Search => {
                    let pattern = p.pattern.ok_or_else(|| {
                        "search requires `pattern`: the label, reference or string to hunt across saved files".to_string()
                    })?;
                    if p.query.is_some() {
                        return Err(
                            "search takes no `query`; use action `find_files` to locate a file by fuzzy name"
                                .to_string(),
                        );
                    }
                    let q = maleficium_index::search::Query {
                        pattern,
                        regex: p.regex.unwrap_or(false),
                        case_sensitive: p.case_sensitive.unwrap_or(false),
                        whole_word: p.whole_word.unwrap_or(false),
                    };
                    Ok(Json(SearchOut::Hits(core::search::search(
                        &self.cx,
                        &p.root_id,
                        &q,
                        p.main_rel.as_deref(),
                        p.max.unwrap_or(core::search::MAX_HITS),
                    )?)))
                }
                SearchAction::FindFiles => {
                    let query = p.query.ok_or_else(|| {
                        "find_files requires `query`: the fuzzy file-name string".to_string()
                    })?;
                    for (name, set) in [
                        ("pattern", p.pattern.is_some()),
                        ("regex", p.regex.is_some()),
                        ("case_sensitive", p.case_sensitive.is_some()),
                        ("whole_word", p.whole_word.is_some()),
                        ("main_rel", p.main_rel.is_some()),
                    ] {
                        if set {
                            return Err(format!(
                                "find_files takes only query + max; remove `{name}` (use action `search` for text search)"
                            ));
                        }
                    }
                    Ok(Json(SearchOut::Files(FindFilesOut {
                        files: core::search::find_files(
                            &self.cx,
                            &p.root_id,
                            &query,
                            p.max.unwrap_or(core::search::MAX_FILE_MATCHES),
                        )?,
                    })))
                }
            }
        })
    }

    #[tool(
        description = "Preview, apply or undo a project-wide replace: `preview` dry-runs across saved files and returns per-file revisions plus a token, `apply` writes by token and is refused whole if any file changed since the preview, `undo` restores a batch. Always preview before apply; keep the batch to undo.",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    fn replace(
        &self,
        Parameters(p): Parameters<ReplaceParams>,
    ) -> Result<Json<ReplaceOut>, String> {
        self.tool("replace", || {
            match p.action {
                ReplaceAction::Preview => {
                    let pattern = p.pattern.ok_or_else(|| {
                        "preview requires `pattern`: the same text or regex `search` would hunt"
                            .to_string()
                    })?;
                    let replacement = p.replacement.ok_or_else(|| {
                        "preview requires `replacement`: the text to write (`$1` / `${name}` expand under regex=true)".to_string()
                    })?;
                    for (name, set) in [
                        ("token", p.token.is_some()),
                        ("batch", p.batch.is_some()),
                    ] {
                        if set {
                            return Err(format!(
                                "preview takes no `{name}`; it dry-runs and returns a token for `apply`"
                            ));
                        }
                    }
                    let q = maleficium_index::search::Query {
                        pattern,
                        regex: p.regex.unwrap_or(false),
                        case_sensitive: p.case_sensitive.unwrap_or(false),
                        whole_word: p.whole_word.unwrap_or(false),
                    };
                    Ok(Json(ReplaceOut::Preview(core::replace::preview(
                        &self.cx,
                        &p.root_id,
                        &q,
                        &replacement,
                        p.main_rel.as_deref(),
                    )?)))
                }
                ReplaceAction::Apply => {
                    let token = p.token.ok_or_else(|| {
                        "apply requires `token`: the token from `preview`".to_string()
                    })?;
                    for (name, set) in [
                        ("pattern", p.pattern.is_some()),
                        ("replacement", p.replacement.is_some()),
                        ("regex", p.regex.is_some()),
                        ("case_sensitive", p.case_sensitive.is_some()),
                        ("whole_word", p.whole_word.is_some()),
                        ("main_rel", p.main_rel.is_some()),
                        ("batch", p.batch.is_some()),
                    ] {
                        if set {
                            return Err(format!(
                                "apply takes only `token`; remove `{name}` (preview again to change the plan)"
                            ));
                        }
                    }
                    Ok(Json(ReplaceOut::Applied(core::replace::apply(
                        &self.cx,
                        &p.root_id,
                        &token,
                        &[],
                    )?)))
                }
                ReplaceAction::Undo => {
                    let batch = p.batch.ok_or_else(|| {
                        "undo requires `batch`: the batch id from `apply`".to_string()
                    })?;
                    for (name, set) in [
                        ("pattern", p.pattern.is_some()),
                        ("replacement", p.replacement.is_some()),
                        ("regex", p.regex.is_some()),
                        ("case_sensitive", p.case_sensitive.is_some()),
                        ("whole_word", p.whole_word.is_some()),
                        ("main_rel", p.main_rel.is_some()),
                        ("token", p.token.is_some()),
                    ] {
                        if set {
                            return Err(format!(
                                "undo takes only `batch`; remove `{name}`"
                            ));
                        }
                    }
                    Ok(Json(ReplaceOut::Undone(ReplaceUndoOut {
                        restored: core::replace::undo(&self.cx, &p.root_id, &batch)?,
                    })))
                }
            }
        })
    }
}

// Named explicitly: rmcp's default server info is its own crate name and
// version, since its env! expands inside rmcp.
#[tool_handler(name = "maleficium")]
impl rmcp::ServerHandler for Maleficium {
    fn get_info(&self) -> rmcp::model::ServerConfig {
        let mut extensions = rmcp::model::ExtensionCapabilities::new();
        extensions.insert(UI_EXTENSION.to_string(), serde_json::Map::new());
        rmcp::model::ServerConfig::new(
            rmcp::model::ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .enable_extensions_with(extensions)
                .build(),
        )
        .with_server_info(rmcp::model::Implementation::new(
            "maleficium",
            env!("CARGO_PKG_VERSION"),
        ))
    }

    // Views are fetched by the URI a tool names, so they stay out of
    // `resources/list` (the default empty list).
    async fn read_resource(
        &self,
        request: rmcp::model::ReadResourceRequestParams,
        _context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> Result<rmcp::model::ReadResourceResponse, rmcp::ErrorData> {
        Ok(rmcp::model::ReadResourceResult::new(vec![view_resource(&request.uri)?]).into())
    }
}

async fn run_stdio() -> anyhow::Result<()> {
    let service = Maleficium::default().serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}

/// The stdio server until the client hangs up: `maleficium-mcp` and
/// `maleficium --mcp` both land here.
pub fn serve_stdio() -> anyhow::Result<()> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(run_stdio())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A latexml-shaped conversion (see core's reflow/article): the fixed
    /// HTML the document cases convert instead of running the engine.
    const CONVERTED: &str =
        include_str!("../../core/src/reflow/fixtures/interactive-converted.frag");

    /// A converter that hands back fixed HTML.
    struct FixedHtml(String);

    impl core::reflow::convert::Converter for FixedHtml {
        fn convert(
            &self,
            _cx: &core::Core,
            _main: &std::path::Path,
            _work: &std::path::Path,
        ) -> core::reflow::convert::Conversion {
            core::reflow::convert::Conversion {
                html: Ok(self.0.clone()),
                errors: Vec::new(),
                log: String::new(),
            }
        }
    }

    /// Every tool call reports an agent event through the shared writer's
    /// shape: the call name, its outcome, and actor `agent`.
    #[test]
    fn tool_calls_log_typed_agent_events() {
        let ok = mcp_event::<()>("search", &Ok(()));
        assert_eq!(ok.actor, maleficium_events::Actor::Agent);
        assert!(matches!(
            ok.event,
            maleficium_events::AppEvent::McpCall { ref tool, ok: true, ref error }
            if tool == "search" && error.is_none()
        ));
        let line = core::eventlog::serialize(&ok);
        let back: maleficium_events::LogLine = serde_json::from_str(&line).unwrap();
        assert_eq!(back.actor, maleficium_events::Actor::Agent);

        let err = mcp_event::<()>(
            "delete",
            &Err("forbidden path (outside project): x".to_string()),
        );
        assert!(matches!(
            err.event,
            maleficium_events::AppEvent::McpCall { ok: false, .. }
        ));
        assert_eq!(err.kind, maleficium_events::EventKind::Error);
    }

    /// Clients show serverInfo; rmcp's default would name rmcp itself.
    #[test]
    fn server_names_itself_with_the_app_version() {
        let info = rmcp::ServerHandler::get_info(&Maleficium::default()).server_info;
        assert_eq!(info.name, "maleficium");
        assert_eq!(info.version, env!("CARGO_PKG_VERSION"));
    }

    /// The MCP tools add nothing to core's confinement: every escape core
    /// refuses (and so the desktop command, a direct forward), the tool
    /// refuses with the same error.
    #[test]
    fn the_ui_extension_is_advertised_beside_resources() {
        let caps = rmcp::ServerHandler::get_info(&Maleficium::default()).capabilities;
        assert!(caps.resources.is_some() && caps.tools.is_some());
        let ext = caps.extensions.expect("extensions");
        assert!(ext.contains_key(UI_EXTENSION), "{ext:?}");
    }

    /// Only the sync tools and paper name a View; the re-render
    /// tool is hidden from the model, and paper's own readings
    /// (poll, cancel, diagnostics, prechecks, readiness, output, log) stay plain
    /// actions so text-only hosts see them as before.
    #[test]
    fn only_the_view_tools_carry_ui_meta() {
        let tools = Maleficium::tool_router().list_all();
        let ui = |t: &rmcp::model::Tool| {
            t.meta
                .as_ref()
                .and_then(|m| m.get("ui").cloned())
                .map(|v| v.to_string())
        };
        let mut with: Vec<_> = tools
            .iter()
            .filter(|t| ui(t).is_some())
            .map(|t| t.name.to_string())
            .collect();
        with.sort();
        assert_eq!(with, ["paper", "sync_render", "sync_view"]);
        let get = |n: &str| ui(tools.iter().find(|t| t.name == n).unwrap()).unwrap();
        assert!(get("sync_view").contains(SNIPPET_VIEW_URI) && get("sync_view").contains("model"));
        assert_eq!(
            get("sync_render"),
            format!(r#"{{"resourceUri":"{SNIPPET_VIEW_URI}","visibility":["app"]}}"#)
        );
        assert_eq!(
            get("paper"),
            format!(r#"{{"resourceUri":"{COMPILE_VIEW_URI}","visibility":["model","app"]}}"#)
        );
    }

    /// Grouped `widget` folds the three views of one reading: `list` the
    /// widgets, `status` their approval state, `check` one widget. Every
    /// action only reads, so the tool stays honestly read-only, and an
    /// unknown action names the valid list with a did-you-mean hint. Its
    /// failures reach the client as tool errors, never as an empty list.
    #[test]
    fn widget_validates_per_action() {
        let tools = Maleficium::tool_router().list_all();
        let t = tools.iter().find(|t| t.name == "widget").expect("widget");
        let a = t.annotations.as_ref().expect("annotations");
        assert_eq!(a.read_only_hint, Some(true));
        assert_eq!(a.destructive_hint, Some(false));
        assert_eq!(a.idempotent_hint, Some(true));
        assert_eq!(a.open_world_hint, Some(false));
        assert!(t.output_schema.is_some());
        let schema = serde_json::to_value(&*t.input_schema).unwrap();
        let mut props: Vec<&str> = schema["properties"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        props.sort();
        assert_eq!(props, ["action", "main_rel", "root_id", "widget"]);
        assert_eq!(schema["additionalProperties"], false);
        assert!(
            serde_json::from_value::<WidgetParams>(
                serde_json::json!({"root_id": "r", "action": "list", "main_rel": "m.tex"})
            )
            .is_ok()
        );
        assert!(
            serde_json::from_value::<WidgetParams>(
                serde_json::json!({"root_id": "r", "action": "list", "main_rel": "m.tex", "approve": true})
            )
            .is_err()
        );

        let m = Maleficium::default();
        let list = |widget: Option<&str>| {
            m.widget(Parameters(WidgetParams {
                root_id: "nope".into(),
                action: WidgetAction::List,
                main_rel: "main.tex".into(),
                widget: widget.map(Into::into),
            }))
        };
        let err = list(None).err().expect("an ungranted root fails");
        assert!(!err.is_empty());
        let err = list(Some("w")).err().expect("list rejects a widget");
        assert!(err.contains("takes no `widget`"), "{err}");

        let err = m
            .widget(Parameters(WidgetParams {
                root_id: "nope".into(),
                action: WidgetAction::Status,
                main_rel: "main.tex".into(),
                widget: Some("w".into()),
            }))
            .err()
            .expect("status rejects a widget");
        assert!(err.contains("takes no `widget`"), "{err}");

        let err = m
            .widget(Parameters(WidgetParams {
                root_id: "nope".into(),
                action: WidgetAction::Check,
                main_rel: "main.tex".into(),
                widget: None,
            }))
            .err()
            .expect("check without a widget fails");
        assert!(err.contains("widget"), "{err}");

        let err = serde_json::from_value::<WidgetParams>(serde_json::json!({
            "root_id": "r", "action": "chek", "main_rel": "m.tex", "widget": "w",
        }))
        .expect_err("a near-miss action fails");
        assert!(err.to_string().contains("did you mean `check`?"), "{err}");
        let err = serde_json::from_value::<WidgetParams>(serde_json::json!({
            "root_id": "r", "action": "stats", "main_rel": "m.tex",
        }))
        .expect_err("an unknown action fails");
        assert!(err.to_string().contains("[list, status, check]"), "{err}");
    }

    /// The structure tool only reads: hosts may call it without
    /// confirmation. Its parameters refuse unknown fields, and its
    /// failures reach the client as tool errors, never as an empty
    /// document. The `document_structure` action reads the conversion; the
    /// other actions read the saved files and the index.
    #[test]
    fn structure_is_a_read_only_tool_with_per_action_params() {
        let tools = Maleficium::tool_router().list_all();
        let t = tools
            .iter()
            .find(|t| t.name == "structure")
            .expect("structure");
        let a = t.annotations.as_ref().expect("annotations");
        assert_eq!(a.read_only_hint, Some(true));
        assert_eq!(a.destructive_hint, Some(false));
        assert_eq!(a.idempotent_hint, Some(true));
        assert_eq!(a.open_world_hint, Some(false));
        assert!(t.output_schema.is_some());
        let schema = serde_json::to_value(&*t.input_schema).unwrap();
        let mut props: Vec<&str> = schema["properties"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        props.sort();
        assert_eq!(
            props,
            [
                "action", "col", "key", "kind", "line", "main_rel", "rel", "root_id"
            ]
        );
        assert_eq!(schema["additionalProperties"], false);
        assert!(
            serde_json::from_value::<StructureParams>(
                serde_json::json!({"root_id": "r", "action": "outline", "rel": "main.tex"})
            )
            .is_ok()
        );
        assert!(
            serde_json::from_value::<StructureParams>(
                serde_json::json!({"root_id": "r", "action": "outline", "rel": "main.tex", "approve": true})
            )
            .is_err()
        );

        let m = Maleficium::default();
        let dir = core::test_scratch::dir("mcp-document");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("main.tex"), "\\section{Hi}\n").unwrap();
        let root = dunce::canonicalize(&dir).unwrap();
        core::grant_root(&m.cx, "docm", &root.to_string_lossy()).unwrap();
        m.cx.set_converter(std::sync::Arc::new(FixedHtml(CONVERTED.to_string())));
        let d = m
            .structure(Parameters(StructureParams {
                root_id: "docm".into(),
                action: StructureAction::DocumentStructure,
                rel: None,
                main_rel: Some("main.tex".into()),
                kind: None,
                key: None,
                line: None,
                col: None,
            }))
            .expect("a fixed conversion parses")
            .0;
        let StructureOut::Document(d) = d else {
            panic!("document_structure returns its document");
        };
        assert_eq!(d.main, "main.tex");
        assert_eq!(d.sections.len(), 2);
        assert_eq!(d.figures.len(), 4);
        assert_eq!(d.widgets.len(), 5);
        assert_eq!(d.citations.len(), 1);

        let err = m
            .structure(Parameters(StructureParams {
                root_id: "nope".into(),
                action: StructureAction::DocumentStructure,
                rel: None,
                main_rel: Some("main.tex".into()),
                kind: None,
                key: None,
                line: None,
                col: None,
            }))
            .err()
            .expect("an ungranted root fails");
        assert!(!err.is_empty());
    }

    /// The export split is honest about effects: reading state never writes,
    /// writing always can, and neither reaches the network. The read takes a
    /// project, a main file and an action, nothing that could approve or
    /// fetch; the write has no approval argument either.
    #[test]
    fn export_read_is_read_only_and_export_write_is_destructive() {
        let tools = Maleficium::tool_router().list_all();
        let t = tools
            .iter()
            .find(|t| t.name == "export_read")
            .expect("export_read");
        let a = t.annotations.as_ref().expect("annotations");
        assert_eq!(a.read_only_hint, Some(true));
        assert_eq!(a.destructive_hint, Some(false));
        assert_eq!(a.idempotent_hint, Some(true));
        assert_eq!(a.open_world_hint, Some(false));
        assert!(t.output_schema.is_some());
        let schema = serde_json::to_value(&*t.input_schema).unwrap();
        let mut props: Vec<&str> = schema["properties"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        props.sort();
        assert_eq!(props, ["action", "main_rel", "root_id"]);
        assert_eq!(schema["additionalProperties"], false);
        assert!(
            serde_json::from_value::<ExportReadParams>(
                serde_json::json!({"root_id": "r", "action": "stamp", "main_rel": "m.tex"})
            )
            .is_ok()
        );
        assert!(
            serde_json::from_value::<ExportReadParams>(
                serde_json::json!({"root_id": "r", "action": "stamp", "main_rel": "m.tex", "dest": "/tmp/x"})
            )
            .is_err()
        );

        let t = tools
            .iter()
            .find(|t| t.name == "export_write")
            .expect("export_write");
        let a = t.annotations.as_ref().expect("annotations");
        assert_eq!(a.read_only_hint, Some(false));
        assert_eq!(a.destructive_hint, Some(true));
        assert_eq!(a.idempotent_hint, Some(true));
        assert_eq!(a.open_world_hint, Some(false));
        assert!(t.output_schema.is_some());

        let schema = serde_json::to_value(&*t.input_schema).unwrap();
        let mut props: Vec<&str> = schema["properties"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        props.sort();
        assert_eq!(
            props,
            [
                "action",
                "dest",
                "main_rel",
                "profile",
                "root_id",
                "size_cap_bytes"
            ]
        );
        assert_eq!(schema["additionalProperties"], false);
    }

    #[test]
    fn export_write_cannot_be_given_an_approval_and_fails_closed() {
        let base = serde_json::json!({
            "root_id": "r", "action": "bundle", "main_rel": "main.tex", "dest": "/tmp/x", "profile": "folder"
        });
        assert!(serde_json::from_value::<ExportWriteParams>(base.clone()).is_ok());
        for extra in [
            "approved_fetch",
            "approve_fetch",
            "approve",
            "fetch",
            "network",
        ] {
            let mut v = base.clone();
            v[extra] = serde_json::json!(["https://media.example.org/clip.mp4"]);
            assert!(
                serde_json::from_value::<ExportWriteParams>(v).is_err(),
                "{extra} must not be accepted"
            );
        }
        let m = Maleficium::default();
        let err = m
            .export_write(Parameters(
                serde_json::from_value::<ExportWriteParams>(base).unwrap(),
            ))
            .err()
            .expect("an ungranted root fails");
        assert!(!err.is_empty());
    }

    /// Grouped `export_read` validates per action: `stamp` and `status` need
    /// `main_rel`, `cancel` takes nothing else, and an unknown action names
    /// the valid list with a did-you-mean hint. Nothing is written either
    /// way: the stamp of a never-compiled project is null, and cancelling
    /// with no conversion running is an error, not silence.
    #[test]
    fn export_read_validates_per_action() {
        let m = Maleficium::default();
        let dir = core::test_scratch::dir("mcp-export-read");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("main.tex"), "x").unwrap();
        let root = dunce::canonicalize(&dir).unwrap();
        core::grant_root(&m.cx, "expr", &root.to_string_lossy()).unwrap();

        let read = |action: ExportReadAction, main_rel: Option<&str>| {
            m.export_read(Parameters(ExportReadParams {
                root_id: "expr".into(),
                action,
                main_rel: main_rel.map(Into::into),
            }))
        };
        let stamp = read(ExportReadAction::Stamp, Some("main.tex"))
            .expect("a stamp reads")
            .0;
        let ExportReadOut::Stamp(stamp) = stamp else {
            panic!("stamp returns its stamp");
        };
        assert!(stamp.stamp.is_none());
        let status = read(ExportReadAction::Status, Some("main.tex"))
            .expect("a status reads")
            .0;
        let ExportReadOut::Status(status) = status else {
            panic!("status returns whether a pdf waits");
        };
        assert!(!status.fresh);

        let err = read(ExportReadAction::Stamp, None)
            .err()
            .expect("stamp without a main file fails");
        assert!(err.contains("main_rel"), "{err}");
        let err = read(ExportReadAction::Status, None)
            .err()
            .expect("status without a main file fails");
        assert!(err.contains("main_rel"), "{err}");
        let err = read(ExportReadAction::Cancel, Some("main.tex"))
            .err()
            .expect("cancel rejects a main file");
        assert!(err.contains("takes no `main_rel`"), "{err}");
        let err = m
            .export_read(Parameters(ExportReadParams {
                root_id: "expr".into(),
                action: ExportReadAction::Cancel,
                main_rel: None,
            }))
            .err()
            .expect("cancelling with no conversion running fails");
        assert!(err.contains("no conversion is running"), "{err}");
        assert!(
            m.export_read(Parameters(ExportReadParams {
                root_id: "nope".into(),
                action: ExportReadAction::Stamp,
                main_rel: Some("main.tex".into()),
            }))
            .is_err()
        );

        let err = serde_json::from_value::<ExportReadParams>(serde_json::json!({
            "root_id": "r", "action": "stomp", "main_rel": "m.tex",
        }))
        .expect_err("an unknown action fails");
        assert!(err.to_string().contains("[stamp, status, cancel]"), "{err}");
        let err = serde_json::from_value::<ExportReadParams>(serde_json::json!({
            "root_id": "r", "action": "stats", "main_rel": "m.tex",
        }))
        .expect_err("a near-miss action fails");
        assert!(err.to_string().contains("did you mean `status`?"), "{err}");
    }

    /// Grouped `export_write` validates per action: each action names the
    /// fields it needs and rejects the rest, before touching the project.
    /// The destination check lives in one place (the core's): every action
    /// that takes a `dest` refuses a path inside the project.
    #[test]
    fn export_write_validates_per_action_and_refuses_inside_project_dest() {
        let m = Maleficium::default();
        let dir = core::test_scratch::dir("mcp-export-write");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("main.tex"), "x").unwrap();
        let root = dunce::canonicalize(&dir).unwrap();
        core::grant_root(&m.cx, "expw", &root.to_string_lossy()).unwrap();
        let inside = root.join("inside.zip").to_string_lossy().to_string();

        let write = |params: ExportWriteParams| m.export_write(Parameters(params));
        let base = |action: ExportWriteAction| ExportWriteParams {
            root_id: "expw".into(),
            action,
            main_rel: None,
            dest: None,
            profile: None,
            size_cap_bytes: None,
        };
        let mut pdf = base(ExportWriteAction::Pdf);
        let err = write(pdf.clone()).err().expect("pdf without fields fails");
        assert!(err.contains("main_rel"), "{err}");
        pdf.main_rel = Some("main.tex".into());
        let err = write(pdf.clone())
            .err()
            .expect("pdf without a destination fails");
        assert!(err.contains("dest"), "{err}");
        pdf.dest = Some("/tmp/maleficium-expw-paper.pdf".into());
        pdf.profile = Some(maleficium_events::BundleProfile::Folder);
        let err = write(pdf).err().expect("pdf rejects a profile");
        assert!(
            err.contains("pdf takes only `main_rel` and `dest`"),
            "{err}"
        );

        let mut zip = base(ExportWriteAction::Zip);
        let err = write(zip.clone())
            .err()
            .expect("zip without a destination fails");
        assert!(err.contains("dest"), "{err}");
        zip.dest = Some(inside.clone());
        zip.main_rel = Some("main.tex".into());
        let err = write(zip.clone()).err().expect("zip rejects a main file");
        assert!(err.contains("zip takes only `dest`"), "{err}");
        zip.main_rel = None;
        let err = write(zip).err().expect("an inside-project dest fails");
        assert!(err.contains("inside the project"), "{err}");

        let mut bundle = base(ExportWriteAction::Bundle);
        bundle.main_rel = Some("main.tex".into());
        bundle.dest = Some(inside.clone());
        bundle.profile = Some(maleficium_events::BundleProfile::Folder);
        let err = write(bundle)
            .err()
            .expect("an inside-project bundle dest fails");
        assert!(err.contains("inside the project"), "{err}");

        let mut bundle = base(ExportWriteAction::Bundle);
        bundle.main_rel = Some("main.tex".into());
        bundle.dest = Some("/tmp/maleficium-expw-bundle".into());
        let err = write(bundle).err().expect("bundle without a profile fails");
        assert!(err.contains("profile"), "{err}");

        let mut preview = base(ExportWriteAction::Preview);
        let err = write(preview.clone())
            .err()
            .expect("preview without a main file fails");
        assert!(err.contains("main_rel"), "{err}");
        preview.main_rel = Some("main.tex".into());
        preview.dest = Some("/tmp/maleficium-expw-preview.html".into());
        let err = write(preview).err().expect("preview rejects a dest");
        assert!(err.contains("preview takes only `main_rel`"), "{err}");

        let mut open = base(ExportWriteAction::Open);
        let err = write(open.clone())
            .err()
            .expect("open without a main file fails");
        assert!(err.contains("main_rel"), "{err}");
        open.main_rel = Some("main.tex".into());
        open.size_cap_bytes = Some(1);
        let err = write(open).err().expect("open rejects a size cap");
        assert!(err.contains("open takes only `main_rel`"), "{err}");

        let err = serde_json::from_value::<ExportWriteParams>(serde_json::json!({
            "root_id": "r", "action": "bundel", "main_rel": "m.tex",
            "dest": "/tmp/x", "profile": "folder",
        }))
        .expect_err("a near-miss action fails");
        assert!(err.to_string().contains("did you mean `bundle`?"), "{err}");
    }

    /// Grouped `runtime_read` stays on its side of the approval boundary:
    /// the scaffold writes only to the user library, the validator only
    /// reads, and the install (a separate, trust-gated tool) only copies a
    /// validated draft into a granted project. None takes a parameter that
    /// approves or allows, and an unknown action names the valid list with
    /// a did-you-mean hint.
    #[test]
    fn runtime_read_stays_library_scoped_and_cannot_approve() {
        let tools = Maleficium::tool_router().list_all();
        let t = tools
            .iter()
            .find(|t| t.name == "runtime_read")
            .expect("runtime_read");
        let a = t.annotations.as_ref().expect("annotations");
        assert_eq!(a.read_only_hint, Some(false));
        assert_eq!(a.destructive_hint, Some(false));
        assert_eq!(a.idempotent_hint, Some(false));
        assert_eq!(a.open_world_hint, Some(false));
        assert!(t.output_schema.is_some());
        let schema = serde_json::to_value(&*t.input_schema).unwrap();
        let mut props: Vec<&str> = schema["properties"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        props.sort();
        assert_eq!(props, ["action", "from", "name", "reference", "root_id"]);
        assert_eq!(schema["additionalProperties"], false);

        let t = tools
            .iter()
            .find(|t| t.name == "runtime_install")
            .expect("runtime_install");
        let a = t.annotations.as_ref().expect("annotations");
        assert_eq!(a.read_only_hint, Some(false));
        assert_eq!(a.destructive_hint, Some(true), "replace swaps a copy out");
        assert_eq!(a.idempotent_hint, Some(true));
        assert_eq!(a.open_world_hint, Some(false));
        let schema = serde_json::to_value(&*t.input_schema).unwrap();
        let mut props: Vec<&str> = schema["properties"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        props.sort();
        assert_eq!(props, ["reference", "replace", "root_id"]);
        assert_eq!(schema["additionalProperties"], false);

        assert!(
            serde_json::from_value::<RuntimeReadParams>(
                serde_json::json!({"action": "scaffold", "name": "orbit-view", "from": "model@1"})
            )
            .is_ok()
        );
        assert!(
            serde_json::from_value::<RuntimeReadParams>(
                serde_json::json!({"action": "validate", "reference": "orbit-view@1"})
            )
            .is_ok()
        );
        for extra in ["approve", "allow", "decide", "digest", "revoke", "mode"] {
            let mut v = serde_json::json!({"action": "scaffold", "name": "orbit-view"});
            v[extra] = serde_json::json!(true);
            assert!(
                serde_json::from_value::<RuntimeReadParams>(v).is_err(),
                "{extra}"
            );
            let mut v = serde_json::json!({"action": "validate", "reference": "orbit-view@1"});
            v[extra] = serde_json::json!(true);
            assert!(
                serde_json::from_value::<RuntimeReadParams>(v).is_err(),
                "{extra}"
            );
            let mut v = serde_json::json!({"root_id": "r", "reference": "orbit-view@1"});
            v[extra] = serde_json::json!(true);
            assert!(
                serde_json::from_value::<RuntimeInstallParams>(v).is_err(),
                "{extra}"
            );
        }
        let m = Maleficium::default();
        assert!(
            m.runtime_read(Parameters(
                serde_json::from_value(serde_json::json!({"action": "scaffold", "name": "nope!"}))
                    .unwrap()
            ))
            .is_err()
        );
        // An ungranted project: nothing to install into.
        assert!(
            m.runtime_install(Parameters(
                serde_json::from_value(
                    serde_json::json!({"root_id": "nope", "reference": "orbit-view@1"})
                )
                .unwrap()
            ))
            .is_err()
        );

        let err = serde_json::from_value::<RuntimeReadParams>(serde_json::json!({
            "action": "validat", "reference": "orbit-view@1",
        }))
        .expect_err("a near-miss action fails");
        assert!(
            err.to_string().contains("did you mean `validate`?"),
            "{err}"
        );

        // Per-action extras die in the handler, on an ungranted root or the
        // library: nothing is touched to learn the fields do not belong.
        let err = m
            .runtime_read(Parameters(RuntimeReadParams {
                action: RuntimeReadAction::Scaffold,
                name: None,
                from: None,
                reference: Some("orbit-view@1".into()),
                root_id: None,
            }))
            .err()
            .expect("scaffold rejects a reference");
        assert!(err.contains("scaffold takes only"), "{err}");
        let err = m
            .runtime_read(Parameters(RuntimeReadParams {
                action: RuntimeReadAction::Validate,
                name: Some("orbit-view".into()),
                from: None,
                reference: Some("orbit-view@1".into()),
                root_id: None,
            }))
            .err()
            .expect("validate rejects a name");
        assert!(err.contains("validate takes only"), "{err}");
    }

    /// The presence tool reads windows but can prompt the user, so it is
    /// honestly non-idempotent and non-read-only. Its parameters refuse
    /// unknown fields, `windows` takes nothing, and `request_open` names a
    /// folder, a file, a window and a wait. (Not called here: a live app on
    /// this machine would show the prompt.)
    #[test]
    fn presence_windows_reads_and_request_open_only_asks() {
        let tools = Maleficium::tool_router().list_all();
        let t = tools
            .iter()
            .find(|t| t.name == "presence")
            .expect("presence");
        let a = t.annotations.as_ref().expect("annotations");
        assert_eq!(a.read_only_hint, Some(false));
        assert_eq!(a.destructive_hint, Some(false));
        assert_eq!(a.idempotent_hint, Some(false));
        assert_eq!(a.open_world_hint, Some(false));
        assert!(t.output_schema.is_some());
        let schema = serde_json::to_value(&*t.input_schema).unwrap();
        let mut props: Vec<&str> = schema["properties"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        props.sort();
        assert_eq!(props, ["action", "file", "pid", "project", "wait_ms"]);
        assert_eq!(schema["additionalProperties"], false);
        assert!(
            serde_json::from_value::<PresenceParams>(
                serde_json::json!({"action": "windows", "open": true})
            )
            .is_err()
        );
        for extra in ["force", "focus", "skip_prompt", "open"] {
            let mut v = serde_json::json!({"action": "request_open", "project": "/p"});
            v[extra] = serde_json::json!(true);
            assert!(
                serde_json::from_value::<PresenceParams>(v).is_err(),
                "{extra}"
            );
        }
        let out = Maleficium::default()
            .presence(Parameters(PresenceParams {
                action: PresenceAction::Windows,
                project: None,
                file: None,
                pid: None,
                wait_ms: None,
            }))
            .unwrap();
        let PresenceOut::Windows(out) = out.0 else {
            panic!("windows returns its windows");
        };
        assert!(!out.hint.is_empty());

        let m = Maleficium::default();
        let err = m
            .presence(Parameters(PresenceParams {
                action: PresenceAction::Windows,
                project: Some("/p".into()),
                file: None,
                pid: None,
                wait_ms: None,
            }))
            .err()
            .expect("windows rejects a project");
        assert!(err.contains("takes no `project`"), "{err}");
        let err = m
            .presence(Parameters(PresenceParams {
                action: PresenceAction::RequestOpen,
                project: None,
                file: None,
                pid: None,
                wait_ms: None,
            }))
            .err()
            .expect("request_open without a project fails");
        assert!(err.contains("project"), "{err}");

        let err = serde_json::from_value::<PresenceParams>(serde_json::json!({
            "action": "windowz",
        }))
        .expect_err("a near-miss action fails");
        assert!(err.to_string().contains("did you mean `windows`?"), "{err}");
    }

    /// The approval boundary: no tool approves, revokes or switches
    /// auto-approval. No tool is named for it, no tool takes a parameter
    /// for it (and the approval tools refuse unknown fields), and this
    /// crate never calls the core's user-action functions.
    #[test]
    fn no_tool_can_approve_revoke_or_change_auto_approval() {
        let tools = Maleficium::tool_router().list_all();
        let banned = ["approv", "revok", "auto", "mode", "trust", "allow"];
        for t in &tools {
            let name = t.name.to_lowercase();
            assert!(!banned.iter().any(|b| name.contains(b)), "tool {name}");
            let schema = serde_json::to_value(&*t.input_schema).unwrap();
            let props = schema["properties"]
                .as_object()
                .cloned()
                .unwrap_or_default();
            for key in props.keys() {
                let k = key.to_lowercase();
                assert!(
                    !banned.iter().any(|b| k.contains(b)),
                    "tool {name} takes `{key}`"
                );
            }
        }
        let t = tools.iter().find(|t| t.name == "widget").expect("widget");
        let a = t.annotations.as_ref().expect("annotations");
        assert_eq!(a.read_only_hint, Some(true), "widget");
        assert_eq!(a.open_world_hint, Some(false), "widget");
        let schema = serde_json::to_value(&*t.input_schema).unwrap();
        assert_eq!(schema["additionalProperties"], false, "widget");
        // Every output schema is an object, so `opencode mcp list` accepts the server.
        for t in &tools {
            if let Some(out) = t.output_schema.as_ref() {
                let schema = serde_json::to_value(&**out).unwrap();
                assert_eq!(schema["type"], "object", "tool {}", t.name);
            }
        }
        let base = serde_json::json!({"root_id": "r", "action": "check", "main_rel": "main.tex", "widget": "w"});
        assert!(serde_json::from_value::<WidgetParams>(base.clone()).is_ok());
        for extra in [
            "approve",
            "approved",
            "auto_approve",
            "digest",
            "revoke",
            "mode",
        ] {
            let mut v = base.clone();
            v[extra] = serde_json::json!(true);
            assert!(
                serde_json::from_value::<WidgetParams>(v).is_err(),
                "{extra}"
            );
            let mut v =
                serde_json::json!({"root_id": "r", "action": "status", "main_rel": "main.tex"});
            v[extra] = serde_json::json!(true);
            assert!(
                serde_json::from_value::<WidgetParams>(v).is_err(),
                "{extra}"
            );
        }
        let src = include_str!("lib.rs");
        for f in [
            "approve",
            "approve_at",
            "revoke",
            "revoke_at",
            "set_auto_approve",
            "set_auto_at",
            "decide_runtime",
            "decide_runtime_at",
            "review_runtime",
            "review_runtime_at",
        ] {
            let call = format!("widget_approval::{f}(");
            assert!(!src.contains(&call), "the MCP crate calls {call}");
        }
    }

    /// A widget nobody approved is a normal result naming what the user
    /// must do, never a tool error; a first-party one is an error.
    #[test]
    fn widget_check_action_returns_approval_required_as_a_normal_result() {
        let m = Maleficium::default();
        let dir = core::test_scratch::dir("mcp-approval");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("widgets/demo")).unwrap();
        std::fs::write(dir.join("main.tex"), "x").unwrap();
        std::fs::write(dir.join("widgets/demo/index.html"), "<p>demo</p>").unwrap();
        let root = dunce::canonicalize(&dir).unwrap();
        core::grant_root(&m.cx, "ap", &root.to_string_lossy()).unwrap();
        let o = core::outputs::outputs_of(&m.cx, "ap", "main.tex").unwrap();
        std::fs::create_dir_all(&o.outdir).unwrap();
        std::fs::write(
            o.outdir.join(&o.pdf_name),
            include_bytes!("../../core/testdata/interactive/main.pdf"),
        )
        .unwrap();
        std::fs::write(
            o.outdir.join("main.mfw"),
            include_str!("../../core/testdata/interactive/main.mfw"),
        )
        .unwrap();
        let p = |widget: &str| {
            Parameters(WidgetParams {
                root_id: "ap".into(),
                action: WidgetAction::Check,
                main_rel: "main.tex".into(),
                widget: Some(widget.into()),
            })
        };
        let r = m.widget(p("fig-demo")).expect("a normal result").0;
        let WidgetOut::Checked(r) = r else {
            panic!("check returns whether the widget may run");
        };
        let v = serde_json::to_value(&r).unwrap();
        assert_eq!(v["status"], "approval_required", "{v}");
        assert_eq!(v["widget"], "fig-demo");
        assert_eq!(v["path"], "widgets/demo");
        assert_eq!(v["cause"], "never_approved");
        assert_eq!(v["digest"].as_str().unwrap().len(), 64);
        assert!(v["userAction"].as_str().unwrap().contains("View > Widgets"));
        assert!(m.widget(p("fig-mesh")).is_err());
        assert!(m.widget(p("nope")).is_err());
        let s = m
            .widget(Parameters(WidgetParams {
                root_id: "ap".into(),
                action: WidgetAction::Status,
                main_rel: "main.tex".into(),
                widget: None,
            }))
            .expect("status")
            .0;
        let WidgetOut::Status(s) = s else {
            panic!("status returns the approval state");
        };
        assert_eq!(s.pending, 1);
        assert_eq!(s.exempt.len(), 4);
        let _ = std::fs::remove_dir_all(&o.outdir);
    }

    /// Every embedded View is a whole html page that loads nothing from
    /// the network (the default Apps CSP would block it anyway).
    #[test]
    fn every_view_is_a_self_contained_page() {
        assert_eq!(VIEWS.len(), 2);
        for (uri, html) in VIEWS {
            let html = html.trim_start().to_ascii_lowercase();
            assert!(html.starts_with("<!doctype html>"), "{uri}");
            assert!(
                html.len() > 10_000,
                "{uri}: the built View is missing or a stub"
            );
            for external in ["src=\"http", "href=\"http", "@import", "<link "] {
                assert!(!html.contains(external), "{uri}: {external}");
            }
        }
    }

    /// Each View is served at its versioned URI as an MCP App page with a
    /// border and no csp; anything else under ui:// is not found.
    #[test]
    fn views_are_served_at_their_uris_and_nothing_else() {
        for (uri, html) in VIEWS {
            let served = serde_json::to_value(view_resource(uri).unwrap()).unwrap();
            assert_eq!(served["uri"], *uri);
            assert_eq!(served["mimeType"], UI_MIME);
            assert_eq!(served["text"], *html);
            assert_eq!(
                served["_meta"],
                serde_json::json!({ "ui": { "prefersBorder": true } })
            );
        }
        assert!(COMPILE_VIEW_URI.ends_with("/v1"));
        for unknown in [
            "ui://maleficium/compile/v2",
            "ui://maleficium/compile",
            "ui://maleficium/nope/v1",
            "file:///etc/passwd",
        ] {
            assert!(view_resource(unknown).is_err(), "{unknown}");
        }
    }

    /// The dashboard's poll reports the phase and downloads it renders.
    #[test]
    fn progress_follows_phase_and_fetch_signals() {
        use maleficium_structure::{CompilePhase, line_signal};
        let mut p = core::Progress::default();
        for line in [
            "note: downloading article.cls",
            "note: downloading size10.clo",
            "warning: failed to download \"x.sty\"; please check your network connection",
            "note: Running TeX ...",
            "plain text with no signal",
        ] {
            p.note(line_signal(line).as_ref());
        }
        assert_eq!(p.phase, Some(CompilePhase::Tex));
        assert_eq!((p.fetched, p.fetch_failed), (2, 1));
        let json = serde_json::to_value(&p).unwrap();
        assert_eq!(json["phase"], "tex");
        assert!(json.get("detail").is_none(), "{json}");
    }

    #[test]
    fn sync_view_takes_exactly_one_target() {
        let p = |tex_rel: Option<&str>, line, label: Option<&str>, page| SyncViewParams {
            root_id: "r".into(),
            action: SyncViewAction::Snippet,
            main_rel: "main.tex".into(),
            tex_rel: tex_rel.map(Into::into),
            line,
            label: label.map(Into::into),
            page,
            x: None,
            y: None,
            with_image: None,
        };
        assert!(p(Some("a.tex"), Some(3), None, None).target().is_ok());
        assert!(p(None, None, Some("fig:x"), None).target().is_ok());
        assert!(p(None, None, None, Some(2)).target().is_ok());
        for bad in [
            p(None, None, None, None),
            p(Some("a.tex"), None, None, None),
            p(None, Some(3), None, None),
            p(Some("a.tex"), Some(3), Some("fig:x"), None),
            p(None, None, Some("fig:x"), Some(1)),
        ] {
            assert!(bad.target().is_err(), "{bad:?}");
        }
    }

    #[test]
    fn synctex_adapters_reject_identically() {
        let m = Maleficium::default();
        let cx = &m.cx;
        let dir = core::test_scratch::dir("sym");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("main.tex"), "x").unwrap();
        // Creating symlinks on Windows needs a privilege; there link.tex is
        // simply missing, which must be refused the same way.
        #[cfg(unix)]
        std::os::unix::fs::symlink("/etc/hostname", dir.join("link.tex")).unwrap();
        let canon = dunce::canonicalize(&dir).unwrap();
        core::grant_root(cx, "sym", &canon.to_string_lossy()).unwrap();
        // A compiled output, so tex_rel validation is reached.
        let out = core::main_outputs(&canon.join("main.tex")).unwrap();
        std::fs::create_dir_all(&out.outdir).unwrap();
        std::fs::write(out.outdir.join(&out.pdf_name), "%PDF").unwrap();
        let cases: &[(&str, &str, &str)] = &[
            ("sym", "../x.tex", "main.tex"),
            ("sym", "/etc/hostname", "main.tex"),
            ("sym", "a\0b.tex", "main.tex"),
            ("sym", "", "main.tex"),
            ("sym", "link.tex", "main.tex"),
            ("sym", "main.tex", "../x.tex"),
            ("sym", "main.tex", "link.tex"),
            ("sym", "main.tex", "missing.tex"),
            ("nope", "main.tex", "main.tex"),
            ("bad id!", "main.tex", "main.tex"),
        ];
        for (root_id, main_rel, tex_rel) in cases {
            let main_rel = main_rel.to_string();
            let desktop = core::forward(cx, root_id, &main_rel, tex_rel, 1);
            let base = SyncViewParams {
                root_id: root_id.to_string(),
                action: SyncViewAction::Forward,
                main_rel: main_rel.clone(),
                tex_rel: Some(tex_rel.to_string()),
                line: Some(1),
                label: None,
                page: None,
                x: None,
                y: None,
                with_image: None,
            };
            let mcp = m.sync_view(Parameters(base));
            let err = desktop
                .err()
                .unwrap_or_else(|| panic!("accepted: {:?}", (root_id, &main_rel)));
            assert_eq!(mcp.err(), Some(err));

            let desktop = core::inverse(cx, root_id, &main_rel, 1, 0.0, 0.0);
            let mcp = m.sync_view(Parameters(SyncViewParams {
                root_id: root_id.to_string(),
                action: SyncViewAction::Inverse,
                main_rel: main_rel.clone(),
                tex_rel: None,
                line: None,
                label: None,
                page: Some(1),
                x: None,
                y: None,
                with_image: None,
            }));
            assert_eq!(mcp.as_ref().err(), desktop.as_ref().err());
            assert_eq!(
                mcp.ok().map(|r| r.structured_content),
                desktop.ok().map(|d| serde_json::to_value(&d).ok())
            );
        }
        let _ = std::fs::remove_dir_all(&out.outdir);
    }

    /// m8ven Tool annotations: every registered tool carries all four hints
    /// explicitly, so a new tool without them fails here instead of in a rescan.
    #[test]
    fn every_tool_carries_all_four_hints() {
        for t in Maleficium::tool_router().list_all() {
            let a = t
                .annotations
                .as_ref()
                .unwrap_or_else(|| panic!("tool `{}` lacks annotations", t.name));
            assert!(
                a.read_only_hint.is_some(),
                "tool `{}` lacks read_only_hint",
                t.name
            );
            assert!(
                a.destructive_hint.is_some(),
                "tool `{}` lacks destructive_hint",
                t.name
            );
            assert!(
                a.idempotent_hint.is_some(),
                "tool `{}` lacks idempotent_hint",
                t.name
            );
            assert!(
                a.open_world_hint.is_some(),
                "tool `{}` lacks open_world_hint",
                t.name
            );
        }
    }

    /// m8ven Test coverage: every registered tool has a case below calling
    /// its named handler. The router enumeration fails on any tool without one.
    #[test]
    fn every_registered_tool_has_a_case_and_runs() {
        let m = Maleficium::default();
        let cx = &m.cx;
        let dir = core::test_scratch::dir("cov");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("main.tex"),
            "\\section{Hi}\\label{sec:hi}\nSee~\\ref{sec:hi}.\n",
        )
        .unwrap();
        let canon = dunce::canonicalize(&dir).unwrap();
        let root = canon.to_string_lossy().to_string();
        core::grant_root(cx, "cov", &root).unwrap();

        let info = m
            .session(Parameters(SessionParams {
                root_id: "cov".into(),
                action: SessionAction::Info,
                root: None,
            }))
            .unwrap();
        assert_eq!(info.0.path, root);

        let grant = m
            .session(Parameters(SessionParams {
                root_id: "cov2".into(),
                action: SessionAction::Grant,
                root: Some(root.clone()),
            }))
            .unwrap();
        assert_eq!(grant.0.path, root);

        let list = m
            .file(Parameters(FileParams {
                root_id: "cov".into(),
                action: FileAction::List,
                rel: None,
            }))
            .unwrap();
        let FileOut::Listing(list) = list.0 else {
            panic!("list returns its entries");
        };
        assert!(list.entries.iter().any(|e| e.name == "main.tex"));

        let read = m
            .file(Parameters(FileParams {
                root_id: "cov".into(),
                action: FileAction::Read,
                rel: Some("main.tex".into()),
            }))
            .unwrap();
        let FileOut::Text(read) = read.0 else {
            panic!("read returns its text");
        };
        assert!(read.text.contains("\\section{Hi}"));

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        // One cluster for the whole compile loop: `start` resolves the main
        // file and returns a job id, `poll`/`cancel` name the job, and the
        // readings name the project and main file. A bad root, an unknown
        // job and a never-compiled project all fail closed.
        let paper = |p: PaperParams| rt.block_on(m.paper(Parameters(p)));
        let base = |action: PaperAction| PaperParams {
            action,
            root_id: None,
            job_id: None,
            main_rel: None,
            networked: None,
            tail_lines: None,
            wait_ms: None,
            max: None,
        };
        let mut start = base(PaperAction::Start);
        start.root_id = Some("nope".into());
        start.main_rel = Some("main.tex".into());
        assert!(paper(start).is_err());

        let mut poll = base(PaperAction::Poll);
        poll.job_id = Some("nope".into());
        poll.wait_ms = Some(0);
        assert!(paper(poll).is_err());

        let mut cancel = base(PaperAction::Cancel);
        cancel.job_id = Some("nope".into());
        assert_eq!(paper(cancel).err(), Some("unknown job: nope".to_string()));

        let mut readiness = base(PaperAction::Readiness);
        readiness.root_id = Some("cov".into());
        assert!(paper(readiness).is_ok());

        let mut output = base(PaperAction::Output);
        output.root_id = Some("cov".into());
        output.main_rel = Some("main.tex".into());
        let stamped = paper(output).unwrap().0;
        let PaperOut::Stamped(stamped) = stamped else {
            panic!("output returns its stamp");
        };
        assert!(stamped.stamp.is_none());

        let mut prechecks = base(PaperAction::Prechecks);
        prechecks.root_id = Some("cov".into());
        prechecks.main_rel = Some("main.tex".into());
        assert!(paper(prechecks).is_ok());

        let mut diagnostics = base(PaperAction::Diagnostics);
        diagnostics.root_id = Some("cov".into());
        diagnostics.main_rel = Some("main.tex".into());
        assert!(paper(diagnostics).is_err());

        let mut stray = base(PaperAction::Start);
        stray.job_id = Some("job-1".into());
        let err = paper(stray).err().expect("start rejects a job id");
        assert!(err.contains("takes no `job_id`"), "{err}");

        let pdf_dest = dir.join("out.pdf").to_string_lossy().to_string();
        assert!(
            m.export_write(Parameters(ExportWriteParams {
                root_id: "cov".into(),
                action: ExportWriteAction::Pdf,
                main_rel: Some("main.tex".into()),
                dest: Some(pdf_dest),
                profile: None,
                size_cap_bytes: None,
            }))
            .is_err()
        );

        let zip_parent = core::test_scratch::dir("cov-zip");
        std::fs::create_dir_all(&zip_parent).unwrap();
        let zip_dest = zip_parent.join("sources.zip").to_string_lossy().to_string();
        let zip = m
            .export_write(Parameters(ExportWriteParams {
                root_id: "cov".into(),
                action: ExportWriteAction::Zip,
                main_rel: None,
                dest: Some(zip_dest.clone()),
                profile: None,
                size_cap_bytes: None,
            }))
            .unwrap();
        let ExportWriteOut::Exported(zip) = zip.0 else {
            panic!("zip returns what it wrote");
        };
        assert!(zip.files.contains(&"main.tex".to_string()));
        assert!(std::path::Path::new(&zip_dest).is_file());

        let stamp = m
            .export_read(Parameters(ExportReadParams {
                root_id: "cov".into(),
                action: ExportReadAction::Stamp,
                main_rel: Some("main.tex".into()),
            }))
            .unwrap();
        let ExportReadOut::Stamp(stamp) = stamp.0 else {
            panic!("stamp returns its stamp");
        };
        assert!(stamp.stamp.is_none());
        let status = m
            .export_read(Parameters(ExportReadParams {
                root_id: "cov".into(),
                action: ExportReadAction::Status,
                main_rel: Some("main.tex".into()),
            }))
            .unwrap();
        assert!(matches!(status.0, ExportReadOut::Status(ref s) if !s.fresh));
        assert!(
            m.export_read(Parameters(ExportReadParams {
                root_id: "cov".into(),
                action: ExportReadAction::Cancel,
                main_rel: None,
            }))
            .is_err()
        );

        let listed = m
            .scaffold(Parameters(ScaffoldParams {
                action: ScaffoldAction::List,
                template: None,
                parent_dir: None,
                name: None,
            }))
            .unwrap();
        let ScaffoldOut::List(listed) = listed.0 else {
            panic!("list returns its templates");
        };
        assert!(!listed.templates.is_empty());

        let tpl_parent = core::test_scratch::dir("cov-tpl");
        std::fs::create_dir_all(&tpl_parent).unwrap();
        let created = m
            .scaffold(Parameters(ScaffoldParams {
                action: ScaffoldAction::Create,
                template: Some("welcome".into()),
                parent_dir: Some(tpl_parent.to_string_lossy().to_string()),
                name: Some("proj1".into()),
            }))
            .unwrap();
        let ScaffoldOut::Created(created) = created.0 else {
            panic!("create returns its project");
        };
        assert!(
            std::path::Path::new(&created.root)
                .join(&created.main)
                .is_file()
        );

        // The sync_view actions fail like the core's on a never-compiled project.
        let view = |action: SyncViewAction| {
            let mut p = SyncViewParams {
                root_id: "cov".into(),
                action,
                main_rel: "main.tex".into(),
                tex_rel: None,
                line: None,
                label: None,
                page: None,
                x: None,
                y: None,
                with_image: None,
            };
            match action {
                SyncViewAction::Forward => {
                    p.tex_rel = Some("main.tex".into());
                    p.line = Some(1);
                }
                SyncViewAction::Inverse | SyncViewAction::Snippet => {
                    p.page = Some(1);
                }
            }
            m.sync_view(Parameters(p))
        };
        // Never compiled: every view action fails like the core's.
        assert!(view(SyncViewAction::Forward).is_err());
        assert!(view(SyncViewAction::Inverse).is_err());
        assert!(view(SyncViewAction::Snippet).is_err());

        assert!(
            m.sync_render(Parameters(SyncRenderParams {
                root_id: "cov".into(),
                action: SyncRenderAction::Render,
                main_rel: "main.tex".into(),
                page: 1,
                region: None,
                scale: None,
            }))
            .is_err()
        );

        std::fs::write(dir.join("victim.txt"), "bye").unwrap();
        let unconfirmed = m.file_mutate(Parameters(FileMutateParams {
            root_id: "cov".into(),
            action: FileMutateAction::Delete,
            rel: Some("victim.txt".into()),
            confirm: None,
            trash_path: None,
        }));
        assert!(unconfirmed.err().unwrap().contains("victim.txt"));
        let abs = canon.join("victim.txt").to_string_lossy().to_string();
        let trashed = m
            .file_mutate(Parameters(FileMutateParams {
                root_id: "cov".into(),
                action: FileMutateAction::Delete,
                rel: Some("victim.txt".into()),
                confirm: Some(abs),
                trash_path: None,
            }))
            .unwrap();
        let FileMutateOut::Deleted(trashed) = trashed.0 else {
            panic!("delete returns its trash path");
        };
        assert!(!canon.join("victim.txt").exists());

        let restored = m
            .file_mutate(Parameters(FileMutateParams {
                root_id: "cov".into(),
                action: FileMutateAction::Undo,
                rel: None,
                confirm: None,
                trash_path: Some(trashed.trash_path),
            }))
            .unwrap();
        let FileMutateOut::Restored(restored) = restored.0 else {
            panic!("undo returns its restored path");
        };
        assert!(restored.path.ends_with("victim.txt"));
        assert_eq!(
            std::fs::read_to_string(canon.join("victim.txt")).unwrap(),
            "bye"
        );

        // The engine log tail reads through `paper log` with the same core
        // call: never compiled, the shard is missing and it fails closed.
        let mut log = base(PaperAction::Log);
        log.root_id = Some("cov".into());
        log.main_rel = Some("main.tex".into());
        assert!(paper(log).is_err());

        let structure = |action: StructureAction| {
            let mut p = StructureParams {
                root_id: "cov".into(),
                action,
                rel: None,
                main_rel: None,
                kind: None,
                key: None,
                line: None,
                col: None,
            };
            match action {
                StructureAction::Outline => p.rel = Some("main.tex".into()),
                StructureAction::Definition => {
                    p.kind = Some(maleficium_index::definition::RefKind::Label);
                    p.key = Some("sec:hi".into());
                }
                _ => p.main_rel = Some("main.tex".into()),
            }
            m.structure(Parameters(p))
        };
        assert!(structure(StructureAction::Outline).is_ok());
        assert!(structure(StructureAction::FileGraph).is_ok());
        assert!(structure(StructureAction::LabelsRefs).is_ok());
        assert!(structure(StructureAction::Citations).is_ok());
        m.cx.set_converter(std::sync::Arc::new(FixedHtml(CONVERTED.to_string())));
        let doc = structure(StructureAction::DocumentStructure).unwrap();
        let StructureOut::Document(doc) = doc.0 else {
            panic!("document_structure returns its document");
        };
        assert_eq!(doc.sections.len(), 2);
        assert_eq!(doc.widgets.len(), 5);
        let defined = structure(StructureAction::Definition).unwrap();
        assert!(matches!(defined.0, StructureOut::Definition(ref d) if d.lookup.is_some()));

        let search = m
            .search(Parameters(SearchParams {
                root_id: "cov".into(),
                action: SearchAction::Search,
                pattern: Some("section".into()),
                query: None,
                regex: None,
                case_sensitive: None,
                whole_word: None,
                main_rel: None,
                max: None,
            }))
            .unwrap();
        assert!(matches!(search.0, SearchOut::Hits(ref r) if r.hits >= 1));

        let preview = m
            .replace(Parameters(ReplaceParams {
                root_id: "cov".into(),
                action: ReplaceAction::Preview,
                pattern: Some("Hi".into()),
                replacement: Some("Hello".into()),
                regex: None,
                case_sensitive: None,
                whole_word: None,
                main_rel: None,
                token: None,
                batch: None,
            }))
            .unwrap();
        let ReplaceOut::Preview(plan) = preview.0 else {
            panic!("preview must return its plan");
        };
        assert_eq!(plan.replacements, 3);
        let applied = m
            .replace(Parameters(ReplaceParams {
                root_id: "cov".into(),
                action: ReplaceAction::Apply,
                pattern: None,
                replacement: None,
                regex: None,
                case_sensitive: None,
                whole_word: None,
                main_rel: None,
                token: Some(plan.token),
                batch: None,
            }))
            .unwrap();
        let ReplaceOut::Applied(done) = applied.0 else {
            panic!("apply must return what it wrote");
        };
        assert!(done.written.contains(&"main.tex".to_string()));
        assert!(
            std::fs::read_to_string(canon.join("main.tex"))
                .unwrap()
                .contains("Hello")
        );
        let undone = m
            .replace(Parameters(ReplaceParams {
                root_id: "cov".into(),
                action: ReplaceAction::Undo,
                pattern: None,
                replacement: None,
                regex: None,
                case_sensitive: None,
                whole_word: None,
                main_rel: None,
                token: None,
                batch: Some(done.batch),
            }))
            .unwrap();
        let ReplaceOut::Undone(restored) = undone.0 else {
            panic!("undo must return what it restored");
        };
        assert!(restored.restored.iter().any(|f| f.rel == "main.tex"));
        assert!(
            std::fs::read_to_string(canon.join("main.tex"))
                .unwrap()
                .contains("\\section{Hi}")
        );

        let found = m
            .search(Parameters(SearchParams {
                root_id: "cov".into(),
                action: SearchAction::FindFiles,
                pattern: None,
                query: Some("main".into()),
                regex: None,
                case_sensitive: None,
                whole_word: None,
                main_rel: None,
                max: None,
            }))
            .unwrap();
        assert!(matches!(found.0, SearchOut::Files(ref f) if !f.files.is_empty()));

        // Never compiled: the `paper` diagnostics reading fails like the core's.
        let mut diag = base(PaperAction::Diagnostics);
        diag.root_id = Some("cov".into());
        diag.main_rel = Some("main.tex".into());
        assert!(paper(diag).is_err());

        // Never compiled: the widget actions fail closed, none returns an empty list.
        let widget = |action: WidgetAction, widget: Option<&str>| {
            m.widget(Parameters(WidgetParams {
                root_id: "cov".into(),
                action,
                main_rel: "main.tex".into(),
                widget: widget.map(Into::into),
            }))
        };
        assert!(widget(WidgetAction::List, None).is_err());
        assert!(widget(WidgetAction::Status, None).is_err());
        assert!(widget(WidgetAction::Check, Some("w")).is_err());
        assert!(
            m.export_write(Parameters(ExportWriteParams {
                root_id: "cov".into(),
                action: ExportWriteAction::Preview,
                main_rel: Some("main.tex".into()),
                dest: None,
                profile: None,
                size_cap_bytes: None,
            }))
            .is_err()
        );
        assert!(
            m.export_write(Parameters(ExportWriteParams {
                root_id: "cov".into(),
                action: ExportWriteAction::Bundle,
                main_rel: Some("main.tex".into()),
                dest: Some("/tmp/maleficium-cov-bundle".into()),
                profile: Some(maleficium_events::BundleProfile::Folder),
                size_cap_bytes: None,
            }))
            .is_err()
        );

        // The runtime read: the scaffold refuses a bad name
        // before touching the library, and the validator fails closed on
        // an ungranted root or a missing library copy. Neither writes to
        // a project, and neither can approve.
        let read = |p: RuntimeReadParams| m.runtime_read(Parameters(p));
        let scaffold = |name: Option<&str>| RuntimeReadParams {
            action: RuntimeReadAction::Scaffold,
            name: name.map(Into::into),
            from: None,
            reference: None,
            root_id: None,
        };
        assert!(read(scaffold(Some("Model"))).is_err());
        assert!(
            read(RuntimeReadParams {
                action: RuntimeReadAction::Validate,
                name: None,
                from: None,
                reference: Some("orbit-view@1".into()),
                root_id: Some("nope".into()),
            })
            .is_err()
        );
        assert!(
            read(RuntimeReadParams {
                action: RuntimeReadAction::Validate,
                name: None,
                from: None,
                reference: Some("nope@1".into()),
                root_id: None,
            })
            .is_err()
        );

        let covered = [
            "session",
            "file",
            "file_mutate",
            "paper",
            "export_read",
            "export_write",
            "scaffold",
            "sync_view",
            "sync_render",
            "structure",
            "search",
            "replace",
            "widget",
            "runtime_read",
            "runtime_install",
            "presence",
            "describe",
        ];
        for t in Maleficium::tool_router().list_all() {
            assert!(
                covered.contains(&t.name.as_ref()),
                "tool `{}` has no coverage case",
                t.name
            );
        }
    }

    fn search_params(action: SearchAction) -> SearchParams {
        SearchParams {
            root_id: "t".into(),
            action,
            pattern: None,
            query: None,
            regex: None,
            case_sensitive: None,
            whole_word: None,
            main_rel: None,
            max: None,
        }
    }

    fn replace_params(action: ReplaceAction) -> ReplaceParams {
        ReplaceParams {
            root_id: "t".into(),
            action,
            pattern: None,
            replacement: None,
            regex: None,
            case_sensitive: None,
            whole_word: None,
            main_rel: None,
            token: None,
            batch: None,
        }
    }

    /// Grouped `session` validates per action: `grant` needs `root`,
    /// `info` takes nothing else, and an unknown action names the valid
    /// list with a did-you-mean hint. The tool registers roots but writes
    /// no project file, so it is honestly non-destructive.
    #[test]
    fn session_validates_per_action() {
        let tools = Maleficium::tool_router().list_all();
        let t = tools.iter().find(|t| t.name == "session").expect("session");
        let a = t.annotations.as_ref().expect("annotations");
        assert_eq!(a.read_only_hint, Some(false));
        assert_eq!(a.destructive_hint, Some(false));
        assert_eq!(a.idempotent_hint, Some(true));
        assert_eq!(a.open_world_hint, Some(false));
        let schema = serde_json::to_value(&*t.input_schema).unwrap();
        let mut props: Vec<&str> = schema["properties"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        props.sort();
        assert_eq!(props, ["action", "root", "root_id"]);
        assert_eq!(schema["additionalProperties"], false);

        let m = Maleficium::default();
        let dir = core::test_scratch::dir("mcp-session");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let root = dunce::canonicalize(&dir).unwrap();
        let root = root.to_string_lossy().to_string();

        let err = m
            .session(Parameters(SessionParams {
                root_id: "sess".into(),
                action: SessionAction::Grant,
                root: None,
            }))
            .err()
            .expect("grant without a root fails");
        assert!(err.contains("root"), "{err}");
        let grant = m
            .session(Parameters(SessionParams {
                root_id: "sess".into(),
                action: SessionAction::Grant,
                root: Some(root.clone()),
            }))
            .expect("a grant registers")
            .0;
        assert_eq!(grant.path, root);
        let info = m
            .session(Parameters(SessionParams {
                root_id: "sess".into(),
                action: SessionAction::Info,
                root: None,
            }))
            .expect("an info reads back")
            .0;
        assert_eq!(info.path, root);
        let err = m
            .session(Parameters(SessionParams {
                root_id: "sess".into(),
                action: SessionAction::Info,
                root: Some(root),
            }))
            .err()
            .expect("info rejects a root");
        assert!(err.contains("takes no `root`"), "{err}");

        let err = serde_json::from_value::<SessionParams>(serde_json::json!({
            "root_id": "r", "action": "gran",
        }))
        .expect_err("a near-miss action fails");
        assert!(err.to_string().contains("did you mean `grant`?"), "{err}");
        assert!(
            serde_json::from_value::<SessionParams>(serde_json::json!({
                "root_id": "r", "action": "info", "root_id2": "x",
            }))
            .is_err()
        );
    }

    /// Grouped `file` validates per action: `read` needs `rel`, `list`
    /// defaults it to the root, and an unknown action names the valid list
    /// with a did-you-mean hint. Both actions only read, so the tool stays
    /// honestly read-only.
    #[test]
    fn file_validates_per_action() {
        let tools = Maleficium::tool_router().list_all();
        let t = tools.iter().find(|t| t.name == "file").expect("file");
        let a = t.annotations.as_ref().expect("annotations");
        assert_eq!(a.read_only_hint, Some(true));
        assert_eq!(a.destructive_hint, Some(false));
        assert_eq!(a.idempotent_hint, Some(true));
        assert_eq!(a.open_world_hint, Some(false));
        let schema = serde_json::to_value(&*t.input_schema).unwrap();
        let mut props: Vec<&str> = schema["properties"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        props.sort();
        assert_eq!(props, ["action", "rel", "root_id"]);
        assert_eq!(schema["additionalProperties"], false);

        let m = Maleficium::default();
        let dir = core::test_scratch::dir("mcp-file");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("main.tex"), "\\section{Hi}\n").unwrap();
        let root = dunce::canonicalize(&dir).unwrap();
        core::grant_root(&m.cx, "file", &root.to_string_lossy()).unwrap();

        let err = m
            .file(Parameters(FileParams {
                root_id: "file".into(),
                action: FileAction::Read,
                rel: None,
            }))
            .err()
            .expect("read without a rel fails");
        assert!(err.contains("rel"), "{err}");
        let read = m
            .file(Parameters(FileParams {
                root_id: "file".into(),
                action: FileAction::Read,
                rel: Some("main.tex".into()),
            }))
            .expect("a read reads")
            .0;
        let FileOut::Text(read) = read else {
            panic!("read returns its text");
        };
        assert!(read.text.contains("\\section{Hi}"));
        let list = m
            .file(Parameters(FileParams {
                root_id: "file".into(),
                action: FileAction::List,
                rel: None,
            }))
            .expect("a list lists")
            .0;
        let FileOut::Listing(list) = list else {
            panic!("list returns its entries");
        };
        assert!(list.entries.iter().any(|e| e.name == "main.tex"));

        let err = serde_json::from_value::<FileParams>(serde_json::json!({
            "root_id": "r", "action": "lis",
        }))
        .expect_err("a near-miss action fails");
        assert!(err.to_string().contains("did you mean `list`?"), "{err}");
        assert!(
            serde_json::from_value::<FileParams>(serde_json::json!({
                "root_id": "r", "action": "read", "rel": "m.tex", "dest": "/tmp/x",
            }))
            .is_err()
        );
    }

    /// Grouped `file_mutate` keeps the two-call delete confirm gate across
    /// the split: `delete` without confirm is refused with the file's
    /// absolute path, `undo` needs the trash path, each rejects the other's
    /// fields, and an unknown action names the valid list with a
    /// did-you-mean hint. The worst action deletes, so the tool is honestly
    /// destructive.
    #[test]
    fn file_mutate_keeps_the_two_call_confirm_gate() {
        let tools = Maleficium::tool_router().list_all();
        let t = tools
            .iter()
            .find(|t| t.name == "file_mutate")
            .expect("file_mutate");
        let a = t.annotations.as_ref().expect("annotations");
        assert_eq!(a.read_only_hint, Some(false));
        assert_eq!(a.destructive_hint, Some(true));
        assert_eq!(a.idempotent_hint, Some(false));
        assert_eq!(a.open_world_hint, Some(false));
        let schema = serde_json::to_value(&*t.input_schema).unwrap();
        let mut props: Vec<&str> = schema["properties"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        props.sort();
        assert_eq!(props, ["action", "confirm", "rel", "root_id", "trash_path"]);
        assert_eq!(schema["additionalProperties"], false);

        let m = Maleficium::default();
        let dir = core::test_scratch::dir("mcp-file-mutate");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("victim.txt"), "bye").unwrap();
        let root = dunce::canonicalize(&dir).unwrap();
        core::grant_root(&m.cx, "mut", &root.to_string_lossy()).unwrap();
        let abs = root.join("victim.txt").to_string_lossy().to_string();

        let delete = |rel: Option<&str>, confirm: Option<&str>, trash: Option<&str>| {
            m.file_mutate(Parameters(FileMutateParams {
                root_id: "mut".into(),
                action: FileMutateAction::Delete,
                rel: rel.map(Into::into),
                confirm: confirm.map(Into::into),
                trash_path: trash.map(Into::into),
            }))
        };
        let err = delete(None, None, None)
            .err()
            .expect("delete without a rel fails");
        assert!(err.contains("rel"), "{err}");
        let err = delete(Some("victim.txt"), None, None)
            .err()
            .expect("delete without confirm is refused");
        assert!(err.contains(&abs), "{err}");
        assert!(root.join("victim.txt").exists());
        let err = delete(Some("victim.txt"), None, Some("/tmp/t"))
            .err()
            .expect("delete rejects a trash path");
        assert!(err.contains("takes no `trash_path`"), "{err}");
        let trashed = delete(Some("victim.txt"), Some(&abs), None)
            .expect("the confirmed call deletes")
            .0;
        let FileMutateOut::Deleted(trashed) = trashed else {
            panic!("delete returns its trash path");
        };
        assert!(!root.join("victim.txt").exists());

        let undo = |rel: Option<&str>, confirm: Option<&str>, trash: Option<&str>| {
            m.file_mutate(Parameters(FileMutateParams {
                root_id: "mut".into(),
                action: FileMutateAction::Undo,
                rel: rel.map(Into::into),
                confirm: confirm.map(Into::into),
                trash_path: trash.map(Into::into),
            }))
        };
        let err = undo(None, None, None)
            .err()
            .expect("undo without a trash path fails");
        assert!(err.contains("trash_path"), "{err}");
        let err = undo(Some("victim.txt"), None, Some(&trashed.trash_path))
            .err()
            .expect("undo rejects a rel");
        assert!(err.contains("takes only `trash_path`"), "{err}");
        let restored = undo(None, None, Some(&trashed.trash_path))
            .expect("an undo restores")
            .0;
        let FileMutateOut::Restored(restored) = restored else {
            panic!("undo returns its restored path");
        };
        assert!(restored.path.ends_with("victim.txt"));
        assert_eq!(
            std::fs::read_to_string(root.join("victim.txt")).unwrap(),
            "bye"
        );

        let err = serde_json::from_value::<FileMutateParams>(serde_json::json!({
            "root_id": "r", "action": "delet", "rel": "v.txt",
        }))
        .expect_err("a near-miss action fails");
        assert!(err.to_string().contains("did you mean `delete`?"), "{err}");
        assert!(
            serde_json::from_value::<FileMutateParams>(serde_json::json!({
                "root_id": "r", "action": "delete", "rel": "v.txt", "approve": true,
            }))
            .is_err()
        );
    }

    /// Grouped `structure` validates per action: each reading action names
    /// the file it needs and rejects the rest, `definition` takes kind and
    /// key or rel, line and col (never both), and an unknown action names
    /// the valid list with a did-you-mean hint. Six readings, no writes, so
    /// the tool stays honestly read-only.
    #[test]
    fn structure_validates_per_action() {
        let m = Maleficium::default();
        let dir = core::test_scratch::dir("mcp-structure");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("main.tex"),
            "\\section{Hi}\\label{sec:hi}\nSee~\\ref{sec:hi}.\n",
        )
        .unwrap();
        let root = dunce::canonicalize(&dir).unwrap();
        core::grant_root(&m.cx, "st", &root.to_string_lossy()).unwrap();

        let base = |action: StructureAction| StructureParams {
            root_id: "st".into(),
            action,
            rel: None,
            main_rel: None,
            kind: None,
            key: None,
            line: None,
            col: None,
        };
        let call = |p: StructureParams| m.structure(Parameters(p));

        let mut p = base(StructureAction::Outline);
        let err = call(p.clone()).err().expect("outline without a rel fails");
        assert!(err.contains("rel"), "{err}");
        p.rel = Some("main.tex".into());
        p.main_rel = Some("main.tex".into());
        let err = call(p).err().expect("outline rejects a main file");
        assert!(err.contains("takes only `rel`"), "{err}");

        for action in [
            StructureAction::FileGraph,
            StructureAction::LabelsRefs,
            StructureAction::Citations,
            StructureAction::DocumentStructure,
        ] {
            let mut p = base(action);
            let err = call(p.clone())
                .err()
                .expect("a main-file reading without one fails");
            assert!(err.contains("main_rel"), "{err}");
            p.main_rel = Some("main.tex".into());
            p.key = Some("sec:hi".into());
            let err = call(p).err().expect("a main-file reading rejects a key");
            assert!(err.contains("takes only `main_rel`"), "{err}");
        }
        let mut p = base(StructureAction::FileGraph);
        p.main_rel = Some("main.tex".into());
        assert!(call(p).is_ok());

        let mut p = base(StructureAction::Definition);
        let err = call(p.clone())
            .err()
            .expect("definition without either spelling fails");
        assert!(err.contains("kind and key"), "{err}");
        p.kind = Some(maleficium_index::definition::RefKind::Label);
        let err = call(p.clone())
            .err()
            .expect("definition with kind but no key fails");
        assert!(err.contains("kind and key"), "{err}");
        p.key = Some("sec:hi".into());
        p.rel = Some("main.tex".into());
        let err = call(p.clone())
            .err()
            .expect("definition with both spellings fails");
        assert!(err.contains("not both"), "{err}");
        p.rel = None;
        let defined = call(p).expect("a kind-and-key lookup runs").0;
        assert!(matches!(defined, StructureOut::Definition(ref d) if d.lookup.is_some()));

        let err = serde_json::from_value::<StructureParams>(serde_json::json!({
            "root_id": "r", "action": "outlin", "rel": "m.tex",
        }))
        .expect_err("a near-miss action fails");
        assert!(err.to_string().contains("did you mean `outline`?"), "{err}");
        let err = serde_json::from_value::<StructureParams>(serde_json::json!({
            "root_id": "r", "action": "graph", "main_rel": "m.tex",
        }))
        .expect_err("an unknown action fails");
        assert!(
            err.to_string().contains(
                "[outline, file_graph, labels_refs, citations, document_structure, definition]"
            ),
            "{err}"
        );
        assert!(
            serde_json::from_value::<StructureParams>(serde_json::json!({
                "root_id": "r", "action": "outline", "rel": "m.tex", "fetch": true,
            }))
            .is_err()
        );
    }

    /// Grouped `sync_view` validates per action: each view action names
    /// the target it needs and rejects the rest, before touching the
    /// project. An unknown action names the valid list with a did-you-mean
    /// hint. Every action only reads, so the tool stays honestly read-only
    /// and keeps the snippet View for the model.
    #[test]
    fn sync_view_validates_per_action() {
        let tools = Maleficium::tool_router().list_all();
        let t = tools
            .iter()
            .find(|t| t.name == "sync_view")
            .expect("sync_view");
        let a = t.annotations.as_ref().expect("annotations");
        assert_eq!(a.read_only_hint, Some(true));
        assert_eq!(a.destructive_hint, Some(false));
        assert_eq!(a.idempotent_hint, Some(true));
        assert_eq!(a.open_world_hint, Some(false));
        assert!(t.output_schema.is_some());
        let schema = serde_json::to_value(&*t.input_schema).unwrap();
        let mut props: Vec<&str> = schema["properties"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        props.sort();
        assert_eq!(
            props,
            [
                "action",
                "label",
                "line",
                "main_rel",
                "page",
                "root_id",
                "tex_rel",
                "with_image",
                "x",
                "y"
            ]
        );
        assert_eq!(schema["additionalProperties"], false);

        let m = Maleficium::default();
        let base = |action: SyncViewAction| SyncViewParams {
            root_id: "nope".into(),
            action,
            main_rel: "main.tex".into(),
            tex_rel: None,
            line: None,
            label: None,
            page: None,
            x: None,
            y: None,
            with_image: None,
        };
        let call = |p: SyncViewParams| m.sync_view(Parameters(p));

        let mut p = base(SyncViewAction::Forward);
        let Err(err) = call(p.clone()) else {
            panic!("forward without a target fails");
        };
        assert!(err.contains("tex_rel"), "{err}");
        p.tex_rel = Some("main.tex".into());
        let Err(err) = call(p.clone()) else {
            panic!("forward without a line fails");
        };
        assert!(err.contains("line"), "{err}");
        p.line = Some(10);
        p.label = Some("fig:x".into());
        let Err(err) = call(p) else {
            panic!("forward rejects a label");
        };
        assert!(err.contains("forward takes no `label`"), "{err}");

        let mut p = base(SyncViewAction::Inverse);
        let Err(err) = call(p.clone()) else {
            panic!("inverse without a page fails");
        };
        assert!(err.contains("page"), "{err}");
        p.page = Some(1);
        p.tex_rel = Some("main.tex".into());
        let Err(err) = call(p) else {
            panic!("inverse rejects a source file");
        };
        assert!(err.contains("inverse takes no `tex_rel`"), "{err}");

        let mut p = base(SyncViewAction::Snippet);
        p.page = Some(1);
        p.x = Some(3.0);
        let Err(err) = call(p) else {
            panic!("snippet rejects a position");
        };
        assert!(err.contains("snippet takes no `x`"), "{err}");

        let err = serde_json::from_value::<SyncViewParams>(serde_json::json!({
            "root_id": "r", "action": "foward", "main_rel": "m.tex", "tex_rel": "a.tex", "line": 1,
        }))
        .expect_err("a near-miss action fails");
        assert!(err.to_string().contains("did you mean `forward`?"), "{err}");
        assert!(
            serde_json::from_value::<SyncViewParams>(serde_json::json!({
                "root_id": "r", "action": "snippet", "main_rel": "m.tex", "page": 1, "approve": true,
            }))
            .is_err()
        );
    }

    /// `sync_render` keeps the re-render's contract across the split: it
    /// names the snippet View, stays hidden from the model, and renders the
    /// named page (or its region band) at the named scale. Per-action there
    /// is only `render`; anything else names it with a did-you-mean hint.
    #[test]
    fn sync_render_keeps_the_app_only_rerender() {
        let tools = Maleficium::tool_router().list_all();
        let t = tools
            .iter()
            .find(|t| t.name == "sync_render")
            .expect("sync_render");
        let a = t.annotations.as_ref().expect("annotations");
        assert_eq!(a.read_only_hint, Some(true));
        assert_eq!(a.destructive_hint, Some(false));
        assert_eq!(a.idempotent_hint, Some(true));
        assert_eq!(a.open_world_hint, Some(false));
        assert!(t.output_schema.is_some());
        let schema = serde_json::to_value(&*t.input_schema).unwrap();
        let mut props: Vec<&str> = schema["properties"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        props.sort();
        assert_eq!(
            props,
            ["action", "main_rel", "page", "region", "root_id", "scale"]
        );
        assert_eq!(schema["additionalProperties"], false);

        let err = serde_json::from_value::<SyncRenderParams>(serde_json::json!({
            "root_id": "r", "action": "rendr", "main_rel": "m.tex", "page": 1,
        }))
        .expect_err("a near-miss action fails");
        assert!(err.to_string().contains("did you mean `render`?"), "{err}");
        assert!(
            serde_json::from_value::<SyncRenderParams>(serde_json::json!({
                "root_id": "r", "action": "render", "main_rel": "m.tex", "page": 1, "wait_ms": 3,
            }))
            .is_err()
        );
    }

    /// Grouped `scaffold` validates per action: `list` takes nothing,
    /// `create` needs the template, the parent folder and the name, and an
    /// unknown action names the valid list with a did-you-mean hint. Only
    /// `create` writes (outside any project), so the tool is honestly
    /// non-idempotent.
    #[test]
    fn scaffold_validates_per_action() {
        let tools = Maleficium::tool_router().list_all();
        let t = tools
            .iter()
            .find(|t| t.name == "scaffold")
            .expect("scaffold");
        let a = t.annotations.as_ref().expect("annotations");
        assert_eq!(a.read_only_hint, Some(false));
        assert_eq!(a.destructive_hint, Some(false));
        assert_eq!(a.idempotent_hint, Some(false));
        assert_eq!(a.open_world_hint, Some(false));
        assert!(t.output_schema.is_some());
        let schema = serde_json::to_value(&*t.input_schema).unwrap();
        let mut props: Vec<&str> = schema["properties"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        props.sort();
        assert_eq!(props, ["action", "name", "parent_dir", "template"]);
        assert_eq!(schema["additionalProperties"], false);

        let m = Maleficium::default();
        let list = m
            .scaffold(Parameters(ScaffoldParams {
                action: ScaffoldAction::List,
                template: None,
                parent_dir: None,
                name: None,
            }))
            .expect("a list lists")
            .0;
        let ScaffoldOut::List(list) = list else {
            panic!("list returns its templates");
        };
        assert!(!list.templates.is_empty());
        let err = m
            .scaffold(Parameters(ScaffoldParams {
                action: ScaffoldAction::List,
                template: Some("article".into()),
                parent_dir: None,
                name: None,
            }))
            .err()
            .expect("list rejects a template");
        assert!(err.contains("takes no `template`"), "{err}");

        let create = |template: Option<&str>, parent: Option<&str>, name: Option<&str>| {
            m.scaffold(Parameters(ScaffoldParams {
                action: ScaffoldAction::Create,
                template: template.map(Into::into),
                parent_dir: parent.map(Into::into),
                name: name.map(Into::into),
            }))
        };
        let dir = core::test_scratch::dir("mcp-scaffold");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let parent = dir.to_string_lossy().to_string();
        let err = create(None, Some(&parent), Some("p1"))
            .err()
            .expect("create without a template fails");
        assert!(err.contains("template"), "{err}");
        let err = create(Some("welcome"), None, Some("p1"))
            .err()
            .expect("create without a parent fails");
        assert!(err.contains("parent_dir"), "{err}");
        let created = create(Some("welcome"), Some(&parent), Some("p1"))
            .expect("a create creates")
            .0;
        let ScaffoldOut::Created(created) = created else {
            panic!("create returns its project");
        };
        assert!(
            std::path::Path::new(&created.root)
                .join(&created.main)
                .is_file()
        );
        let err = create(Some("welcome"), Some(&parent), Some("p1"))
            .err()
            .expect("create refuses a folder in use");
        assert!(err.contains("not empty"), "{err}");

        let err = serde_json::from_value::<ScaffoldParams>(serde_json::json!({
            "action": "creat", "template": "article", "parent_dir": "/tmp/x", "name": "p",
        }))
        .expect_err("a near-miss action fails");
        assert!(err.to_string().contains("did you mean `create`?"), "{err}");
        assert!(
            serde_json::from_value::<ScaffoldParams>(serde_json::json!({
                "action": "list", "root_id": "r",
            }))
            .is_err()
        );
    }

    /// `describe` is the read-only reference behind the short upfront
    /// descriptions: one cluster's schema, hints, examples and token-flow,
    /// narrowed to one action when asked. Unknown clusters and actions name
    /// the valid list with a did-you-mean hint; single-purpose tools take
    /// no action. Every registered tool resolves, and every grouped action
    /// has a calling-rules note.
    #[test]
    fn describe_reports_every_cluster_and_action() {
        let tools = Maleficium::tool_router().list_all();
        let t = tools
            .iter()
            .find(|t| t.name == "describe")
            .expect("describe");
        let a = t.annotations.as_ref().expect("annotations");
        assert_eq!(a.read_only_hint, Some(true));
        assert_eq!(a.destructive_hint, Some(false));
        assert_eq!(a.idempotent_hint, Some(true));
        assert_eq!(a.open_world_hint, Some(false));
        assert!(t.output_schema.is_some());
        let schema = serde_json::to_value(&*t.input_schema).unwrap();
        let mut props: Vec<&str> = schema["properties"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        props.sort();
        assert_eq!(props, ["action", "cluster"]);
        assert_eq!(schema["additionalProperties"], false);

        let m = Maleficium::default();
        let describe = |cluster: &str, action: Option<&str>| {
            m.describe(Parameters(DescribeParams {
                cluster: cluster.into(),
                action: action.map(Into::into),
            }))
        };
        // Every registered tool resolves to its own description and schema.
        for tool in &tools {
            let out = describe(&tool.name, None)
                .unwrap_or_else(|e| panic!("describe resolves `{}`: {e}", tool.name))
                .0;
            assert_eq!(out.cluster, tool.name);
            assert!(out.action.is_none());
            assert_eq!(
                out.description,
                tool.description.as_deref().unwrap_or(""),
                "{}",
                tool.name
            );
            assert!(out.input_schema.is_object(), "{}", tool.name);
            assert!(!out.token_flow.is_empty(), "{}", tool.name);
        }
        // Every grouped action validates and carries its calling rules.
        let mut noted = 0;
        for tool in &tools {
            if let Some(actions) = cluster_actions(&tool.name) {
                assert!(!actions.is_empty(), "{}", tool.name);
                for action in actions {
                    let out = describe(&tool.name, Some(action))
                        .unwrap_or_else(|e| {
                            panic!("describe resolves `{}` `{action}`: {e}", tool.name)
                        })
                        .0;
                    assert_eq!(out.action.as_deref(), Some(*action));
                    assert!(!out.hints.is_empty(), "{} {action}", tool.name);
                    assert!(!out.examples.is_empty(), "{} {action}", tool.name);
                    assert!(action_notes(&tool.name, action).is_some());
                    noted += 1;
                }
            } else {
                let Err(err) = describe(&tool.name, Some("nope")) else {
                    panic!("`{}` takes no action", tool.name);
                };
                assert!(err.contains("takes no action"), "{err}");
            }
        }
        assert!(noted >= 30, "every grouped action has a note, got {noted}");

        let Err(err) = describe("structur", None) else {
            panic!("a near-miss cluster fails");
        };
        assert!(err.contains("did you mean `structure`?"), "{err}");
        let Err(err) = describe("structure", Some("outlin")) else {
            panic!("a near-miss action fails");
        };
        assert!(err.contains("did you mean `outline`?"), "{err}");
        let Err(err) = describe("runtime_install", Some("tail")) else {
            panic!("an action on a single-purpose tool fails");
        };
        assert!(err.contains("takes no action"), "{err}");
        assert!(
            serde_json::from_value::<DescribeParams>(serde_json::json!({
                "cluster": "structure", "action": "outline", "verbose": true,
            }))
            .is_err()
        );

        // The coverage case exercises describe beside the other tools.
        let out = describe("sync_view", Some("snippet")).unwrap().0;
        assert_eq!(out.cluster, "sync_view");
        assert!(out.hints.iter().any(|h| h.contains("with_image")));
    }

    /// Grouped `search` validates per action: `search` needs `pattern`,
    /// `find_files` needs `query`, each rejects the other's fields, and an
    /// unknown action names the valid list with a did-you-mean hint. The
    /// tool stays read-only: only `replace` writes.
    #[test]
    fn search_validates_per_action() {
        let tools = Maleficium::tool_router().list_all();
        let t = tools.iter().find(|t| t.name == "search").expect("search");
        let a = t.annotations.as_ref().expect("annotations");
        assert_eq!(a.read_only_hint, Some(true));
        assert_eq!(a.destructive_hint, Some(false));
        assert_eq!(a.idempotent_hint, Some(true));
        assert_eq!(a.open_world_hint, Some(false));

        let m = Maleficium::default();
        let err = m
            .search(Parameters(search_params(SearchAction::Search)))
            .err()
            .expect("search without a pattern fails");
        assert!(err.contains("pattern"), "{err}");

        let err = m
            .search(Parameters(search_params(SearchAction::FindFiles)))
            .err()
            .expect("find_files without a query fails");
        assert!(err.contains("query"), "{err}");

        let mut query_on_search = search_params(SearchAction::Search);
        query_on_search.pattern = Some("x".into());
        query_on_search.query = Some("x".into());
        let err = m
            .search(Parameters(query_on_search))
            .err()
            .expect("search rejects `query`");
        assert!(err.contains("find_files"), "{err}");

        let mut pattern_on_files = search_params(SearchAction::FindFiles);
        pattern_on_files.query = Some("main".into());
        pattern_on_files.pattern = Some("main".into());
        let err = m
            .search(Parameters(pattern_on_files))
            .err()
            .expect("find_files rejects `pattern`");
        assert!(err.contains("find_files takes only query + max"), "{err}");

        let err = serde_json::from_value::<SearchParams>(serde_json::json!({
            "root_id": "r", "action": "find", "query": "x",
        }))
        .expect_err("an unknown action fails");
        assert!(err.to_string().contains("[search, find_files]"), "{err}");

        let err = serde_json::from_value::<SearchParams>(serde_json::json!({
            "root_id": "r", "action": "serach", "pattern": "x",
        }))
        .expect_err("a near-miss action fails");
        assert!(err.to_string().contains("did you mean `search`?"), "{err}");
    }

    /// Grouped `replace` keeps the token state machine across the split:
    /// preview, apply and undo restore bytes, a token applies once, and a
    /// plan over files changed since the preview is refused whole. Each
    /// action names the field it needs. The worst action writes, so the
    /// tool is honestly destructive.
    #[test]
    fn replace_token_flow() {
        let tools = Maleficium::tool_router().list_all();
        let t = tools.iter().find(|t| t.name == "replace").expect("replace");
        let a = t.annotations.as_ref().expect("annotations");
        assert_eq!(a.read_only_hint, Some(false));
        assert_eq!(a.destructive_hint, Some(true));
        assert_eq!(a.idempotent_hint, Some(false));
        assert_eq!(a.open_world_hint, Some(false));

        let m = Maleficium::default();
        let dir = core::test_scratch::dir("mcp-replace-flow");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("main.tex"), "\\section{Hi}\n").unwrap();
        let root = dunce::canonicalize(&dir).unwrap();
        core::grant_root(&m.cx, "flow", &root.to_string_lossy()).unwrap();
        let before = std::fs::read_to_string(root.join("main.tex")).unwrap();

        let mut preview = replace_params(ReplaceAction::Preview);
        preview.root_id = "flow".into();
        preview.pattern = Some("Hi".into());
        preview.replacement = Some("Hello".into());
        let ReplaceOut::Preview(plan) = m.replace(Parameters(preview)).unwrap().0 else {
            panic!("preview returns its plan");
        };
        assert!(!plan.token.is_empty());

        let mut apply = replace_params(ReplaceAction::Apply);
        apply.root_id = "flow".into();
        apply.token = Some(plan.token.clone());
        let ReplaceOut::Applied(done) = m.replace(Parameters(apply)).unwrap().0 else {
            panic!("apply returns what it wrote");
        };
        assert!(done.written.contains(&"main.tex".to_string()));
        assert!(
            std::fs::read_to_string(root.join("main.tex"))
                .unwrap()
                .contains("Hello")
        );

        let mut again = replace_params(ReplaceAction::Apply);
        again.root_id = "flow".into();
        again.token = Some(plan.token.clone());
        let err = m
            .replace(Parameters(again))
            .err()
            .expect("a spent token fails");
        assert!(err.contains("unknown or expired"), "{err}");

        let mut undo = replace_params(ReplaceAction::Undo);
        undo.root_id = "flow".into();
        undo.batch = Some(done.batch);
        let ReplaceOut::Undone(restored) = m.replace(Parameters(undo)).unwrap().0 else {
            panic!("undo returns what it restored");
        };
        assert!(restored.restored.iter().any(|f| f.rel == "main.tex"));
        assert_eq!(
            std::fs::read_to_string(root.join("main.tex")).unwrap(),
            before
        );

        // A file changed after the preview refuses the whole plan.
        let mut preview = replace_params(ReplaceAction::Preview);
        preview.root_id = "flow".into();
        preview.pattern = Some("Hi".into());
        preview.replacement = Some("Hello".into());
        let ReplaceOut::Preview(stale) = m.replace(Parameters(preview)).unwrap().0 else {
            panic!("preview returns its plan");
        };
        std::fs::write(root.join("main.tex"), "edited elsewhere\n").unwrap();
        let mut apply = replace_params(ReplaceAction::Apply);
        apply.root_id = "flow".into();
        apply.token = Some(stale.token);
        let err = m
            .replace(Parameters(apply))
            .err()
            .expect("a stale plan is refused");
        assert!(err.contains("nothing replaced"), "{err}");
        assert_eq!(
            std::fs::read_to_string(root.join("main.tex")).unwrap(),
            "edited elsewhere\n",
            "a refused plan writes nothing"
        );

        let mut no_replacement = replace_params(ReplaceAction::Preview);
        no_replacement.pattern = Some("x".into());
        let err = m
            .replace(Parameters(no_replacement))
            .err()
            .expect("preview without a replacement fails");
        assert!(err.contains("replacement"), "{err}");

        let err = m
            .replace(Parameters(replace_params(ReplaceAction::Apply)))
            .err()
            .expect("apply without a token fails");
        assert!(err.contains("token"), "{err}");

        let err = m
            .replace(Parameters(replace_params(ReplaceAction::Undo)))
            .err()
            .expect("undo without a batch fails");
        assert!(err.contains("batch"), "{err}");

        let err = serde_json::from_value::<ReplaceParams>(serde_json::json!({
            "root_id": "r", "action": "aply", "token": "t",
        }))
        .expect_err("a near-miss action fails");
        assert!(err.to_string().contains("did you mean `apply`?"), "{err}");
    }

    /// The grouped tools refuse unknown fields at the schema and at the
    /// action: serde rejects extras (mirroring `app_request_open`), the
    /// schemas carry `additionalProperties: false`, and the property lists
    /// stay flat objects. Handler checks catch per-action extras the
    /// schema cannot see, before touching the project.
    #[test]
    fn search_and_replace_reject_unknown_fields() {
        let tools = Maleficium::tool_router().list_all();
        let schema_of = |name: &str| {
            let t = tools.iter().find(|t| t.name == name).expect(name);
            serde_json::to_value(&*t.input_schema).unwrap()
        };
        let search_schema = schema_of("search");
        let mut search_props: Vec<&str> = search_schema["properties"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        search_props.sort();
        assert_eq!(
            search_props,
            [
                "action",
                "case_sensitive",
                "main_rel",
                "max",
                "pattern",
                "query",
                "regex",
                "root_id",
                "whole_word"
            ]
        );
        let replace_schema = schema_of("replace");
        let mut replace_props: Vec<&str> = replace_schema["properties"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        replace_props.sort();
        assert_eq!(
            replace_props,
            [
                "action",
                "batch",
                "case_sensitive",
                "main_rel",
                "pattern",
                "regex",
                "replacement",
                "root_id",
                "token",
                "whole_word"
            ]
        );
        for name in ["search", "replace"] {
            assert_eq!(schema_of(name)["additionalProperties"], false, "{name}");
        }
        assert!(
            serde_json::from_value::<SearchParams>(serde_json::json!({
                "root_id": "r", "action": "search", "pattern": "x", "token": "t",
            }))
            .is_err()
        );
        assert!(
            serde_json::from_value::<ReplaceParams>(serde_json::json!({
                "root_id": "r", "action": "apply", "token": "t", "approve": true,
            }))
            .is_err()
        );

        // Per-action extras die in the handler, on an ungranted root: no
        // project is touched to learn the fields do not belong.
        let m = Maleficium::default();
        let mut apply = replace_params(ReplaceAction::Apply);
        apply.token = Some("t".into());
        apply.batch = Some("b".into());
        let err = m
            .replace(Parameters(apply))
            .err()
            .expect("apply rejects `batch`");
        assert!(err.contains("apply takes only `token`"), "{err}");
    }

    /// Grouped `paper` validates per action: `start` compiles a project
    /// and returns a job id, `poll` and `cancel` name that job, and the
    /// readings name the project they read. Each action rejects the rest
    /// before touching the project, and an unknown action names the valid
    /// list with a did-you-mean hint. `start` can fetch, so the tool is
    /// honestly open-world; nothing it does is destructive.
    #[test]
    fn paper_validates_per_action() {
        let tools = Maleficium::tool_router().list_all();
        let t = tools.iter().find(|t| t.name == "paper").expect("paper");
        let a = t.annotations.as_ref().expect("annotations");
        assert_eq!(a.read_only_hint, Some(false));
        assert_eq!(a.destructive_hint, Some(false));
        assert_eq!(a.idempotent_hint, Some(false));
        assert_eq!(a.open_world_hint, Some(true));
        assert!(t.output_schema.is_some());
        let schema = serde_json::to_value(&*t.input_schema).unwrap();
        let mut props: Vec<&str> = schema["properties"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        props.sort();
        assert_eq!(
            props,
            [
                "action",
                "job_id",
                "main_rel",
                "max",
                "networked",
                "root_id",
                "tail_lines",
                "wait_ms"
            ]
        );
        assert_eq!(schema["additionalProperties"], false);
        assert!(
            serde_json::from_value::<PaperParams>(
                serde_json::json!({"action": "start", "root_id": "r", "main_rel": "m.tex"})
            )
            .is_ok()
        );
        assert!(
            serde_json::from_value::<PaperParams>(
                serde_json::json!({"action": "poll", "job_id": "job-1", "approve": true})
            )
            .is_err()
        );

        let m = Maleficium::default();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let paper = |p: PaperParams| rt.block_on(m.paper(Parameters(p)));
        let base = |action: PaperAction| PaperParams {
            action,
            root_id: None,
            job_id: None,
            main_rel: None,
            networked: None,
            tail_lines: None,
            wait_ms: None,
            max: None,
        };
        // Per-action extras die in the handler: no project is touched to
        // learn the fields do not belong.
        let mut start = base(PaperAction::Start);
        start.job_id = Some("job-1".into());
        let err = paper(start).err().expect("start rejects a job id");
        assert!(err.contains("takes no `job_id`"), "{err}");

        let mut start = base(PaperAction::Start);
        start.root_id = Some("nope".into());
        start.main_rel = Some("main.tex".into());
        start.max = Some(10);
        let err = paper(start).err().expect("start rejects a max");
        assert!(err.contains("takes no `max`"), "{err}");

        let err = paper(base(PaperAction::Poll))
            .err()
            .expect("poll without a job fails");
        assert!(err.contains("job_id"), "{err}");

        let mut poll = base(PaperAction::Poll);
        poll.job_id = Some("job-1".into());
        poll.root_id = Some("r".into());
        let err = paper(poll).err().expect("poll rejects a root");
        assert!(err.contains("poll takes only"), "{err}");

        let mut cancel = base(PaperAction::Cancel);
        cancel.root_id = Some("r".into());
        cancel.job_id = Some("job-1".into());
        let err = paper(cancel).err().expect("cancel rejects a root");
        assert!(err.contains("cancel takes only"), "{err}");

        let err = paper(base(PaperAction::Diagnostics))
            .err()
            .expect("diagnostics without a root fails");
        assert!(err.contains("root_id"), "{err}");

        let mut prechecks = base(PaperAction::Prechecks);
        prechecks.root_id = Some("r".into());
        prechecks.main_rel = Some("m.tex".into());
        prechecks.max = Some(10);
        let err = paper(prechecks).err().expect("prechecks rejects a max");
        assert!(err.contains("prechecks takes only"), "{err}");

        let mut readiness = base(PaperAction::Readiness);
        readiness.root_id = Some("r".into());
        readiness.main_rel = Some("m.tex".into());
        let err = paper(readiness)
            .err()
            .expect("readiness rejects a main file");
        assert!(err.contains("readiness takes only"), "{err}");

        let mut output = base(PaperAction::Output);
        output.main_rel = Some("m.tex".into());
        let err = paper(output).err().expect("output without a root fails");
        assert!(err.contains("root_id"), "{err}");

        let mut log = base(PaperAction::Log);
        log.main_rel = Some("m.tex".into());
        let err = paper(log).err().expect("log without a root fails");
        assert!(err.contains("root_id"), "{err}");

        let mut log = base(PaperAction::Log);
        log.root_id = Some("r".into());
        log.main_rel = Some("m.tex".into());
        log.job_id = Some("job-1".into());
        let err = paper(log).err().expect("log rejects a job");
        assert!(err.contains("log takes only"), "{err}");

        let err = serde_json::from_value::<PaperParams>(serde_json::json!({
            "action": "pol", "job_id": "job-1",
        }))
        .expect_err("a near-miss action fails");
        assert!(err.to_string().contains("did you mean `poll`?"), "{err}");
        let err = serde_json::from_value::<PaperParams>(serde_json::json!({
            "action": "cancelled", "job_id": "job-1",
        }))
        .expect_err("an unknown action fails");
        assert!(
            err.to_string()
                .contains("[start, poll, cancel, diagnostics, prechecks, readiness, output, log]"),
            "{err}"
        );
    }

    /// Grouped tools stay under the token budget: every registered tool
    /// costs under 1000 tokens of input schema plus description (chars/4),
    /// so a later noun tool that creeps over fails here instead of in a
    /// rescan. The count lands the phase at 17 tools: 27 minus the 13
    /// folded, plus the 3 grouped nouns.
    #[test]
    fn grouped_tools_stay_under_token_budget() {
        let tools = Maleficium::tool_router().list_all();
        assert_eq!(tools.len(), 17, "the phase lands at 17 tools");
        for t in &tools {
            let schema = serde_json::to_value(&*t.input_schema).unwrap().to_string();
            let desc = t.description.as_deref().unwrap_or("");
            let est = (schema.len() + desc.len()) / 4;
            assert!(
                est < 1000,
                "tool `{}` costs ~{est} tokens (input schema {} chars + description {} chars)",
                t.name,
                schema.len(),
                desc.len()
            );
        }
    }
}
