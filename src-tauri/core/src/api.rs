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

use maleficium_events::{BatchFile, OfflineReadiness, RecordOutcome, RetentionInfo, Revision};
use maleficium_index::definition::Lookup;
use maleficium_index::replace::{ReplaceApplied, ReplacePreview};
use maleficium_index::search::{FileMatch, Query, Ranked, SearchResult};
use maleficium_structure::{Diagnostic, Outline};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::export::Exported;
use crate::outputs::OutputStamp;
use crate::structure::Precheck;
use crate::synctex::{ForwardHit, InverseHit};
use crate::templates::{Created, TemplateInfo, TemplateList};

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
    IndexOverlayParams { root_id: String, rel: String, text: Option<String> },
    IndexSearchParams { root_id: String, query: Query, main_rel: Option<String>, max: Option<usize> },
    IndexFindFilesParams { root_id: String, query: String, max: Option<usize> },
    IndexReplacePreviewParams { root_id: String, query: Query, replacement: String, main_rel: Option<String> },
    IndexReplaceApplyParams { root_id: String, token: String, keep_open: Vec<String> },
    FuzzyRankParams { query: String, items: Vec<String>, max: Option<usize> },
    IndexDefinitionAtParams { root_id: String, line: String, col: u32, main_rel: Option<String> },
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
    IndexOverlay via index_overlay(IndexOverlayParams) -> (),
    IndexSearch via index_search(IndexSearchParams) -> SearchResult,
    IndexFindFiles via index_find_files(IndexFindFilesParams) -> Vec<FileMatch>,
    IndexReplacePreview via index_replace_preview(IndexReplacePreviewParams) -> ReplacePreview,
    IndexReplaceApply via index_replace_apply(IndexReplaceApplyParams) -> ReplaceApplied,
    FuzzyRank via fuzzy_rank(FuzzyRankParams) -> Vec<Ranked>,
    IndexDefinitionAt via index_definition_at(IndexDefinitionAtParams) -> Option<Lookup>,
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
        TemplateInfo::decl(&cfg),
        TemplateList::decl(&cfg),
        Created::decl(&cfg),
        ForwardHit::decl(&cfg),
        InverseHit::decl(&cfg),
        Precheck::decl(&cfg),
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
        IndexOverlayParams::decl(&cfg),
        IndexSearchParams::decl(&cfg),
        IndexFindFilesParams::decl(&cfg),
        IndexReplacePreviewParams::decl(&cfg),
        IndexReplaceApplyParams::decl(&cfg),
        FuzzyRankParams::decl(&cfg),
        IndexDefinitionAtParams::decl(&cfg),
        Request::decl(&cfg),
        Response::decl(&cfg),
    ];
    let mut out = String::from(
        "// Generated from src-tauri/core (maleficium-core). Do not edit:\n\
         // change the Rust types, then run\n\
         //   MALEFICIUM_WRITE_TS=1 cargo test --manifest-path src-tauri/Cargo.toml --workspace\n\n\
         import type { BatchFile, OfflineReadiness, RecordOutcome, RetentionInfo, Revision } from './events';\n\
         import type { FileMatch, Lookup, Query, Ranked, ReplaceApplied, ReplacePreview, SearchResult } from './index';\n\
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
