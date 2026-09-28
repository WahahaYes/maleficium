//! Automation API over stdio: every tool validates its session root and
//! rejects escapes before touching the fs.
//!
//! Failures come back as tool errors (`isError: true`, the reason as text);
//! successes carry the typed record as `structuredContent` (the model and
//! text-only hosts read the same JSON in `content`).

use rmcp::{
    handler::server::wrapper::{Json, Parameters},
    tool, tool_handler, tool_router,
    transport::stdio,
    ServiceExt,
};
use serde::{Deserialize, Serialize};

use maleficium_core as core;

/// The MCP adapter: one `Core` for the life of the stdio session.
#[derive(Clone, Default)]
struct Maleficium {
    cx: core::Core,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct GrantParams {
    root_id: String,
    root: String,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct PathOut {
    path: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct RootParams {
    root_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct ListParams {
    root_id: String,
    rel: Option<String>,
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

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct FileParams {
    root_id: String,
    rel: String,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct TextOut {
    text: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct CompileRunParams {
    root_id: String,
    rel: String,
    /// Fetch everything online first, then prove it compiles from the cache
    /// alone (the app's Make Available Offline).
    networked: Option<bool>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct CompileRunOut {
    job_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct CompilePollParams {
    job_id: String,
    tail_lines: Option<usize>,
    /// Wait up to this long (ms, at most 60000) for a running job to finish
    /// before answering.
    wait_ms: Option<u64>,
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
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct OutputStampOut {
    stamp: Option<core::OutputStamp>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct ExportPdfParams {
    root_id: String,
    main_rel: String,
    /// Absolute path outside the project.
    dest: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct ExportZipParams {
    root_id: String,
    /// Absolute path outside the project.
    dest: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct NewFromTemplateParams {
    /// A template id from `templates` (or "welcome").
    template: String,
    /// Absolute folder the new project goes in.
    parent_dir: String,
    /// The new project's folder name.
    name: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct CancelParams {
    job_id: String,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct CancelOut {
    status: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct ForwardParams {
    root_id: String,
    main_rel: String,
    tex_rel: String,
    line: u32,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct InverseParams {
    root_id: String,
    main_rel: String,
    page: u32,
    x: Option<f32>,
    y: Option<f32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct DeleteParams {
    root_id: String,
    rel: String,
    confirm: Option<String>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct DeleteOut {
    trash_path: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct UndoParams {
    root_id: String,
    trash_path: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct LogTailParams {
    root_id: String,
    rel: String,
    max_lines: Option<usize>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct MainParams {
    root_id: String,
    main_rel: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct DiagnosticsParams {
    root_id: String,
    main_rel: String,
    max: Option<usize>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct SearchParams {
    root_id: String,
    /// Literal text, or a regular expression when `regex` is true.
    pattern: String,
    regex: Option<bool>,
    case_sensitive: Option<bool>,
    whole_word: Option<bool>,
    /// Files reachable from this main file rank first.
    main_rel: Option<String>,
    /// Hits returned (default 1000).
    max: Option<usize>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct ReplacePreviewParams {
    root_id: String,
    /// Literal text, or a regular expression when `regex` is true.
    pattern: String,
    /// Inserted as written; with regex=true, `$1` / `${name}` expand groups.
    replacement: String,
    regex: Option<bool>,
    case_sensitive: Option<bool>,
    whole_word: Option<bool>,
    main_rel: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct ReplaceApplyParams {
    root_id: String,
    /// The token from replace_preview.
    token: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct ReplaceUndoParams {
    root_id: String,
    /// The batch from replace_apply.
    batch: String,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct ReplaceUndoOut {
    restored: Vec<maleficium_events::BatchFile>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct DefinitionParams {
    root_id: String,
    /// Look up a known reference: label, citation, macro (with its
    /// backslash) or input.
    kind: Option<maleficium_index::definition::RefKind>,
    key: Option<String>,
    /// Or look up whatever sits at a 1-based line and UTF-16 column of a file.
    rel: Option<String>,
    line: Option<u32>,
    col: Option<u32>,
    /// Inputs resolve from this main file's directory (default: the root).
    main_rel: Option<String>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct DefinitionOut {
    /// `None` when nothing referable sits at the position.
    lookup: Option<maleficium_index::definition::Lookup>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct FindFilesParams {
    root_id: String,
    query: String,
    max: Option<usize>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct FindFilesOut {
    files: Vec<maleficium_index::search::FileMatch>,
}

fn path_string(p: std::path::PathBuf) -> String {
    p.to_string_lossy().to_string()
}

#[tool_router]
impl Maleficium {
    #[tool(description = "Grant a session project root (absolute directory, canonicalized)")]
    fn grant(&self, Parameters(p): Parameters<GrantParams>) -> Result<Json<PathOut>, String> {
        let path = path_string(core::grant_root(&self.cx, &p.root_id, &p.root)?);
        Ok(Json(PathOut { path }))
    }

    #[tool(description = "Show a granted session root")]
    fn info(&self, Parameters(p): Parameters<RootParams>) -> Result<Json<PathOut>, String> {
        let path = path_string(core::session_root(&self.cx, &p.root_id)?);
        Ok(Json(PathOut { path }))
    }

    #[tool(description = "List one directory level inside a session root")]
    fn list(&self, Parameters(p): Parameters<ListParams>) -> Result<Json<ListOut>, String> {
        let entries = core::list_dir(&self.cx, &p.root_id, p.rel.as_deref().unwrap_or("."))?
            .into_iter()
            .map(|e| EntryOut {
                name: e.name,
                entry_type: e.entry_type,
            })
            .collect();
        Ok(Json(ListOut { entries }))
    }

    #[tool(description = "Read a project file as UTF-8 text")]
    fn read(&self, Parameters(p): Parameters<FileParams>) -> Result<Json<TextOut>, String> {
        let text = core::read_text(&self.cx, &p.root_id, &p.rel)?;
        Ok(Json(TextOut { text }))
    }

    #[tool(
        description = "Start a compile job; poll for the result. Offline-first: compiles from cached TeX files, fetching what the cache lacks only when the machine has network"
    )]
    fn compile_run(
        &self,
        Parameters(p): Parameters<CompileRunParams>,
    ) -> Result<Json<CompileRunOut>, String> {
        let job_id = core::run_job(
            &self.cx,
            &p.root_id,
            &p.rel,
            p.networked.unwrap_or(false),
            120,
        )?;
        Ok(Json(CompileRunOut { job_id }))
    }

    #[tool(
        description = "Poll a compile job; running jobs report lines so far. Pass wait_ms (up to 60000) to wait for the job to finish instead of polling in a loop. pdf_url locates the output: treat it as opaque. missing names the dependency a finished run lacked (a file, font, tool or package) and why: not-cached, fetch-failed, not-in-bundle, cache-empty, bundle-unreachable, bundle-invalid, bundle-changed, system-font, external-tool, shell-escape-required"
    )]
    async fn compile_poll(
        &self,
        Parameters(p): Parameters<CompilePollParams>,
    ) -> Result<Json<CompilePollOut>, String> {
        let tail = p.tail_lines.unwrap_or(50);
        let wait = std::time::Duration::from_millis(p.wait_ms.unwrap_or(0).min(MAX_POLL_WAIT_MS));
        let deadline = tokio::time::Instant::now() + wait;
        let mut r = core::poll_job(&self.cx, &p.job_id, tail)?;
        while r.status == core::JobStatus::Running && tokio::time::Instant::now() < deadline {
            tokio::time::sleep_until(deadline.min(tokio::time::Instant::now() + POLL_STEP)).await;
            r = core::poll_job(&self.cx, &p.job_id, tail)?;
        }
        Ok(Json(CompilePollOut {
            status: r.status.as_str().to_string(),
            pdf_url: r.pdf_url,
            log: r.log,
            lines: r.lines,
            missing: r.missing,
        }))
    }

    #[tool(
        description = "Offline readiness of a project: ready (its last compile succeeded from cached TeX files alone, and every tool and system font it used is present), needs-network, needs-tool, needs-font, blocked or unverified, with what it needs and the dependency its last compile lacked"
    )]
    fn offline_readiness(
        &self,
        Parameters(p): Parameters<RootParams>,
    ) -> Result<Json<maleficium_events::OfflineReadiness>, String> {
        Ok(Json(core::compile::offline_readiness(
            &self.cx, &p.root_id,
        )?))
    }

    #[tool(
        description = "Stamp of main_rel's compiled pdf (mtimeMs, bytes), or null before any compile: it changes whenever anyone recompiles, so a viewer polls it to refresh"
    )]
    fn output_stamp(
        &self,
        Parameters(p): Parameters<MainParams>,
    ) -> Result<Json<OutputStampOut>, String> {
        Ok(Json(OutputStampOut {
            stamp: core::output_stamp(&self.cx, &p.root_id, &p.main_rel)?,
        }))
    }

    #[tool(
        description = "Copy main_rel's compiled pdf to dest, an absolute path outside the project. Fails before any compile."
    )]
    fn export_pdf(
        &self,
        Parameters(p): Parameters<ExportPdfParams>,
    ) -> Result<Json<core::export::Exported>, String> {
        Ok(Json(core::export::export_pdf(
            &self.cx,
            &p.root_id,
            &p.main_rel,
            &p.dest,
        )?))
    }

    #[tool(
        description = "Zip the project's sources to dest, an absolute path outside the project: every file but build outputs, trash, dot files and symlinks, listed root-relative in files."
    )]
    fn export_zip(
        &self,
        Parameters(p): Parameters<ExportZipParams>,
    ) -> Result<Json<core::export::Exported>, String> {
        Ok(Json(core::export::export_zip(
            &self.cx, &p.root_id, &p.dest,
        )?))
    }

    #[tool(
        description = "Project templates: the bundled set (article, report, book, letter, beamer, assignment, cv, resume, journal) and the user's own, each with name, description, category and main file"
    )]
    fn templates(&self) -> Result<Json<core::templates::TemplateList>, String> {
        Ok(Json(core::templates::list()))
    }

    #[tool(
        description = "Create parent_dir/name from a template (refused when that folder exists and is not empty). Returns the new root and its main file; grant the root to work in it."
    )]
    fn new_from_template(
        &self,
        Parameters(p): Parameters<NewFromTemplateParams>,
    ) -> Result<Json<core::templates::Created>, String> {
        Ok(Json(core::templates::instantiate(
            &p.template,
            &p.parent_dir,
            &p.name,
        )?))
    }

    #[tool(description = "Cancel a running compile job")]
    fn compile_cancel(
        &self,
        Parameters(p): Parameters<CancelParams>,
    ) -> Result<Json<CancelOut>, String> {
        let status = core::cancel_job(&self.cx, &p.job_id)?;
        Ok(Json(CancelOut { status }))
    }

    #[tool(
        description = "Forward SyncTeX query: the PDF page showing a line of tex_rel, in the output of main_rel"
    )]
    fn synctex_forward(
        &self,
        Parameters(p): Parameters<ForwardParams>,
    ) -> Result<Json<core::ForwardHit>, String> {
        Ok(Json(core::forward(
            &self.cx,
            &p.root_id,
            &p.main_rel,
            &p.tex_rel,
            p.line,
        )?))
    }

    #[tool(
        description = "Inverse SyncTeX query: the root-relative source file and line at a position in the output of main_rel"
    )]
    fn synctex_inverse(
        &self,
        Parameters(p): Parameters<InverseParams>,
    ) -> Result<Json<core::InverseHit>, String> {
        Ok(Json(core::inverse(
            &self.cx,
            &p.root_id,
            &p.main_rel,
            p.page,
            p.x.unwrap_or(0.0),
            p.y.unwrap_or(0.0),
        )?))
    }

    #[tool(
        description = "Delete a project file to the app-local trash, in two calls: without confirm it is refused with the file's absolute path; call again with that absolute path as confirm to delete."
    )]
    fn delete(&self, Parameters(p): Parameters<DeleteParams>) -> Result<Json<DeleteOut>, String> {
        let confirm = p.confirm.as_deref().unwrap_or("");
        let trash_path = core::trash_file(&self.cx, &p.root_id, &p.rel, confirm)?;
        Ok(Json(DeleteOut { trash_path }))
    }

    #[tool(description = "Restore a trashed file to its original path")]
    fn undo(&self, Parameters(p): Parameters<UndoParams>) -> Result<Json<PathOut>, String> {
        let path = core::undo_trash(&self.cx, &p.root_id, &p.trash_path)?;
        Ok(Json(PathOut { path }))
    }

    #[tool(description = "Tail the engine log for one main-file dir shard")]
    fn log_tail(&self, Parameters(p): Parameters<LogTailParams>) -> Result<Json<TextOut>, String> {
        let text = core::log_tail(&self.cx, &p.root_id, &p.rel, p.max_lines.unwrap_or(50))?;
        Ok(Json(TextOut { text }))
    }

    #[tool(
        description = "Outline of one saved file: sections (level 0 chapter .. 4 paragraph) with labels, figures, tables, and input boundaries as marker rows, each with its 1-based line. Reads disk, not unsaved editor buffers; revision changes when the file does."
    )]
    fn outline(
        &self,
        Parameters(p): Parameters<FileParams>,
    ) -> Result<Json<core::structure::OutlineDoc>, String> {
        Ok(Json(core::structure::outline_of(
            &self.cx, &p.root_id, &p.rel,
        )?))
    }

    #[tool(
        description = "File graph of a document: every file reachable from main_rel over \\input/\\include/\\subfile (resolved from the main file's directory, as the engine does), with missing files and edges that leave the project flagged. Paths are root-relative."
    )]
    fn file_graph(
        &self,
        Parameters(p): Parameters<MainParams>,
    ) -> Result<Json<core::structure::FileGraph>, String> {
        Ok(Json(core::structure::file_graph(
            &self.cx,
            &p.root_id,
            &p.main_rel,
        )?))
    }

    #[tool(
        description = "Labels and references across the whole document from main_rel: each \\label with file and line (duplicate keys flagged), each \\ref-family use with whether its key is defined."
    )]
    fn labels_refs(
        &self,
        Parameters(p): Parameters<MainParams>,
    ) -> Result<Json<core::structure::LabelsRefs>, String> {
        Ok(Json(core::structure::labels_refs(
            &self.cx,
            &p.root_id,
            &p.main_rel,
        )?))
    }

    #[tool(
        description = "Citations across the whole document from main_rel, checked against its \\bibliography/\\addbibresource files: each cite key with file, line, and whether a bib entry defines it."
    )]
    fn citations(
        &self,
        Parameters(p): Parameters<MainParams>,
    ) -> Result<Json<core::structure::Citations>, String> {
        Ok(Json(core::structure::citations(
            &self.cx,
            &p.root_id,
            &p.main_rel,
        )?))
    }

    #[tool(
        description = "Dependency checks over the whole document from main_rel, before compiling: every package or class neither the project nor the TeX bundle provides (all at once), biblatex needing biber (suggests backend=bibtex), shell-escape packages and \\write18, and fontspec fonts not installed. bundleChecked is false until a first compile has cached the bundle index."
    )]
    fn precompile_checks(
        &self,
        Parameters(p): Parameters<MainParams>,
    ) -> Result<Json<core::structure::Precheck>, String> {
        Ok(Json(core::structure::precompile_checks(
            &self.cx,
            &p.root_id,
            &p.main_rel,
        )?))
    }

    #[tool(
        description = "Search every text file of the project for literal text (default, case-insensitive) or a regex (regex=true; may span lines). Hits carry root-relative path, 1-based line, UTF-16 column and length, and the line as preview; each file carries its revision. Files reachable from main_rel rank first. Reads saved files. truncated counts hits past max (default 1000); unsearched counts files with no text (binary, over 2 MB, not UTF-8)."
    )]
    fn search(
        &self,
        Parameters(p): Parameters<SearchParams>,
    ) -> Result<Json<maleficium_index::search::SearchResult>, String> {
        let q = maleficium_index::search::Query {
            pattern: p.pattern,
            regex: p.regex.unwrap_or(false),
            case_sensitive: p.case_sensitive.unwrap_or(false),
            whole_word: p.whole_word.unwrap_or(false),
        };
        Ok(Json(core::search::search(
            &self.cx,
            &p.root_id,
            &q,
            p.main_rel.as_deref(),
            p.max.unwrap_or(core::search::MAX_HITS),
        )?))
    }

    #[tool(
        description = "Preview replacing every match of a search (same pattern and flags as search; no cap) across the project's saved files. Writes nothing. Returns each file's revision, replacement count and before/after lines, plus a token for replace_apply. Regex replacements expand $1 / ${name}."
    )]
    fn replace_preview(
        &self,
        Parameters(p): Parameters<ReplacePreviewParams>,
    ) -> Result<Json<maleficium_index::replace::ReplacePreview>, String> {
        let q = maleficium_index::search::Query {
            pattern: p.pattern,
            regex: p.regex.unwrap_or(false),
            case_sensitive: p.case_sensitive.unwrap_or(false),
            whole_word: p.whole_word.unwrap_or(false),
        };
        Ok(Json(core::replace::preview(
            &self.cx,
            &p.root_id,
            &q,
            &p.replacement,
            p.main_rel.as_deref(),
        )?))
    }

    #[tool(
        description = "Apply a previewed replace by its token. Refused whole, with nothing written, if any file changed since the preview (preview again). Each file's prior content is kept as one history batch; returns the batch for replace_undo and the files written."
    )]
    fn replace_apply(
        &self,
        Parameters(p): Parameters<ReplaceApplyParams>,
    ) -> Result<Json<maleficium_index::replace::ReplaceApplied>, String> {
        Ok(Json(core::replace::apply(
            &self.cx,
            &p.root_id,
            &p.token,
            &[],
        )?))
    }

    #[tool(
        description = "Undo an applied replace: every file of the batch back as it was before (the replaced state is kept in history too). Returns the files restored."
    )]
    fn replace_undo(
        &self,
        Parameters(p): Parameters<ReplaceUndoParams>,
    ) -> Result<Json<ReplaceUndoOut>, String> {
        Ok(Json(ReplaceUndoOut {
            restored: core::replace::undo(&self.cx, &p.root_id, &p.batch)?,
        }))
    }

    #[tool(
        description = "Where a label, citation key, macro or input is defined, across the project's saved files: pass kind + key, or rel + line + col to look up what sits there. Returns every definition (a duplicate label lists each) with root-relative path, line, and a one-line summary (the defining line, a bib entry's author, title and year, the macro definition, or the input's first line)."
    )]
    fn definition(
        &self,
        Parameters(p): Parameters<DefinitionParams>,
    ) -> Result<Json<DefinitionOut>, String> {
        use core::search::DefinitionTarget;
        let target = match (p.kind, p.key, p.rel, p.line, p.col) {
            (Some(kind), Some(key), _, _, _) => {
                let command = (kind == maleficium_index::definition::RefKind::Input)
                    .then(|| "input".to_string());
                DefinitionTarget::Ref(maleficium_index::definition::RefAt { kind, key, command })
            }
            (_, _, Some(rel), Some(line), Some(col)) => DefinitionTarget::At { rel, line, col },
            _ => return Err("pass kind and key, or rel, line and col".to_string()),
        };
        Ok(Json(DefinitionOut {
            lookup: core::search::definition(&self.cx, &p.root_id, target, p.main_rel.as_deref())?,
        }))
    }

    #[tool(
        description = "Find project files by fuzzy name: every query character in order, case-insensitive; file-name and segment-start matches rank first. Returns root-relative paths, best first (default 50)."
    )]
    fn find_files(
        &self,
        Parameters(p): Parameters<FindFilesParams>,
    ) -> Result<Json<FindFilesOut>, String> {
        Ok(Json(FindFilesOut {
            files: core::search::find_files(
                &self.cx,
                &p.root_id,
                &p.query,
                p.max.unwrap_or(core::search::MAX_FILE_MATCHES),
            )?,
        }))
    }

    #[tool(
        description = "Structured diagnostics from main_rel's last compile: the engine's errors, and TeX's warnings for undefined references and citations and duplicate labels (placed on the line that uses or defines the key), each with root-relative path, line, message, severity. Entries outside the project are flagged external and carry no path. missing names the dependency that compile lacked and why. max caps rows (default 100)."
    )]
    fn diagnostics(
        &self,
        Parameters(p): Parameters<DiagnosticsParams>,
    ) -> Result<Json<core::structure::Diagnostics>, String> {
        Ok(Json(core::structure::diagnostics(
            &self.cx,
            &p.root_id,
            &p.main_rel,
            p.max.unwrap_or(100),
        )?))
    }
}

// Named explicitly: rmcp's default server info is its own crate name and
// version, since its env! expands inside rmcp.
#[tool_handler(name = "maleficium")]
impl rmcp::ServerHandler for Maleficium {}

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
            let mcp = m.synctex_forward(Parameters(ForwardParams {
                root_id: root_id.to_string(),
                main_rel: main_rel.clone(),
                tex_rel: tex_rel.to_string(),
                line: 1,
            }));
            let err = desktop
                .err()
                .unwrap_or_else(|| panic!("accepted: {:?}", (root_id, &main_rel)));
            assert_eq!(mcp.err(), Some(err));

            let desktop = core::inverse(cx, root_id, &main_rel, 1, 0.0, 0.0);
            let mcp = m.synctex_inverse(Parameters(InverseParams {
                root_id: root_id.to_string(),
                main_rel: main_rel.clone(),
                page: 1,
                x: None,
                y: None,
            }));
            assert_eq!(mcp.as_ref().err(), desktop.as_ref().err());
            assert_eq!(mcp.ok().map(|j| j.0), desktop.ok());
        }
        let _ = std::fs::remove_dir_all(&out.outdir);
    }
}
