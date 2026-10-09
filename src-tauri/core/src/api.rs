//! One typed operation contract for every adapter: the desktop Tauri
//! command, the future HTTP route, and the curated MCP tools all run these
//! operations. A small `macro_rules!` list names each operation once; it
//! generates the `Request` and `Response` enums plus `dispatch`. Per
//! operation, hand-written: its params struct and its thin handler into the
//! domain modules.
//!
//! Out of scope on purpose: the two streaming compile runs (their child
//! slot and window events stay in the Tauri adapter until the compile
//! service owns them) and the two history byte reads (raw Tauri responses,
//! not JSON). The frontend reaches those through dedicated commands.

use crate::Core;

use maleficium_events::{
    BatchFile, BundleProfile, BusEvent, OfflineReadiness, RecordOutcome, RetentionInfo, Revision,
};
use maleficium_index::definition::Lookup;
use maleficium_index::replace::{ReplaceApplied, ReplacePreview};
use maleficium_index::search::{FileMatch, Query, Ranked, SearchResult};
use maleficium_index::ProjectMacro;
use maleficium_structure::{Diagnostic, Outline};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::bundle::BundleExported;
use crate::eventlog::RotateReport;
use crate::export::Exported;
use crate::fs::FileStat;
use crate::mainfile::{MainResolution, MainSource};
use crate::outputs::OutputStamp;
use crate::presence::{OpenOutcome, OpenRequest};
use crate::structure::Precheck;
use crate::synctex::{ForwardHit, InverseHit};
use crate::templates::{Created, TemplateInfo, TemplateList};
use crate::watch::WatchEvent;
use crate::widget_approval::{WidgetReview, WidgetsStatus};
use crate::widgets::WidgetList;

/// A granted project: its canonical path and the session-root id the other
/// operations take. The Tauri adapter mints the fs-scope grant beside this.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ProjectGrant {
    pub path: String,
    pub root_id: String,
}

macro_rules! params {
    ($($name:ident { $($field:ident: $ty:ty),* $(,)? }),* $(,)?) => {
        $(#[derive(Debug, Clone, Serialize, Deserialize, TS)]
        #[serde(rename_all = "camelCase")]
        pub struct $name {
            $(pub $field: $ty),*
        })*
    };
}

params! {
    GrantProjectParams { root: String },
    GrantUntitledParams {},
    CompileDiagnosticsParams { root_id: String, main_rel: String },
    OfflineReadinessParams { root_id: String },
    PrecompileChecksParams { root_id: String, main_rel: String },
    OutputStampParams { root_id: String, main_rel: String },
    OutputPdfParams { root_id: String, main_rel: String },
    ExportPdfParams { root_id: String, main_rel: String, dest: String },
    ExportZipParams { root_id: String, dest: String },
    OutputsFreshParams { root_id: String, main_rel: String },
    CleanOutputsParams { root_id: String, main_rel: String },
    TemplatesListParams {},
    TemplateInstantiateParams { template: String, parent: String, name: String },
    TemplateSaveProjectParams { root_id: String, info: TemplateInfo },
    TemplateImportFolderParams { dir: String, info: TemplateInfo },
    TemplateWelcomeParams {},
    WidgetsParams { root_id: String, main_rel: String },
    WidgetsStatusParams { root_id: String, main_rel: String },
    WidgetReviewParams { root_id: String, main_rel: String, widget: String },
    ExportBundleParams { root_id: String, main_rel: String, dest: String, profile: BundleProfile, size_cap_bytes: Option<u64> },
    ExportCancelParams {},
    ForwardSyncParams { root_id: String, main_rel: String, tex_rel: String, line: u32 },
    InverseSyncParams { root_id: String, main_rel: String, page: u32, x: f32, y: f32 },
    StructureOutlineParams { text: String },
    StructureDiagnosticsParams { log: String, root: String, base: String },
    HistoryRecordParams { root_id: String, rel: String, text: String },
    HistoryListParams { root_id: String, rel: String },
    HistoryRetentionParams { root_id: String },
    HistoryBatchFilesParams { root_id: String, batch: String },
    IndexOpenParams { root_id: String },
    IndexWatchedParams { root_id: String, watched: bool },
    IndexTouchParams { root_id: String, paths: Vec<String> },
    WatchStartParams { root_id: String },
    WatchPollParams { root_id: String },
    WatchStopParams { root_id: String },
    IndexOverlayParams { root_id: String, rel: String, text: Option<String> },
    IndexSearchParams { root_id: String, query: Query, main_rel: Option<String>, max: Option<usize> },
    IndexFindFilesParams { root_id: String, query: String, max: Option<usize> },
    IndexReplacePreviewParams { root_id: String, query: Query, replacement: String, main_rel: Option<String> },
    IndexReplaceApplyParams { root_id: String, token: String, keep_open: Vec<String> },
    FuzzyRankParams { query: String, items: Vec<String>, max: Option<usize> },
    IndexDefinitionAtParams { root_id: String, line: String, col: u32, main_rel: Option<String> },
    IndexMacrosParams { root_id: String },
    PresenceSetParams { root_id: Option<String>, main_rel: Option<String>, active_rel: Option<String> },
    PresencePendingParams {},
    PresenceAnswerParams { id: String, outcome: OpenOutcome },
    MainResolveParams { root_id: String, opened_abs: Option<String> },
    MainSetAssociationParams { root_id: String, rel: String },
    FileReadParams { root_id: String, rel: String },
    FileReadBytesParams { root_id: String, rel: String },
    FileWriteParams { root_id: String, rel: String, text: String },
    FileSaveParams { root_id: String, rel: String, text: String, base: Option<String> },
    FileListParams { root_id: String, rel: String },
    FileRenameParams { root_id: String, old_rel: String, new_rel: String },
    FileMkdirParams { root_id: String, rel: String },
    FileRemoveParams { root_id: String, rel: String, recursive: bool },
    FileStatParams { root_id: String, rel: String },
    FileTrashParams { root_id: String, rel: String, confirm: String },
    FileUndoTrashParams { root_id: String, trash_path: String },
    EventAppendParams { events: Vec<BusEvent> },
    EventRotateParams {},
}

macro_rules! operations {
    ($($op:ident via $handler:ident($params:ty) -> $result:ty),* $(,)?) => {
        /// One operation call: which operation plus its params. The future
        /// `POST /api` takes the same JSON.
        #[derive(Debug, Clone, Serialize, Deserialize, TS)]
        #[serde(tag = "op", content = "params", rename_all = "camelCase")]
        pub enum Request {
            $($op($params)),*
        }

        /// One operation result: which operation plus its payload.
        #[derive(Debug, Clone, Serialize, TS)]
        #[serde(tag = "op", content = "result", rename_all = "camelCase")]
        pub enum Response {
            $($op($result)),*
        }

        /// Run one operation against the core. Errors are plain strings, as
        /// the per-command handlers returned before.
        pub fn dispatch(cx: &Core, req: Request) -> Result<Response, String> {
            match req {
                $(Request::$op(p) => $handler(cx, p).map(Response::$op)),*
            }
        }
    };
}

operations! {
    GrantProjectAccess via grant_project_access(GrantProjectParams) -> ProjectGrant,
    GrantUntitledAccess via grant_untitled_access(GrantUntitledParams) -> ProjectGrant,
    CompileDiagnostics via compile_diagnostics(CompileDiagnosticsParams) -> Vec<Diagnostic>,
    OfflineReadiness via offline_readiness(OfflineReadinessParams) -> OfflineReadiness,
    PrecompileChecks via precompile_checks(PrecompileChecksParams) -> Precheck,
    OutputStamp via output_stamp(OutputStampParams) -> Option<OutputStamp>,
    OutputPdf via output_pdf(OutputPdfParams) -> Option<String>,
    ExportPdf via export_pdf(ExportPdfParams) -> Exported,
    ExportZip via export_zip(ExportZipParams) -> Exported,
    OutputsFresh via outputs_fresh(OutputsFreshParams) -> bool,
    CleanOutputs via clean_outputs(CleanOutputsParams) -> usize,
    TemplatesList via templates_list(TemplatesListParams) -> TemplateList,
    TemplateInstantiate via template_instantiate(TemplateInstantiateParams) -> Created,
    TemplateSaveProject via template_save_project(TemplateSaveProjectParams) -> TemplateInfo,
    TemplateImportFolder via template_import_folder(TemplateImportFolderParams) -> TemplateInfo,
    TemplateWelcome via template_welcome(TemplateWelcomeParams) -> Created,
    Widgets via widgets(WidgetsParams) -> WidgetList,
    WidgetsStatus via widgets_status(WidgetsStatusParams) -> WidgetsStatus,
    WidgetReview via widget_review(WidgetReviewParams) -> Box<WidgetReview>,
    ExportBundle via export_bundle(ExportBundleParams) -> BundleExported,
    ExportCancel via export_cancel(ExportCancelParams) -> String,
    ForwardSync via forward_sync(ForwardSyncParams) -> ForwardHit,
    InverseSync via inverse_sync(InverseSyncParams) -> InverseHit,
    StructureOutline via structure_outline(StructureOutlineParams) -> Outline,
    StructureDiagnostics via structure_diagnostics(StructureDiagnosticsParams) -> Vec<Diagnostic>,
    HistoryRecord via history_record(HistoryRecordParams) -> RecordOutcome,
    HistoryList via history_list(HistoryListParams) -> Vec<Revision>,
    HistoryRetention via history_retention(HistoryRetentionParams) -> RetentionInfo,
    HistoryBatchFiles via history_batch_files(HistoryBatchFilesParams) -> Vec<BatchFile>,
    IndexOpen via index_open(IndexOpenParams) -> usize,
    IndexWatched via index_watched(IndexWatchedParams) -> (),
    IndexTouch via index_touch(IndexTouchParams) -> (),
    WatchStart via watch_start(WatchStartParams) -> (),
    WatchPoll via watch_poll(WatchPollParams) -> Vec<WatchEvent>,
    WatchStop via watch_stop(WatchStopParams) -> (),
    IndexOverlay via index_overlay(IndexOverlayParams) -> (),
    IndexSearch via index_search(IndexSearchParams) -> SearchResult,
    IndexFindFiles via index_find_files(IndexFindFilesParams) -> Vec<FileMatch>,
    IndexReplacePreview via index_replace_preview(IndexReplacePreviewParams) -> ReplacePreview,
    IndexReplaceApply via index_replace_apply(IndexReplaceApplyParams) -> ReplaceApplied,
    FuzzyRank via fuzzy_rank(FuzzyRankParams) -> Vec<Ranked>,
    IndexDefinitionAt via index_definition_at(IndexDefinitionAtParams) -> Option<Lookup>,
    IndexMacros via index_macros(IndexMacrosParams) -> Vec<ProjectMacro>,
    PresenceSet via presence_set(PresenceSetParams) -> (),
    PresencePending via presence_pending(PresencePendingParams) -> Option<OpenRequest>,
    PresenceAnswer via presence_answer(PresenceAnswerParams) -> (),
    MainResolve via main_resolve(MainResolveParams) -> MainResolution,
    MainSetAssociation via main_set_association(MainSetAssociationParams) -> (),
    FileRead via file_read(FileReadParams) -> String,
    FileReadBytes via file_read_bytes(FileReadBytesParams) -> String,
    FileWrite via file_write(FileWriteParams) -> (),
    FileSave via file_save(FileSaveParams) -> RecordOutcome,
    FileList via file_list(FileListParams) -> Vec<crate::FileEntry>,
    FileRename via file_rename(FileRenameParams) -> (),
    FileMkdir via file_mkdir(FileMkdirParams) -> (),
    FileRemove via file_remove(FileRemoveParams) -> (),
    FileStat via file_stat(FileStatParams) -> Option<FileStat>,
    FileTrash via file_trash(FileTrashParams) -> String,
    FileUndoTrash via file_undo_trash(FileUndoTrashParams) -> String,
    EventAppend via event_append(EventAppendParams) -> (),
    EventRotate via event_rotate(EventRotateParams) -> RotateReport,
}

fn grant_project_access(cx: &Core, p: GrantProjectParams) -> Result<ProjectGrant, String> {
    let (canon, root_id) = crate::grant_project(cx, &p.root)?;
    Ok(ProjectGrant {
        path: canon.to_string_lossy().to_string(),
        root_id,
    })
}

fn grant_untitled_access(cx: &Core, _p: GrantUntitledParams) -> Result<ProjectGrant, String> {
    let (canon, root_id) = crate::grant_untitled(cx)?;
    Ok(ProjectGrant {
        path: canon.to_string_lossy().to_string(),
        root_id,
    })
}

fn compile_diagnostics(cx: &Core, p: CompileDiagnosticsParams) -> Result<Vec<Diagnostic>, String> {
    Ok(crate::structure::diagnostics(cx, &p.root_id, &p.main_rel, 100)?.diagnostics)
}

fn offline_readiness(cx: &Core, p: OfflineReadinessParams) -> Result<OfflineReadiness, String> {
    crate::compile::offline_readiness(cx, &p.root_id)
}

fn precompile_checks(cx: &Core, p: PrecompileChecksParams) -> Result<Precheck, String> {
    crate::structure::precompile_checks(cx, &p.root_id, &p.main_rel)
}

fn output_stamp(cx: &Core, p: OutputStampParams) -> Result<Option<OutputStamp>, String> {
    crate::output_stamp(cx, &p.root_id, &p.main_rel)
}

fn output_pdf(cx: &Core, p: OutputPdfParams) -> Result<Option<String>, String> {
    crate::output_pdf(cx, &p.root_id, &p.main_rel)
}

fn export_pdf(cx: &Core, p: ExportPdfParams) -> Result<Exported, String> {
    crate::export::export_pdf(cx, &p.root_id, &p.main_rel, &p.dest)
}

fn export_zip(cx: &Core, p: ExportZipParams) -> Result<Exported, String> {
    crate::export::export_zip(cx, &p.root_id, &p.dest)
}

fn outputs_fresh(cx: &Core, p: OutputsFreshParams) -> Result<bool, String> {
    crate::outputs_fresh(cx, &p.root_id, &p.main_rel)
}

fn clean_outputs(cx: &Core, p: CleanOutputsParams) -> Result<usize, String> {
    crate::clean_outputs(cx, &p.root_id, &p.main_rel)
}

fn templates_list(_cx: &Core, _p: TemplatesListParams) -> Result<TemplateList, String> {
    Ok(crate::templates::list())
}

fn template_instantiate(_cx: &Core, p: TemplateInstantiateParams) -> Result<Created, String> {
    crate::templates::instantiate(&p.template, &p.parent, &p.name)
}

fn template_save_project(cx: &Core, p: TemplateSaveProjectParams) -> Result<TemplateInfo, String> {
    crate::templates::save_project(cx, &p.root_id, p.info)
}

fn template_import_folder(
    _cx: &Core,
    p: TemplateImportFolderParams,
) -> Result<TemplateInfo, String> {
    crate::templates::import_folder(&p.dir, p.info)
}

fn template_welcome(_cx: &Core, _p: TemplateWelcomeParams) -> Result<Created, String> {
    crate::templates::welcome()
}

fn export_bundle(cx: &Core, p: ExportBundleParams) -> Result<BundleExported, String> {
    crate::bundle::export_bundle(
        cx,
        &p.root_id,
        &p.main_rel,
        &p.dest,
        p.profile,
        p.size_cap_bytes,
    )
}

/// Stop the HTML conversion of a running bundle export: that export then
/// fails as cancelled and writes nothing.
fn export_cancel(cx: &Core, _p: ExportCancelParams) -> Result<String, String> {
    crate::reflow::convert::cancel(cx)
}

fn widgets(cx: &Core, p: WidgetsParams) -> Result<WidgetList, String> {
    crate::widgets::widgets(cx, &p.root_id, &p.main_rel)
}

/// Read-only: approving, revoking and the auto-approval switch are not
/// operations of this contract (see `widget_approval`).
fn widgets_status(cx: &Core, p: WidgetsStatusParams) -> Result<WidgetsStatus, String> {
    crate::widget_approval::widgets_status(cx, &p.root_id, &p.main_rel)
}

fn widget_review(cx: &Core, p: WidgetReviewParams) -> Result<Box<WidgetReview>, String> {
    crate::widget_approval::review(cx, &p.root_id, &p.main_rel, &p.widget).map(Box::new)
}

fn forward_sync(cx: &Core, p: ForwardSyncParams) -> Result<ForwardHit, String> {
    crate::forward(cx, &p.root_id, &p.main_rel, &p.tex_rel, p.line)
}

fn inverse_sync(cx: &Core, p: InverseSyncParams) -> Result<InverseHit, String> {
    crate::inverse(cx, &p.root_id, &p.main_rel, p.page, p.x, p.y)
}

fn structure_outline(_cx: &Core, p: StructureOutlineParams) -> Result<Outline, String> {
    Ok(maleficium_structure::outline(&p.text))
}

fn structure_diagnostics(
    _cx: &Core,
    p: StructureDiagnosticsParams,
) -> Result<Vec<Diagnostic>, String> {
    Ok(maleficium_structure::diagnostics(&p.log, &p.root, &p.base))
}

fn history_record(cx: &Core, p: HistoryRecordParams) -> Result<RecordOutcome, String> {
    Ok(crate::history::record(
        cx,
        &p.root_id,
        &p.rel,
        p.text.as_bytes(),
    ))
}

fn history_list(cx: &Core, p: HistoryListParams) -> Result<Vec<Revision>, String> {
    Ok(crate::history::list(cx, &p.root_id, &p.rel))
}

fn history_retention(cx: &Core, p: HistoryRetentionParams) -> Result<RetentionInfo, String> {
    Ok(crate::history::retention(cx, &p.root_id))
}

fn history_batch_files(cx: &Core, p: HistoryBatchFilesParams) -> Result<Vec<BatchFile>, String> {
    Ok(crate::history::batch_files(cx, &p.root_id, &p.batch))
}

fn index_open(cx: &Core, p: IndexOpenParams) -> Result<usize, String> {
    crate::index::open(cx, &p.root_id)
}

fn index_watched(cx: &Core, p: IndexWatchedParams) -> Result<(), String> {
    crate::index::set_watched(cx, &p.root_id, p.watched)
}

fn index_touch(cx: &Core, p: IndexTouchParams) -> Result<(), String> {
    crate::index::touch(cx, &p.root_id, &p.paths)
}

fn watch_start(cx: &Core, p: WatchStartParams) -> Result<(), String> {
    crate::watch::start(cx, &p.root_id)
}

fn watch_poll(cx: &Core, p: WatchPollParams) -> Result<Vec<WatchEvent>, String> {
    crate::watch::poll(cx, &p.root_id)
}

fn watch_stop(cx: &Core, p: WatchStopParams) -> Result<(), String> {
    crate::watch::stop(cx, &p.root_id)
}

fn index_overlay(cx: &Core, p: IndexOverlayParams) -> Result<(), String> {
    crate::index::overlay(cx, &p.root_id, &p.rel, p.text)
}

fn index_search(cx: &Core, p: IndexSearchParams) -> Result<SearchResult, String> {
    crate::search::search(
        cx,
        &p.root_id,
        &p.query,
        p.main_rel.as_deref(),
        p.max.unwrap_or(crate::search::MAX_HITS),
    )
}

fn index_find_files(cx: &Core, p: IndexFindFilesParams) -> Result<Vec<FileMatch>, String> {
    crate::search::find_files(
        cx,
        &p.root_id,
        &p.query,
        p.max.unwrap_or(crate::search::MAX_FILE_MATCHES),
    )
}

fn index_replace_preview(
    cx: &Core,
    p: IndexReplacePreviewParams,
) -> Result<ReplacePreview, String> {
    crate::replace::preview(
        cx,
        &p.root_id,
        &p.query,
        &p.replacement,
        p.main_rel.as_deref(),
    )
}

fn index_replace_apply(cx: &Core, p: IndexReplaceApplyParams) -> Result<ReplaceApplied, String> {
    crate::replace::apply(cx, &p.root_id, &p.token, &p.keep_open)
}

fn fuzzy_rank(_cx: &Core, p: FuzzyRankParams) -> Result<Vec<Ranked>, String> {
    Ok(maleficium_index::search::rank(
        &p.query,
        &p.items,
        p.max.unwrap_or(p.items.len()),
    ))
}

fn index_definition_at(cx: &Core, p: IndexDefinitionAtParams) -> Result<Option<Lookup>, String> {
    crate::search::definition_at(cx, &p.root_id, &p.line, p.col, p.main_rel.as_deref())
}

fn index_macros(cx: &Core, p: IndexMacrosParams) -> Result<Vec<ProjectMacro>, String> {
    crate::search::macros(cx, &p.root_id)
}

/// What this app has open, for the MCP server's read view.
fn presence_set(cx: &Core, p: PresenceSetParams) -> Result<(), String> {
    crate::presence::set(cx, p.root_id.as_deref(), p.main_rel, p.active_rel)
}

/// An agent's ask that this window open a project, while the user has not
/// answered it.
fn presence_pending(_cx: &Core, _p: PresencePendingParams) -> Result<Option<OpenRequest>, String> {
    Ok(crate::presence::pending())
}

/// The user's answer to this window's open request.
fn presence_answer(_cx: &Core, p: PresenceAnswerParams) -> Result<(), String> {
    crate::presence::answer(&p.id, p.outcome)
}

fn main_resolve(cx: &Core, p: MainResolveParams) -> Result<MainResolution, String> {
    crate::mainfile::resolve(cx, &p.root_id, p.opened_abs.as_deref())
}

fn main_set_association(cx: &Core, p: MainSetAssociationParams) -> Result<(), String> {
    crate::mainfile::set_association(cx, &p.root_id, &p.rel)
}

fn file_read(cx: &Core, p: FileReadParams) -> Result<String, String> {
    crate::fs::read_text(cx, &p.root_id, &p.rel)
}

fn file_read_bytes(cx: &Core, p: FileReadBytesParams) -> Result<String, String> {
    use base64::Engine as _;
    let bytes = crate::fs::read_bytes(cx, &p.root_id, &p.rel)?;
    Ok(base64::engine::general_purpose::STANDARD.encode(bytes))
}

fn file_write(cx: &Core, p: FileWriteParams) -> Result<(), String> {
    crate::fs::write_bytes(cx, &p.root_id, &p.rel, p.text.as_bytes())
}

fn file_save(cx: &Core, p: FileSaveParams) -> Result<RecordOutcome, String> {
    crate::fs::save(
        cx,
        &p.root_id,
        &p.rel,
        p.text.as_bytes(),
        p.base.as_deref().map(str::as_bytes),
    )
}

fn file_list(cx: &Core, p: FileListParams) -> Result<Vec<crate::FileEntry>, String> {
    crate::fs::list_dir(cx, &p.root_id, &p.rel)
}

fn file_rename(cx: &Core, p: FileRenameParams) -> Result<(), String> {
    crate::fs::rename_path(cx, &p.root_id, &p.old_rel, &p.new_rel)
}

fn file_mkdir(cx: &Core, p: FileMkdirParams) -> Result<(), String> {
    crate::fs::make_dir(cx, &p.root_id, &p.rel)
}

fn file_remove(cx: &Core, p: FileRemoveParams) -> Result<(), String> {
    crate::fs::remove_path(cx, &p.root_id, &p.rel, p.recursive)
}

fn file_stat(cx: &Core, p: FileStatParams) -> Result<Option<FileStat>, String> {
    crate::fs::stat_path(cx, &p.root_id, &p.rel)
}

fn file_trash(cx: &Core, p: FileTrashParams) -> Result<String, String> {
    crate::fs::trash_file(cx, &p.root_id, &p.rel, &p.confirm)
}

fn file_undo_trash(cx: &Core, p: FileUndoTrashParams) -> Result<String, String> {
    crate::fs::undo_trash(cx, &p.root_id, &p.trash_path)
}

fn event_append(_cx: &Core, p: EventAppendParams) -> Result<(), String> {
    crate::eventlog::append(&p.events)
}

fn event_rotate(_cx: &Core, _p: EventRotateParams) -> Result<RotateReport, String> {
    crate::eventlog::rotate()
}

/// The generated TypeScript module for the operation contract
/// (`src/lib/generated/api.ts`). Payload types owned elsewhere are imported
/// from their own modules; only core-owned types are declared here.
pub fn typescript() -> String {
    use ts_rs::{Config, TS};
    let cfg = Config::new().with_large_int("number");
    let decls = [
        ProjectGrant::decl(&cfg),
        Exported::decl(&cfg),
        OutputStamp::decl(&cfg),
        crate::FileEntry::decl(&cfg),
        FileStat::decl(&cfg),
        TemplateInfo::decl(&cfg),
        TemplateList::decl(&cfg),
        Created::decl(&cfg),
        ForwardHit::decl(&cfg),
        InverseHit::decl(&cfg),
        Precheck::decl(&cfg),
        MainResolution::decl(&cfg),
        MainSource::decl(&cfg),
        GrantProjectParams::decl(&cfg),
        GrantUntitledParams::decl(&cfg),
        CompileDiagnosticsParams::decl(&cfg),
        OfflineReadinessParams::decl(&cfg),
        PrecompileChecksParams::decl(&cfg),
        OutputStampParams::decl(&cfg),
        OutputPdfParams::decl(&cfg),
        ExportPdfParams::decl(&cfg),
        ExportZipParams::decl(&cfg),
        OutputsFreshParams::decl(&cfg),
        CleanOutputsParams::decl(&cfg),
        TemplatesListParams::decl(&cfg),
        TemplateInstantiateParams::decl(&cfg),
        TemplateSaveProjectParams::decl(&cfg),
        TemplateImportFolderParams::decl(&cfg),
        TemplateWelcomeParams::decl(&cfg),
        WidgetsParams::decl(&cfg),
        crate::widgets::WidgetType::decl(&cfg),
        crate::widgets::WidgetRect::decl(&cfg),
        crate::widgets::WidgetSource::decl(&cfg),
        crate::widgets::WidgetOption::decl(&cfg),
        crate::widgets::WidgetCsp::decl(&cfg),
        crate::widgets::Widget::decl(&cfg),
        WidgetList::decl(&cfg),
        WidgetsStatusParams::decl(&cfg),
        WidgetReviewParams::decl(&cfg),
        crate::widget_approval::ApprovalKind::decl(&cfg),
        crate::widget_approval::ApprovedVia::decl(&cfg),
        crate::widget_approval::VendoredId::decl(&cfg),
        crate::widget_approval::RuntimeInfo::decl(&cfg),
        crate::widget_approval::WidgetApproved::decl(&cfg),
        crate::widget_approval::ApprovalRequired::decl(&cfg),
        crate::widget_approval::WidgetApprovalStatus::decl(&cfg),
        crate::widget_approval::WidgetUnavailable::decl(&cfg),
        WidgetsStatus::decl(&cfg),
        crate::widget_approval::FileChange::decl(&cfg),
        crate::widget_approval::ReviewFile::decl(&cfg),
        WidgetReview::decl(&cfg),
        crate::widget_approval::WidgetApproveParams::decl(&cfg),
        crate::widget_approval::WidgetRevokeParams::decl(&cfg),
        crate::widget_approval::WidgetAutoApproveParams::decl(&cfg),
        crate::widget_approval::RuntimeDecisionParams::decl(&cfg),
        crate::widgets::poster::PosterRequest::decl(&cfg),
        crate::widgets::poster::PosterRendered::decl(&cfg),
        crate::widgets::poster::PosterOutcome::decl(&cfg),
        ExportBundleParams::decl(&cfg),
        ExportCancelParams::decl(&cfg),
        crate::bundle::BundleWarningKind::decl(&cfg),
        crate::bundle::BundleWarning::decl(&cfg),
        BundleExported::decl(&cfg),
        crate::bundle::ArticleAnchor::decl(&cfg),
        crate::bundle::ArticleView::decl(&cfg),
        ForwardSyncParams::decl(&cfg),
        InverseSyncParams::decl(&cfg),
        StructureOutlineParams::decl(&cfg),
        StructureDiagnosticsParams::decl(&cfg),
        HistoryRecordParams::decl(&cfg),
        HistoryListParams::decl(&cfg),
        HistoryRetentionParams::decl(&cfg),
        HistoryBatchFilesParams::decl(&cfg),
        IndexOpenParams::decl(&cfg),
        IndexWatchedParams::decl(&cfg),
        IndexTouchParams::decl(&cfg),
        WatchStartParams::decl(&cfg),
        WatchPollParams::decl(&cfg),
        WatchStopParams::decl(&cfg),
        WatchEvent::decl(&cfg),
        IndexOverlayParams::decl(&cfg),
        IndexSearchParams::decl(&cfg),
        IndexFindFilesParams::decl(&cfg),
        IndexReplacePreviewParams::decl(&cfg),
        IndexReplaceApplyParams::decl(&cfg),
        FuzzyRankParams::decl(&cfg),
        IndexDefinitionAtParams::decl(&cfg),
        IndexMacrosParams::decl(&cfg),
        PresenceSetParams::decl(&cfg),
        PresencePendingParams::decl(&cfg),
        PresenceAnswerParams::decl(&cfg),
        OpenOutcome::decl(&cfg),
        OpenRequest::decl(&cfg),
        MainResolveParams::decl(&cfg),
        MainSetAssociationParams::decl(&cfg),
        FileReadParams::decl(&cfg),
        FileReadBytesParams::decl(&cfg),
        FileWriteParams::decl(&cfg),
        FileSaveParams::decl(&cfg),
        FileListParams::decl(&cfg),
        FileRenameParams::decl(&cfg),
        FileMkdirParams::decl(&cfg),
        FileRemoveParams::decl(&cfg),
        FileStatParams::decl(&cfg),
        FileTrashParams::decl(&cfg),
        FileUndoTrashParams::decl(&cfg),
        EventAppendParams::decl(&cfg),
        EventRotateParams::decl(&cfg),
        RotateReport::decl(&cfg),
        Request::decl(&cfg),
        Response::decl(&cfg),
    ];
    let mut out = String::from(
        "// Generated from src-tauri/core (maleficium-core). Do not edit:\n\
         // change the Rust types, then run\n\
         //   MALEFICIUM_WRITE_TS=1 cargo test --manifest-path src-tauri/Cargo.toml --workspace\n\n\
         import type { BatchFile, BundleProfile, BusEvent, OfflineReadiness, RecordOutcome, RetentionInfo, Revision, RuntimeDecision, WatchChange, WidgetApprovalCause } from './events';\n\
         import type { FileMatch, Lookup, ProjectMacro, Query, Ranked, ReplaceApplied, ReplacePreview, SearchResult } from './index';\n\
         import type { Diagnostic, Finding, Outline } from './structure';\n",
    );
    for d in decls {
        out.push_str("\nexport ");
        out.push_str(&d);
        out.push('\n');
    }
    out.push_str(
        "\n/** Params by operation name, derived from `Request`. */\n\
         export type OpParams = { [R in Request as R[\"op\"]]: R[\"params\"] };\n\n\
         /** Results by operation name, derived from `Response`. */\n\
         export type OpResult = { [R in Response as R[\"op\"]]: R[\"result\"] };\n",
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typescript_bindings_are_fresh() {
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../src/lib/generated/api.ts");
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
    fn dispatch_round_trips_through_json() {
        let cx = &Core::default();
        let req = Request::TemplatesList(TemplatesListParams {});
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["op"], "templatesList");
        let back: Request = serde_json::from_value(json).unwrap();
        let resp = dispatch(cx, back).unwrap();
        assert!(matches!(resp, Response::TemplatesList(_)));
        let v = serde_json::to_value(&resp).unwrap();
        assert_eq!(v["op"], "templatesList");
    }

    #[test]
    fn widgets_round_trips_and_rejects_unknown_roots() {
        let cx = &Core::default();
        let req = Request::Widgets(WidgetsParams {
            root_id: "nope".into(),
            main_rel: "main.tex".into(),
        });
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["op"], "widgets");
        assert_eq!(json["params"]["mainRel"], "main.tex");
        assert!(!dispatch(cx, req).unwrap_err().is_empty());
    }

    /// The shared contract (desktop, the future HTTP route, anything that
    /// dispatches requests) can read widget approvals but never write one:
    /// approve, revoke and auto-approval are desktop user actions only.
    #[test]
    fn no_operation_approves_revokes_or_sets_auto_approval() {
        use ts_rs::TS;
        let cfg = ts_rs::Config::new().with_large_int("number");
        let ops = Request::decl(&cfg).to_lowercase();
        assert!(ops.contains("widgetsstatus") && ops.contains("widgetreview"));
        for word in ["approve", "revoke", "auto", "grantwidget", "trust"] {
            assert!(!ops.contains(word), "an operation names `{word}`: {ops}");
        }
        let req = serde_json::json!({"op": "widgetApprove", "params": {"rootId": "r",
            "mainRel": "main.tex", "widget": "fig-demo", "digest": "0"}});
        assert!(serde_json::from_value::<Request>(req).is_err());
    }

    #[test]
    fn dispatch_rejects_unknown_roots_like_the_commands_did() {
        let cx = &Core::default();
        let err = dispatch(
            cx,
            Request::IndexOpen(IndexOpenParams {
                root_id: "nope".into(),
            }),
        )
        .unwrap_err();
        assert!(!err.is_empty());
    }
}
