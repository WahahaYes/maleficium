// Generated from src-tauri/events (maleficium-events). Do not edit:
// change the Rust types, then run
//   MALEFICIUM_WRITE_TS=1 cargo test --manifest-path src-tauri/Cargo.toml --workspace

import type {
  CheckKind,
  CompilePhase,
  FetchOutcome,
  LineSignal,
  MissingDependency,
  MissingReason,
  Severity,
} from './structure';

export type Actor = "user" | "agent" | "system";

export type EventScope = "compile" | "preview" | "fs" | "app";

export type EventKind = "info" | "progress" | "success" | "error" | "warn";

export type SaveMode = "manual" | "untitled" | "auto";

export type SaveBlockedReason = "large-placeholder";

export type CommandBlockedReason = "no-project" | "not-a-project-file";

export type CompileBlockedReason = "large-placeholder" | "non-text-selection" | "not-a-tex-file";

export type CompileRefusedReason = "editor-does-not-own-target";

export type CompileFailure = "spawn-failed" | "engine-error" | "missing-dependency";

export type WarmSkippedReason = "no-cached-output";

export type CleanSkippedReason = "no-project-file";

export type RevisionSkipReason = "not-text" | "too-large" | "unchanged" | "unavailable" | "index-unreadable";

export type SaveTrigger = "switch" | "close" | "auto";

export type WatchChange = "create" | "modify" | "delete";

export type SyncDirection = "forward" | "inverse";

export type CompileStream = "stdout" | "stderr" | "status";

export type CompileLine = { stream: CompileStream, text: string, signal?: LineSignal, };

export type CompileReport = { pdfUrl: string | null, failure: CompileFailure | null, missing: MissingDependency | null, message: string, };

export type ZoomKind = "fit-width" | "fit-page" | "percent";

export type ExportKind = "pdf" | "zip";

export type OfflineState = "ready" | "needs-network" | "needs-tool" | "needs-font" | "blocked" | "unverified";

export type OfflineReadiness = { state: OfflineState, needs: Array<string>, missing: MissingDependency | null, };

export type Revision = { rev: string, 
/**
 * Milliseconds since the Unix epoch.
 */
at: number, bytes: number, 
/**
 * The replace batch this revision was taken for.
 */
batch?: string, };

export type RetentionInfo = { maxRevisionsPerFile: number, maxHistoryBytesPerProject: number, minRevisionsKeptPerFile: number, snapshotMaxFileBytes: number, 
/**
 * Revisions currently held across the project.
 */
revisions: number, 
/**
 * Summed distinct blob bytes currently held.
 */
bytes: number, };

export type RecordOutcome = { stored: boolean, rev?: string, deduped?: boolean, reason?: RevisionSkipReason, };

export type BatchFile = { rel: string, rev: string, };

export type AppEvent = { "action": "file.preview", path: string, } | { "action": "file.switch", path: string, dirty: boolean, } | { "action": "file.load", path: string, } | { "action": "file.too-large", path: string, bytes: number, } | { "action": "file.open", path: string, chars: number, } | { "action": "file.load-failed", path: string, error: string, } | { "action": "file.save-blocked", path: string, reason: SaveBlockedReason, } | { "action": "file.save", path: string, chars: number, mode: SaveMode, } | { "action": "file.save-failed", path: string, trigger: SaveTrigger, error: string, } | { "action": "file.close", path: string, } | { "action": "file.close-many", count: number, kept?: string, } | { "action": "file.create", path: string, } | { "action": "file.create-failed", dir: string, name: string, error: string, } | { "action": "file.rename", from: string, to: string, } | { "action": "file.rename-failed", from: string, error: string, } | { "action": "file.reload", path: string, chars: number, } | { "action": "file.reload-failed", path: string, error: string, } | { "action": "file.delete", path: string, } | { "action": "file.delete-failed", path: string, error: string, } | { "action": "file.undo-delete", path: string | null, } | { "action": "file.undo-delete-failed", error: string, } | { "action": "main.set", mainFile: string | null, } | { "action": "main.resolved", root: string, mainFile: string | null, } | { "action": "project.open", root: string, } | { "action": "project.open-refused", root: string, error: string, } | { "action": "project.open-cancelled", } | { "action": "tree.load", rows: number, ms: number, deep: boolean, } | { "action": "tree.load-failed", dir: string, error: string, } | { "action": "fs.external", change: WatchChange, path: string, } | { "action": "fs.external-delete", path: string, } | { "action": "fs.watch-unavailable", root: string, error: string, } | { "action": "command.blocked", command: string, reason: CommandBlockedReason, } | { "action": "compile.start", target: string, } | { "action": "compile.auto", target: string, } | { "action": "compile.one-off", target: string, } | { "action": "compile.warm", target: string, } | { "action": "compile.warm-skipped", reason: WarmSkippedReason, target: string, } | { "action": "compile.blocked", reason: CompileBlockedReason, target?: string, } | { "action": "compile.refused", reason: CompileRefusedReason, target: string, } | { "action": "compile.persist-failed", target: string | null, error: string, } | { "action": "compile.precheck", target: string, kind: CheckKind, 
/**
 * The package, class, tool or font the document asks for.
 */
name: string, 
/**
 * Root-relative file and 1-based line of the request.
 */
path: string, line: number, 
/**
 * A change that removes the need, when one exists.
 */
suggestion?: string, } | { "action": "compile.precheck-failed", target: string, error: string, } | { "action": "compile.phase", phase: CompilePhase, detail?: string, } | { "action": "compile.fetch", file: string, outcome: FetchOutcome, } | { "action": "compile.missing", target: string, file: string | null, reason: MissingReason, } | { "action": "offline.readiness", root: string, state: OfflineState, needs: Array<string>, } | { "action": "offline.readiness-failed", root: string, error: string, } | { "action": "index.open", files: number, ms: number, } | { "action": "index.failed", root: string, error: string, } | { "action": "compile.engine-line", stream: CompileStream, } | { "action": "compile.progress", target: string, elapsedMs: number, } | { "action": "compile.finish", target: string, ok: boolean, ms: number, pdfUrl?: string, reason?: CompileFailure, } | { "action": "compile.problem", rootId: string | null, 
/**
 * Root-relative source path; `None` when `external`.
 */
path?: string, line: number, message: string, severity: Severity, 
/**
 * The source resolves outside the project root.
 */
external: boolean, } | { "action": "compile.frame-probe", maxFrameMs: number, } | { "action": "compile.cancel-failed", error: string, } | { "action": "compile.clean", removed: number, target?: string, reason?: CleanSkippedReason, } | { "action": "compile.clean-failed", target: string, error: string, } | { "action": "preview.update", pdfUrl: string, } | { "action": "preview.pdf-load", pages: number, ms: number, } | { "action": "preview.external-update", pdfUrl: string, } | { "action": "preview.stamp-failed", error: string, } | { "action": "template.create", template: string, root: string, } | { "action": "template.create-failed", template: string, error: string, } | { "action": "template.save", id: string, name: string, } | { "action": "template.save-failed", name: string, error: string, } | { "action": "template.list-failed", error: string, } | { "action": "template.welcome", root: string, } | { "action": "export.done", kind: ExportKind, path: string, bytes: number, } | { "action": "export.failed", kind: ExportKind, error: string, } | { "action": "preview.zoom", mode: ZoomKind, percent: number, } | { "action": "preview.page-render", page: number, ms: number, } | { "action": "synctex.forward", page: number, } | { "action": "synctex.inverse", path: string, line: number, } | { "action": "synctex.no-match", direction: SyncDirection, compiling: boolean, } | { "action": "synctex.outside", direction: SyncDirection, } | { "action": "synctex.failed", direction: SyncDirection, error: string, } | { "action": "revision.record", rel: string, revisions: number, stored: boolean, rev?: string, deduped?: boolean, reason?: RevisionSkipReason, } | { "action": "revision.restore", rel: string, rev: string, chars: number, } | { "action": "nav.definition", kind: string, key: string, found: number, } | { "action": "replace.apply", files: number, replacements: number, batch: string, } | { "action": "replace.undo", batch: string, files: number, } | { "action": "replace.failed", error: string, } | { "action": "revision.restore-unavailable", rel: string, rev: string, } | { "action": "outline.parse", entries: number, ms: number, } | { "action": "editor.render", bytes: number, ms: number, } | { "action": "log.open", path: string, maxEvents: number, maxLineBytes: number, };

export type BusEvent = { 
/**
 * Epoch milliseconds.
 */
at: number, scope: EventScope, kind: EventKind, actor: Actor, message: string, event: AppEvent, };

export type LogLine = { at: number, scope: EventScope, kind: EventKind, actor: Actor, message: string, event?: AppEvent, dropped?: string, };
