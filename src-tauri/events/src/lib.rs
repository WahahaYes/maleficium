//! The app event catalog: the one definition of every bus payload.
//!
//! The frontend's TypeScript types are generated from these (see
//! `typescript()`), so the desktop bus, the JSONL event log and the MCP
//! tools read one schema. Pure by contract: no
//! fs, no process, no Tauri.

use maleficium_structure::{
    CompilePhase, Diagnostic, FetchOutcome, Finding, LineSignal, MissingDependency, MissingReason,
};
use serde::{Deserialize, Serialize};
use ts_rs::{Config, TS};

/// Who caused an event: a person at the app, an agent acting through the
/// automation surface, or the app itself (watchers, timers, warm-open).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, TS)]
#[serde(rename_all = "lowercase")]
pub enum Actor {
    User,
    Agent,
    System,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, TS)]
#[serde(rename_all = "lowercase")]
pub enum EventScope {
    Compile,
    Preview,
    Fs,
    App,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, TS)]
#[serde(rename_all = "lowercase")]
pub enum EventKind {
    Info,
    Progress,
    Success,
    Error,
    Warn,
}

/// How a save was triggered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, TS)]
#[serde(rename_all = "lowercase")]
pub enum SaveMode {
    Manual,
    Untitled,
    Auto,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, TS)]
#[serde(rename_all = "kebab-case")]
pub enum SaveBlockedReason {
    LargePlaceholder,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, TS)]
#[serde(rename_all = "kebab-case")]
pub enum CommandBlockedReason {
    NoProject,
    NotAProjectFile,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, TS)]
#[serde(rename_all = "kebab-case")]
pub enum CompileBlockedReason {
    LargePlaceholder,
    NonTextSelection,
    NotATexFile,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, TS)]
#[serde(rename_all = "kebab-case")]
pub enum CompileRefusedReason {
    EditorDoesNotOwnTarget,
}

/// Why a finished compile produced no output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, TS)]
#[serde(rename_all = "kebab-case")]
pub enum CompileFailure {
    SpawnFailed,
    EngineError,
    /// The engine could not get a dependency; `compile.missing` says which.
    MissingDependency,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, TS)]
#[serde(rename_all = "kebab-case")]
pub enum WarmSkippedReason {
    NoCachedOutput,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, TS)]
#[serde(rename_all = "kebab-case")]
pub enum CleanSkippedReason {
    NoProjectFile,
}

/// Why a save produced no revision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, TS)]
#[serde(rename_all = "kebab-case")]
pub enum RevisionSkipReason {
    NotText,
    TooLarge,
    Unchanged,
    Unavailable,
    /// The project's history index exists but cannot be read; it is left
    /// untouched rather than replaced.
    IndexUnreadable,
}

/// What asked for a save: the user's Ctrl+S, or the app on their behalf.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, TS)]
#[serde(rename_all = "lowercase")]
pub enum SaveTrigger {
    Manual,
    Switch,
    Close,
    Auto,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, TS)]
#[serde(rename_all = "lowercase")]
pub enum WatchChange {
    Create,
    Modify,
    Delete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, TS)]
#[serde(rename_all = "lowercase")]
pub enum SyncDirection {
    Forward,
    Inverse,
}

/// Where a compile line came from: the engine's two streams, or the app's
/// own status line about the run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, TS)]
#[serde(rename_all = "lowercase")]
pub enum CompileStream {
    Stdout,
    Stderr,
    Status,
}

/// One line of a running compile, as the desktop backend streams it, with
/// what the line says when it says something typed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, TS)]
pub struct CompileLine {
    pub stream: CompileStream,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub signal: Option<LineSignal>,
}

/// How a desktop compile ended. `pdfUrl` is set on success; `failure` says
/// why there is none. `missing` names the dependency the run lacked, and is
/// also set beside a pdf when the pinned bundle changed under it. `message`
/// is the human-readable failure, empty on success.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct CompileReport {
    pub pdf_url: Option<String>,
    pub failure: Option<CompileFailure>,
    pub missing: Option<MissingDependency>,
    pub message: String,
}

/// How the preview sizes pages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, TS)]
#[serde(rename_all = "kebab-case")]
pub enum ZoomKind {
    FitWidth,
    FitPage,
    Percent,
}

/// What an export wrote.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, TS)]
#[serde(rename_all = "lowercase")]
pub enum ExportKind {
    Pdf,
    Zip,
}

/// Whether a project compiles without network, as far as its last compiles
/// and this machine show.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, TS)]
#[serde(rename_all = "kebab-case")]
pub enum OfflineState {
    /// The last compile succeeded from the cache alone against the pinned
    /// bundle, and every tool and system file it used is present.
    Ready,
    /// A fetch would fix it: `needs` names the files.
    NeedsNetwork,
    /// A program the document runs is not installed.
    NeedsTool,
    /// A system font the document uses is not installed.
    NeedsFont,
    /// Neither network nor installs fix it (outside the bundle, needs
    /// shell escape, bad bundle).
    Blocked,
    /// No compile has shown it yet, or the pinned bundle changed.
    Unverified,
}

/// A project's offline readiness: the state, what it needs (files, tools,
/// fonts, as names), and the dependency its last compile lacked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, TS)]
pub struct OfflineReadiness {
    pub state: OfflineState,
    pub needs: Vec<String>,
    pub missing: Option<MissingDependency>,
}

/// One stored revision as it crosses the seam: opaque id, no hash, no path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, TS)]
pub struct Revision {
    pub rev: String,
    /// Milliseconds since the Unix epoch.
    pub at: u64,
    pub bytes: u64,
    /// The replace batch this revision was taken for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub batch: Option<String>,
}

/// History caps and what one project currently holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct RetentionInfo {
    pub max_revisions_per_file: u32,
    pub max_history_bytes_per_project: u64,
    pub min_revisions_kept_per_file: u32,
    pub snapshot_max_file_bytes: u64,
    /// Revisions currently held across the project.
    pub revisions: u32,
    /// Summed distinct blob bytes currently held.
    pub bytes: u64,
}

/// What recording one revision did: stored (possibly reusing a blob), or
/// why nothing was stored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, TS)]
pub struct RecordOutcome {
    pub stored: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub rev: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub deduped: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub reason: Option<RevisionSkipReason>,
}

/// One file of a replace batch and the revision holding its prior content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, TS)]
pub struct BatchFile {
    pub rel: String,
    pub rev: String,
}

/// How the pre-compile panel came up: on its own after a compile, or at the
/// user's request (the status-bar chip or Tools > Show Pre-compile Warnings).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, TS)]
#[serde(rename_all = "kebab-case")]
pub enum PrecheckPanelVia {
    Auto,
    Request,
}

/// Every fact the app reports, tagged by `action`. Paths are as the
/// emitting surface holds them; `compile.problem` is root-relative.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema, TS)]
#[serde(tag = "action", rename_all_fields = "camelCase")]
pub enum AppEvent {
    #[serde(rename = "file.preview")]
    FilePreview { path: String },
    #[serde(rename = "file.switch")]
    FileSwitch { path: String, dirty: bool },
    #[serde(rename = "file.load")]
    FileLoad { path: String },
    #[serde(rename = "file.too-large")]
    FileTooLarge { path: String, bytes: u64 },
    #[serde(rename = "file.open")]
    FileOpen { path: String, chars: u64 },
    #[serde(rename = "file.load-failed")]
    FileLoadFailed { path: String, error: String },
    #[serde(rename = "file.save-blocked")]
    FileSaveBlocked {
        path: String,
        reason: SaveBlockedReason,
    },
    #[serde(rename = "file.save")]
    FileSave {
        path: String,
        chars: u64,
        mode: SaveMode,
    },
    /// An implicit save failed; the buffer stays dirty.
    #[serde(rename = "file.save-failed")]
    FileSaveFailed {
        path: String,
        trigger: SaveTrigger,
        error: String,
    },
    #[serde(rename = "file.close")]
    FileClose { path: String },
    #[serde(rename = "file.close-many")]
    FileCloseMany {
        count: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        kept: Option<String>,
    },
    #[serde(rename = "file.create")]
    FileCreate { path: String },
    #[serde(rename = "file.create-failed")]
    FileCreateFailed {
        dir: String,
        name: String,
        error: String,
    },
    #[serde(rename = "file.rename")]
    FileRename { from: String, to: String },
    #[serde(rename = "file.rename-failed")]
    FileRenameFailed { from: String, error: String },
    #[serde(rename = "file.reload")]
    FileReload { path: String, chars: u64 },
    #[serde(rename = "file.reload-failed")]
    FileReloadFailed { path: String, error: String },
    /// An open file changed on disk while its buffer has unsaved edits.
    #[serde(rename = "file.external-conflict")]
    FileExternalConflict { path: String },
    /// The user kept their edits over a disk change; the next save wins.
    #[serde(rename = "file.keep-mine")]
    FileKeepMine { path: String },
    #[serde(rename = "file.delete")]
    FileDelete { path: String },
    #[serde(rename = "file.delete-failed")]
    FileDeleteFailed { path: String, error: String },
    #[serde(rename = "file.undo-delete")]
    FileUndoDelete { path: Option<String> },
    #[serde(rename = "file.undo-delete-failed")]
    FileUndoDeleteFailed { error: String },

    #[serde(rename = "main.set")]
    MainSet { main_file: Option<String> },
    #[serde(rename = "main.resolved")]
    MainResolved {
        root: String,
        main_file: Option<String>,
    },
    #[serde(rename = "project.open")]
    ProjectOpen { root: String },
    #[serde(rename = "project.open-refused")]
    ProjectOpenRefused { root: String, error: String },
    #[serde(rename = "project.open-cancelled")]
    ProjectOpenCancelled {},
    #[serde(rename = "tree.load")]
    TreeLoad { rows: u32, ms: u64, deep: bool },
    #[serde(rename = "tree.load-failed")]
    TreeLoadFailed { dir: String, error: String },
    #[serde(rename = "fs.external")]
    FsExternal { change: WatchChange, path: String },
    #[serde(rename = "fs.external-delete")]
    FsExternalDelete { path: String },
    /// Live tracking could not start: external changes need a manual reload.
    #[serde(rename = "fs.watch-unavailable")]
    FsWatchUnavailable { root: String, error: String },
    #[serde(rename = "command.blocked")]
    CommandBlocked {
        command: String,
        reason: CommandBlockedReason,
    },

    #[serde(rename = "compile.start")]
    CompileStart { target: String },
    /// A save started a compile (auto-compile on save).
    #[serde(rename = "compile.auto")]
    CompileAuto { target: String },
    #[serde(rename = "compile.one-off")]
    CompileOneOff { target: String },
    #[serde(rename = "compile.warm")]
    CompileWarm { target: String },
    #[serde(rename = "compile.warm-skipped")]
    CompileWarmSkipped {
        reason: WarmSkippedReason,
        target: String,
    },
    #[serde(rename = "compile.blocked")]
    CompileBlocked {
        reason: CompileBlockedReason,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        target: Option<String>,
    },
    #[serde(rename = "compile.refused")]
    CompileRefused {
        reason: CompileRefusedReason,
        target: String,
    },
    #[serde(rename = "compile.persist-failed")]
    CompilePersistFailed {
        target: Option<String>,
        error: String,
    },
    /// A dependency problem found before compiling; `path` is root-relative.
    #[serde(rename = "compile.precheck")]
    CompilePrecheck {
        target: String,
        #[serde(flatten)]
        finding: Finding,
    },
    /// The pre-compile checks could not run; the compile goes ahead.
    #[serde(rename = "compile.precheck-failed")]
    CompilePrecheckFailed { target: String, error: String },
    /// The pre-compile panel opened over `count` findings for `target`.
    #[serde(rename = "precheck.panel-shown")]
    PrecheckPanelShown {
        target: String,
        count: u32,
        via: PrecheckPanelVia,
    },
    /// The pre-compile panel closed; `dontShowAgain` turned its popup off.
    #[serde(rename = "precheck.panel-dismissed")]
    PrecheckPanelDismissed { dont_show_again: bool },
    /// The setting that lets the panel open on its own changed.
    #[serde(rename = "precheck.popup-setting")]
    PrecheckPopupSetting { on: bool },
    /// The engine entered a phase of the run.
    #[serde(rename = "compile.phase")]
    CompilePhase {
        phase: CompilePhase,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        detail: Option<String>,
    },
    /// The engine fetched a bundle file, or failed to.
    #[serde(rename = "compile.fetch")]
    CompileFetch { file: String, outcome: FetchOutcome },
    /// A dependency the compile lacked, and why.
    #[serde(rename = "compile.missing")]
    CompileMissing {
        target: String,
        file: Option<String>,
        reason: MissingReason,
    },
    #[serde(rename = "offline.readiness")]
    OfflineReadiness {
        root: String,
        state: OfflineState,
        needs: Vec<String>,
    },
    /// The readiness record could not be read: the badge shows nothing.
    #[serde(rename = "offline.readiness-failed")]
    OfflineReadinessFailed { root: String, error: String },
    /// The project index was built: files listed and the time it took.
    #[serde(rename = "index.open")]
    IndexOpen { files: u32, ms: u64 },
    /// The project index could not be built or updated.
    #[serde(rename = "index.failed")]
    IndexFailed { root: String, error: String },
    #[serde(rename = "compile.engine-line")]
    CompileEngineLine { stream: CompileStream },
    #[serde(rename = "compile.progress")]
    CompileProgress { target: String, elapsed_ms: u64 },
    #[serde(rename = "compile.finish")]
    CompileFinish {
        target: String,
        ok: bool,
        ms: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        pdf_url: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        reason: Option<CompileFailure>,
    },
    /// One engine diagnostic; `rootId` names the session root its path is
    /// relative to (absent when the compile had no root).
    #[serde(rename = "compile.problem")]
    CompileProblem {
        root_id: Option<String>,
        #[serde(flatten)]
        diagnostic: Diagnostic,
    },
    #[serde(rename = "compile.frame-probe")]
    CompileFrameProbe { max_frame_ms: u64 },
    #[serde(rename = "compile.cancel-failed")]
    CompileCancelFailed { error: String },
    #[serde(rename = "compile.clean")]
    CompileClean {
        removed: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        target: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        reason: Option<CleanSkippedReason>,
    },
    #[serde(rename = "compile.clean-failed")]
    CompileCleanFailed { target: String, error: String },

    #[serde(rename = "preview.update")]
    PreviewUpdate { pdf_url: String },
    #[serde(rename = "preview.pdf-load")]
    PreviewPdfLoad { pages: u32, ms: u64 },
    /// The pdf changed on disk without this app compiling it (an agent's
    /// compile, say); the preview reloaded it.
    #[serde(rename = "preview.external-update")]
    PreviewExternalUpdate { pdf_url: String },
    /// The pdf's stamp could not be read: outside rewrites go unnoticed.
    #[serde(rename = "preview.stamp-failed")]
    PreviewStampFailed { error: String },
    /// The pdf could not be opened in the preview: its worker failed to load.
    #[serde(rename = "preview.load-failed")]
    PreviewLoadFailed { error: String },
    /// A new project was made from a template.
    #[serde(rename = "template.create")]
    TemplateCreate { template: String, root: String },
    #[serde(rename = "template.create-failed")]
    TemplateCreateFailed { template: String, error: String },
    /// A project or folder was saved as a user template.
    #[serde(rename = "template.save")]
    TemplateSave { id: String, name: String },
    #[serde(rename = "template.save-failed")]
    TemplateSaveFailed { name: String, error: String },
    /// The template list could not be read, or some user templates could not.
    #[serde(rename = "template.list-failed")]
    TemplateListFailed { error: String },
    /// The welcome project opened.
    #[serde(rename = "template.welcome")]
    TemplateWelcome { root: String },
    #[serde(rename = "export.done")]
    ExportDone {
        kind: ExportKind,
        path: String,
        bytes: u64,
    },
    #[serde(rename = "export.failed")]
    ExportFailed { kind: ExportKind, error: String },
    /// The preview zoom changed; `percent` is what a page now shows at.
    #[serde(rename = "preview.zoom")]
    PreviewZoom { mode: ZoomKind, percent: u32 },
    /// A page bitmap was drawn; `width` x `height` is its canvas in device px.
    #[serde(rename = "preview.page-render")]
    PreviewPageRender {
        page: u32,
        ms: u64,
        width: u32,
        height: u32,
    },
    #[serde(rename = "synctex.forward")]
    SynctexForward { page: u32 },
    #[serde(rename = "synctex.inverse")]
    SynctexInverse { path: String, line: u32 },
    #[serde(rename = "synctex.no-match")]
    SynctexNoMatch {
        direction: SyncDirection,
        compiling: bool,
    },
    #[serde(rename = "synctex.outside")]
    SynctexOutside { direction: SyncDirection },
    #[serde(rename = "synctex.failed")]
    SynctexFailed {
        direction: SyncDirection,
        error: String,
    },

    #[serde(rename = "revision.record")]
    RevisionRecord {
        rel: String,
        revisions: u32,
        stored: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        rev: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        deduped: Option<bool>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        reason: Option<RevisionSkipReason>,
    },
    #[serde(rename = "revision.restore")]
    RevisionRestore {
        rel: String,
        rev: String,
        chars: u64,
    },
    /// A replace across the project was applied: files it changed (written
    /// or edited in open buffers), replacements, and the batch undo restores.
    /// Go to definition ran: what was looked up and how many definitions
    /// it has (0: undefined; 2+: a duplicate).
    #[serde(rename = "nav.definition")]
    NavDefinition {
        kind: String,
        key: String,
        found: u32,
    },
    #[serde(rename = "replace.apply")]
    ReplaceApply {
        files: u32,
        replacements: u32,
        batch: String,
    },
    /// A replace was undone: the files put back.
    #[serde(rename = "replace.undo")]
    ReplaceUndo { batch: String, files: u32 },
    /// A replace or its undo did not happen (a stale plan, an unreadable
    /// revision).
    #[serde(rename = "replace.failed")]
    ReplaceFailed { error: String },
    #[serde(rename = "revision.restore-unavailable")]
    RevisionRestoreUnavailable { rel: String, rev: String },

    #[serde(rename = "outline.parse")]
    OutlineParse { entries: u32, ms: u64 },
    #[serde(rename = "editor.render")]
    EditorRender { bytes: u64, ms: u64 },
    #[serde(rename = "log.open")]
    LogOpen {
        path: String,
        max_events: u32,
        max_line_bytes: u32,
    },
    /// The widgets of a compiled main file were read: how many the document
    /// declares (sidecar and pdf annotations agreeing).
    #[serde(rename = "widgets.read")]
    WidgetsRead { main: String, count: u32 },
    /// The widget list could not be produced (never compiled, a stale or
    /// malformed sidecar, a bad bundle manifest).
    #[serde(rename = "widgets.failed")]
    WidgetsFailed { main: String, error: String },
    /// An automation-surface tool call and its outcome, logged with actor
    /// `agent` through the same writer as the app's events.
    #[serde(rename = "mcp.call")]
    McpCall {
        tool: String,
        ok: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        error: Option<String>,
    },
}

/// One event on the app bus.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema, TS)]
pub struct BusEvent {
    /// Epoch milliseconds.
    pub at: u64,
    pub scope: EventScope,
    pub kind: EventKind,
    pub actor: Actor,
    pub message: String,
    pub event: AppEvent,
}

/// One line of the JSONL event log: a bus event, or (when its payload
/// would break the line ceiling) the event with its payload reduced to the
/// action name in `dropped`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema, TS)]
pub struct LogLine {
    pub at: u64,
    pub scope: EventScope,
    pub kind: EventKind,
    pub actor: Actor,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub event: Option<AppEvent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub dropped: Option<String>,
}

/// The generated TypeScript module for this catalog
/// (`src/lib/generated/events.ts`).
pub fn typescript() -> String {
    let cfg = Config::new().with_large_int("number");
    let decls = [
        Actor::decl(&cfg),
        EventScope::decl(&cfg),
        EventKind::decl(&cfg),
        SaveMode::decl(&cfg),
        SaveBlockedReason::decl(&cfg),
        CommandBlockedReason::decl(&cfg),
        CompileBlockedReason::decl(&cfg),
        CompileRefusedReason::decl(&cfg),
        CompileFailure::decl(&cfg),
        WarmSkippedReason::decl(&cfg),
        CleanSkippedReason::decl(&cfg),
        RevisionSkipReason::decl(&cfg),
        SaveTrigger::decl(&cfg),
        WatchChange::decl(&cfg),
        SyncDirection::decl(&cfg),
        CompileStream::decl(&cfg),
        CompileLine::decl(&cfg),
        CompileReport::decl(&cfg),
        ZoomKind::decl(&cfg),
        ExportKind::decl(&cfg),
        OfflineState::decl(&cfg),
        OfflineReadiness::decl(&cfg),
        PrecheckPanelVia::decl(&cfg),
        Revision::decl(&cfg),
        RetentionInfo::decl(&cfg),
        RecordOutcome::decl(&cfg),
        BatchFile::decl(&cfg),
        AppEvent::decl(&cfg),
        BusEvent::decl(&cfg),
        LogLine::decl(&cfg),
    ];
    let mut out = String::from(
        "// Generated from src-tauri/events (maleficium-events). Do not edit:\n\
         // change the Rust types, then run\n\
         //   MALEFICIUM_WRITE_TS=1 cargo test --manifest-path src-tauri/Cargo.toml --workspace\n\n\
         import type {\n  CheckKind,\n  CompilePhase,\n  FetchOutcome,\n  LineSignal,\n  MissingDependency,\n  MissingReason,\n  Severity,\n} from './structure';\n",
    );
    for d in decls {
        out.push_str("\nexport ");
        out.push_str(&d);
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typescript_bindings_are_fresh() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../src/lib/generated/events.ts");
        let want = typescript();
        if std::env::var_os("MALEFICIUM_WRITE_TS").is_some() {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, &want).unwrap();
        }
        let have = std::fs::read_to_string(&path).unwrap_or_default();
        assert!(
            have == want,
            "{} is stale: run MALEFICIUM_WRITE_TS=1 cargo test --manifest-path src-tauri/Cargo.toml --workspace",
            path.display()
        );
    }

    #[test]
    fn events_round_trip_with_their_action_tag() {
        let e = BusEvent {
            at: 1,
            scope: EventScope::Compile,
            kind: EventKind::Error,
            actor: Actor::User,
            message: "m".into(),
            event: AppEvent::CompileProblem {
                root_id: Some("r".into()),
                diagnostic: maleficium_structure::diagnostics("error: a.tex:3: x", "/p", "/p")
                    .remove(0),
            },
        };
        let json = serde_json::to_value(&e).unwrap();
        assert_eq!(json["event"]["action"], "compile.problem");
        assert_eq!(json["event"]["rootId"], "r");
        assert_eq!(json["event"]["path"], "a.tex");
        assert_eq!(json["actor"], "user");
        let back: BusEvent = serde_json::from_value(json).unwrap();
        assert_eq!(back, e);
    }

    #[test]
    fn optional_fields_are_omitted_and_unit_events_carry_only_the_tag() {
        let v = serde_json::to_value(AppEvent::FileCloseMany {
            count: 2,
            kept: None,
        })
        .unwrap();
        assert_eq!(
            v,
            serde_json::json!({"action": "file.close-many", "count": 2})
        );
        let v = serde_json::to_value(AppEvent::ProjectOpenCancelled {}).unwrap();
        assert_eq!(v, serde_json::json!({"action": "project.open-cancelled"}));
        let v = serde_json::to_value(AppEvent::MainSet { main_file: None }).unwrap();
        assert_eq!(
            v,
            serde_json::json!({"action": "main.set", "mainFile": null})
        );
    }
}
