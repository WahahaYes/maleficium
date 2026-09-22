//! The app event catalog: the one definition of every bus payload.
//!
//! The frontend's TypeScript types are generated from these (see
//! `typescript()`), so the desktop bus, the JSONL event log, MCP tools and
//! any future bridge or MCP App View read one schema. Pure by contract: no
//! fs, no process, no Tauri.

use maleficium_structure::Diagnostic;
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

/// One line of a running compile, as the desktop backend streams it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, TS)]
pub struct CompileLine {
    pub stream: CompileStream,
    pub text: String,
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
    #[serde(rename = "compile.download")]
    CompileDownload { package: String },
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
    #[serde(rename = "preview.page-render")]
    PreviewPageRender { page: u32, ms: u64 },
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
        WatchChange::decl(&cfg),
        SyncDirection::decl(&cfg),
        CompileStream::decl(&cfg),
        CompileLine::decl(&cfg),
        AppEvent::decl(&cfg),
        BusEvent::decl(&cfg),
        LogLine::decl(&cfg),
    ];
    let mut out = String::from(
        "// Generated from src-tauri/events (maleficium-events). Do not edit:\n\
         // change the Rust types, then run\n\
         //   MALEFICIUM_WRITE_TS=1 cargo test --manifest-path src-tauri/Cargo.toml --workspace\n\n\
         import type { Severity } from './structure';\n",
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
