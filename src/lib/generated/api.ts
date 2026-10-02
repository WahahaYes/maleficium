// Generated from src-tauri/core (maleficium-core). Do not edit:
// change the Rust types, then run
//   MALEFICIUM_WRITE_TS=1 cargo test --manifest-path src-tauri/Cargo.toml --workspace

import type { BatchFile, BundleProfile, BusEvent, OfflineReadiness, RecordOutcome, RetentionInfo, Revision, WatchChange } from './events';
import type { FileMatch, Lookup, Query, Ranked, ReplaceApplied, ReplacePreview, SearchResult } from './index';
import type { Diagnostic, Finding, Outline } from './structure';

export type ProjectGrant = { path: string, rootId: string, };

export type Exported = { 
/**
 * The file written.
 */
path: string, bytes: number, 
/**
 * Root-relative paths packed (zip only; empty for a pdf).
 */
files: Array<string>, };

export type OutputStamp = { mtimeMs: number, bytes: number, };

export type FileEntry = { name: string, entryType: string, };

export type FileStat = { size: number, isFile: boolean, isDir: boolean, };

export type TemplateInfo = { id: string, name: string, description: string, category: string, 
/**
 * Root-relative main file.
 */
main: string, 
/**
 * Saved or imported by the user (under app data), not bundled.
 */
user: boolean, };

export type TemplateList = { templates: Array<TemplateInfo>, 
/**
 * User template folders whose manifest could not be read.
 */
unreadable: Array<string>, };

export type Created = { 
/**
 * Absolute path of the new project folder.
 */
root: string, 
/**
 * Root-relative main file.
 */
main: string, };

export type ForwardHit = { page: number | null, };

export type InverseHit = { relPath: string | null, line: number | null, };

export type Precheck = { source: string, revision: string, main: string, findings: Array<Finding>, 
/**
 * Packages were checked against the cached bundle index (false until
 * the first compile has cached it).
 */
bundleChecked: boolean, 
/**
 * Fonts were checked against this machine's font database.
 */
fontsChecked: boolean, };

export type MainResolution = { main: string | null, source: MainSource, candidates: Array<string>, };

export type MainSource = "config" | "magic" | "scan" | "single" | "none";

export type GrantProjectParams = { root: string, };

export type GrantUntitledParams = Record<symbol, never>;

export type CompileDiagnosticsParams = { rootId: string, mainRel: string, };

export type OfflineReadinessParams = { rootId: string, };

export type PrecompileChecksParams = { rootId: string, mainRel: string, };

export type OutputStampParams = { rootId: string, mainRel: string, };

export type OutputPdfParams = { rootId: string, mainRel: string, };

export type ExportPdfParams = { rootId: string, mainRel: string, dest: string, };

export type ExportZipParams = { rootId: string, dest: string, };

export type OutputsFreshParams = { rootId: string, mainRel: string, };

export type CleanOutputsParams = { rootId: string, mainRel: string, };

export type TemplatesListParams = Record<symbol, never>;

export type TemplateInstantiateParams = { template: string, parent: string, name: string, };

export type TemplateSaveProjectParams = { rootId: string, info: TemplateInfo, };

export type TemplateImportFolderParams = { dir: string, info: TemplateInfo, };

export type TemplateWelcomeParams = Record<symbol, never>;

export type InteractiveInstallParams = { rootId: string, };

export type Installed = { file: string, };

export type WidgetsParams = { rootId: string, mainRel: string, };

export type WidgetType = "model" | "video" | "table" | "chart" | "html";

export type WidgetRect = { x0: number, y0: number, x1: number, y1: number, };

export type WidgetSource = { role: string, path: string, };

export type WidgetOption = { key: string, value: string, };

export type WidgetCsp = { connectDomains: Array<string>, resourceDomains: Array<string>, frameDomains: Array<string>, };

export type Widget = { id: string, type: WidgetType, 
/**
 * Built-in runtime and major version (`model@1`); absent for `html`.
 */
runtime?: string, 
/**
 * LaTeX label of the enclosing float; absent when the macro sat before
 * the caption (recorded empty rather than guessed).
 */
label?: string, 
/**
 * The float's rendered number, absent with the label.
 */
figure?: string, theme: string, 
/**
 * Poster image path; absent for a table (the typeset rows are it).
 */
poster?: string, sources: Array<WidgetSource>, options: Array<WidgetOption>, alt: string, 
/**
 * 1-based page of the annotation.
 */
page: number, rect: WidgetRect, 
/**
 * Declared extra origins; absent when the widget declares none.
 */
csp?: WidgetCsp, };

export type WidgetList = { widgets: Array<Widget>, };

export type WidgetApproval = { 
/**
 * Widgets may run live (otherwise they stay posters).
 */
run: boolean, 
/**
 * Live widgets may reach the origins they declare.
 */
network: boolean, };

export type PosterReason = "not-approved" | "no-runtime";

export type WidgetPlanEntry = { id: string, 
/**
 * Absent when the widget can run.
 */
poster?: PosterReason, 
/**
 * The widget declares origins it wants to reach.
 */
declaresNetwork: boolean, 
/**
 * It may reach them (declared and approved).
 */
network: boolean, };

export type WidgetSession = { approval: WidgetApproval, entries: Array<WidgetPlanEntry>, unavailable: Array<string>, };

export type ExportBundleParams = { rootId: string, mainRel: string, dest: string, profile: BundleProfile, sizeCapBytes: number | null, };

export type BundleWarningKind = "size-cap" | "unfolded-files" | "external-reference" | "fold-limit" | "metadata";

export type BundleWarning = { kind: BundleWarningKind, message: string, };

export type BundleExported = { 
/**
 * The folder (folder, hosted) or the file (single-file) written.
 */
path: string, profile: BundleProfile, 
/**
 * Total bytes written.
 */
bytes: number, widgets: number, assets: number, warnings: Array<BundleWarning>, };

export type ForwardSyncParams = { rootId: string, mainRel: string, texRel: string, line: number, };

export type InverseSyncParams = { rootId: string, mainRel: string, page: number, x: number, y: number, };

export type StructureOutlineParams = { text: string, };

export type StructureDiagnosticsParams = { log: string, root: string, base: string, };

export type HistoryRecordParams = { rootId: string, rel: string, text: string, };

export type HistoryListParams = { rootId: string, rel: string, };

export type HistoryRetentionParams = { rootId: string, };

export type HistoryBatchFilesParams = { rootId: string, batch: string, };

export type IndexOpenParams = { rootId: string, };

export type IndexWatchedParams = { rootId: string, watched: boolean, };

export type IndexTouchParams = { rootId: string, paths: Array<string>, };

export type WatchStartParams = { rootId: string, };

export type WatchPollParams = { rootId: string, };

export type WatchStopParams = { rootId: string, };

export type WatchEvent = { rel: string, change: WatchChange, };

export type IndexOverlayParams = { rootId: string, rel: string, text: string | null, };

export type IndexSearchParams = { rootId: string, query: Query, mainRel: string | null, max: number | null, };

export type IndexFindFilesParams = { rootId: string, query: string, max: number | null, };

export type IndexReplacePreviewParams = { rootId: string, query: Query, replacement: string, mainRel: string | null, };

export type IndexReplaceApplyParams = { rootId: string, token: string, keepOpen: Array<string>, };

export type FuzzyRankParams = { query: string, items: Array<string>, max: number | null, };

export type IndexDefinitionAtParams = { rootId: string, line: string, col: number, mainRel: string | null, };

export type MainResolveParams = { rootId: string, openedAbs: string | null, };

export type MainSetAssociationParams = { rootId: string, rel: string, };

export type FileReadParams = { rootId: string, rel: string, };

export type FileReadBytesParams = { rootId: string, rel: string, };

export type FileWriteParams = { rootId: string, rel: string, text: string, };

export type FileSaveParams = { rootId: string, rel: string, text: string, base: string | null, };

export type FileListParams = { rootId: string, rel: string, };

export type FileRenameParams = { rootId: string, oldRel: string, newRel: string, };

export type FileMkdirParams = { rootId: string, rel: string, };

export type FileRemoveParams = { rootId: string, rel: string, recursive: boolean, };

export type FileStatParams = { rootId: string, rel: string, };

export type FileTrashParams = { rootId: string, rel: string, confirm: string, };

export type FileUndoTrashParams = { rootId: string, trashPath: string, };

export type EventAppendParams = { events: Array<BusEvent>, };

export type EventRotateParams = Record<symbol, never>;

export type RotateReport = { path: string, kept: number, dropped: number, };

export type Request = { "op": "grantProjectAccess", "params": GrantProjectParams } | { "op": "grantUntitledAccess", "params": GrantUntitledParams } | { "op": "compileDiagnostics", "params": CompileDiagnosticsParams } | { "op": "offlineReadiness", "params": OfflineReadinessParams } | { "op": "precompileChecks", "params": PrecompileChecksParams } | { "op": "outputStamp", "params": OutputStampParams } | { "op": "outputPdf", "params": OutputPdfParams } | { "op": "exportPdf", "params": ExportPdfParams } | { "op": "exportZip", "params": ExportZipParams } | { "op": "outputsFresh", "params": OutputsFreshParams } | { "op": "cleanOutputs", "params": CleanOutputsParams } | { "op": "templatesList", "params": TemplatesListParams } | { "op": "templateInstantiate", "params": TemplateInstantiateParams } | { "op": "templateSaveProject", "params": TemplateSaveProjectParams } | { "op": "templateImportFolder", "params": TemplateImportFolderParams } | { "op": "templateWelcome", "params": TemplateWelcomeParams } | { "op": "interactiveInstall", "params": InteractiveInstallParams } | { "op": "widgets", "params": WidgetsParams } | { "op": "exportBundle", "params": ExportBundleParams } | { "op": "forwardSync", "params": ForwardSyncParams } | { "op": "inverseSync", "params": InverseSyncParams } | { "op": "structureOutline", "params": StructureOutlineParams } | { "op": "structureDiagnostics", "params": StructureDiagnosticsParams } | { "op": "historyRecord", "params": HistoryRecordParams } | { "op": "historyList", "params": HistoryListParams } | { "op": "historyRetention", "params": HistoryRetentionParams } | { "op": "historyBatchFiles", "params": HistoryBatchFilesParams } | { "op": "indexOpen", "params": IndexOpenParams } | { "op": "indexWatched", "params": IndexWatchedParams } | { "op": "indexTouch", "params": IndexTouchParams } | { "op": "watchStart", "params": WatchStartParams } | { "op": "watchPoll", "params": WatchPollParams } | { "op": "watchStop", "params": WatchStopParams } | { "op": "indexOverlay", "params": IndexOverlayParams } | { "op": "indexSearch", "params": IndexSearchParams } | { "op": "indexFindFiles", "params": IndexFindFilesParams } | { "op": "indexReplacePreview", "params": IndexReplacePreviewParams } | { "op": "indexReplaceApply", "params": IndexReplaceApplyParams } | { "op": "fuzzyRank", "params": FuzzyRankParams } | { "op": "indexDefinitionAt", "params": IndexDefinitionAtParams } | { "op": "mainResolve", "params": MainResolveParams } | { "op": "mainSetAssociation", "params": MainSetAssociationParams } | { "op": "fileRead", "params": FileReadParams } | { "op": "fileReadBytes", "params": FileReadBytesParams } | { "op": "fileWrite", "params": FileWriteParams } | { "op": "fileSave", "params": FileSaveParams } | { "op": "fileList", "params": FileListParams } | { "op": "fileRename", "params": FileRenameParams } | { "op": "fileMkdir", "params": FileMkdirParams } | { "op": "fileRemove", "params": FileRemoveParams } | { "op": "fileStat", "params": FileStatParams } | { "op": "fileTrash", "params": FileTrashParams } | { "op": "fileUndoTrash", "params": FileUndoTrashParams } | { "op": "eventAppend", "params": EventAppendParams } | { "op": "eventRotate", "params": EventRotateParams };

export type Response = { "op": "grantProjectAccess", "result": ProjectGrant } | { "op": "grantUntitledAccess", "result": ProjectGrant } | { "op": "compileDiagnostics", "result": Array<Diagnostic> } | { "op": "offlineReadiness", "result": OfflineReadiness } | { "op": "precompileChecks", "result": Precheck } | { "op": "outputStamp", "result": OutputStamp | null } | { "op": "outputPdf", "result": string | null } | { "op": "exportPdf", "result": Exported } | { "op": "exportZip", "result": Exported } | { "op": "outputsFresh", "result": boolean } | { "op": "cleanOutputs", "result": number } | { "op": "templatesList", "result": TemplateList } | { "op": "templateInstantiate", "result": Created } | { "op": "templateSaveProject", "result": TemplateInfo } | { "op": "templateImportFolder", "result": TemplateInfo } | { "op": "templateWelcome", "result": Created } | { "op": "interactiveInstall", "result": Installed } | { "op": "widgets", "result": WidgetList } | { "op": "exportBundle", "result": BundleExported } | { "op": "forwardSync", "result": ForwardHit } | { "op": "inverseSync", "result": InverseHit } | { "op": "structureOutline", "result": Outline } | { "op": "structureDiagnostics", "result": Array<Diagnostic> } | { "op": "historyRecord", "result": RecordOutcome } | { "op": "historyList", "result": Array<Revision> } | { "op": "historyRetention", "result": RetentionInfo } | { "op": "historyBatchFiles", "result": Array<BatchFile> } | { "op": "indexOpen", "result": number } | { "op": "indexWatched", "result": null } | { "op": "indexTouch", "result": null } | { "op": "watchStart", "result": null } | { "op": "watchPoll", "result": Array<WatchEvent> } | { "op": "watchStop", "result": null } | { "op": "indexOverlay", "result": null } | { "op": "indexSearch", "result": SearchResult } | { "op": "indexFindFiles", "result": Array<FileMatch> } | { "op": "indexReplacePreview", "result": ReplacePreview } | { "op": "indexReplaceApply", "result": ReplaceApplied } | { "op": "fuzzyRank", "result": Array<Ranked> } | { "op": "indexDefinitionAt", "result": Lookup | null } | { "op": "mainResolve", "result": MainResolution } | { "op": "mainSetAssociation", "result": null } | { "op": "fileRead", "result": string } | { "op": "fileReadBytes", "result": string } | { "op": "fileWrite", "result": null } | { "op": "fileSave", "result": RecordOutcome } | { "op": "fileList", "result": Array<FileEntry> } | { "op": "fileRename", "result": null } | { "op": "fileMkdir", "result": null } | { "op": "fileRemove", "result": null } | { "op": "fileStat", "result": FileStat | null } | { "op": "fileTrash", "result": string } | { "op": "fileUndoTrash", "result": string } | { "op": "eventAppend", "result": null } | { "op": "eventRotate", "result": RotateReport };

/** Params by operation name, derived from `Request`. */
export type OpParams = { [R in Request as R["op"]]: R["params"] };

/** Results by operation name, derived from `Response`. */
export type OpResult = { [R in Response as R["op"]]: R["result"] };
