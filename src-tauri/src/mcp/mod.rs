//! Automation API over stdio: every tool validates its session root and
//! rejects escapes before touching the fs.
//!
//! Failures come back as tool errors (`isError: true`, the reason as text);
//! successes carry the typed record as `structuredContent` (the model and
//! text-only hosts read the same JSON in `content`).

use rmcp::{
    handler::server::wrapper::{Json, Parameters},
    tool, tool_router,
    transport::stdio,
    ServiceExt,
};
use serde::{Deserialize, Serialize};

use crate::core;

#[derive(Debug, Clone, Copy, Default)]
struct Maleficium;

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

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct CompileRunOut {
    job_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct CompilePollParams {
    job_id: String,
    tail_lines: Option<usize>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct CompilePollOut {
    status: String,
    pdf_url: Option<String>,
    log: String,
    lines: Vec<String>,
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

fn path_string(p: std::path::PathBuf) -> String {
    p.to_string_lossy().to_string()
}

#[tool_router(server_handler)]
impl Maleficium {
    #[tool(description = "Grant a session project root (absolute directory, canonicalized)")]
    fn grant(&self, Parameters(p): Parameters<GrantParams>) -> Result<Json<PathOut>, String> {
        let path = path_string(core::grant_root(&p.root_id, &p.root)?);
        Ok(Json(PathOut { path }))
    }

    #[tool(description = "Show a granted session root")]
    fn info(&self, Parameters(p): Parameters<RootParams>) -> Result<Json<PathOut>, String> {
        let path = path_string(core::session_root(&p.root_id)?);
        Ok(Json(PathOut { path }))
    }

    #[tool(description = "List one directory level inside a session root")]
    fn list(&self, Parameters(p): Parameters<ListParams>) -> Result<Json<ListOut>, String> {
        let entries = core::list_dir(&p.root_id, p.rel.as_deref().unwrap_or("."))?
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
        let text = core::read_text(&p.root_id, &p.rel)?;
        Ok(Json(TextOut { text }))
    }

    #[tool(description = "Start a compile job; poll for the result")]
    fn compile_run(
        &self,
        Parameters(p): Parameters<FileParams>,
    ) -> Result<Json<CompileRunOut>, String> {
        let job_id = core::run_job(&p.root_id, &p.rel, 120)?;
        Ok(Json(CompileRunOut { job_id }))
    }

    #[tool(
        description = "Poll a compile job; running jobs report lines so far. pdf_url locates the output: treat it as opaque"
    )]
    fn compile_poll(
        &self,
        Parameters(p): Parameters<CompilePollParams>,
    ) -> Result<Json<CompilePollOut>, String> {
        let r = core::poll_job(&p.job_id, p.tail_lines.unwrap_or(50))?;
        Ok(Json(CompilePollOut {
            status: r.status.as_str().to_string(),
            pdf_url: r.pdf_url,
            log: r.log,
            lines: r.lines,
        }))
    }

    #[tool(description = "Cancel a running compile job")]
    fn compile_cancel(
        &self,
        Parameters(p): Parameters<CancelParams>,
    ) -> Result<Json<CancelOut>, String> {
        let status = core::cancel_job(&p.job_id)?;
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
            &p.root_id,
            &p.main_rel,
            p.page,
            p.x.unwrap_or(0.0),
            p.y.unwrap_or(0.0),
        )?))
    }

    #[tool(
        description = "Delete a project file to the app-local trash. Pass the file path back as confirm to act; without it returns the required token."
    )]
    fn delete(&self, Parameters(p): Parameters<DeleteParams>) -> Result<Json<DeleteOut>, String> {
        let confirm = p.confirm.as_deref().unwrap_or("");
        let trash_path = core::trash_file(&p.root_id, &p.rel, confirm)?;
        Ok(Json(DeleteOut { trash_path }))
    }

    #[tool(description = "Restore a trashed file to its original path")]
    fn undo(&self, Parameters(p): Parameters<UndoParams>) -> Result<Json<PathOut>, String> {
        let path = core::undo_trash(&p.root_id, &p.trash_path)?;
        Ok(Json(PathOut { path }))
    }

    #[tool(description = "Tail the engine log for one main-file dir shard")]
    fn log_tail(&self, Parameters(p): Parameters<LogTailParams>) -> Result<Json<TextOut>, String> {
        let text = core::log_tail(&p.root_id, &p.rel, p.max_lines.unwrap_or(50))?;
        Ok(Json(TextOut { text }))
    }

    #[tool(
        description = "Outline of one saved file: sections (level 0 chapter .. 4 paragraph) with labels, figures, tables, and input boundaries as marker rows, each with its 1-based line. Reads disk, not unsaved editor buffers; revision changes when the file does."
    )]
    fn outline(
        &self,
        Parameters(p): Parameters<FileParams>,
    ) -> Result<Json<core::structure::OutlineDoc>, String> {
        Ok(Json(core::structure::outline_of(&p.root_id, &p.rel)?))
    }

    #[tool(
        description = "File graph of a document: every file reachable from main_rel over \\input/\\include/\\subfile (resolved from the main file's directory, as the engine does), with missing files and edges that leave the project flagged. Paths are root-relative."
    )]
    fn file_graph(
        &self,
        Parameters(p): Parameters<MainParams>,
    ) -> Result<Json<core::structure::FileGraph>, String> {
        Ok(Json(core::structure::file_graph(&p.root_id, &p.main_rel)?))
    }

    #[tool(
        description = "Labels and references across the whole document from main_rel: each \\label with file and line (duplicate keys flagged), each \\ref-family use with whether its key is defined."
    )]
    fn labels_refs(
        &self,
        Parameters(p): Parameters<MainParams>,
    ) -> Result<Json<core::structure::LabelsRefs>, String> {
        Ok(Json(core::structure::labels_refs(&p.root_id, &p.main_rel)?))
    }

    #[tool(
        description = "Citations across the whole document from main_rel, checked against its \\bibliography/\\addbibresource files: each cite key with file, line, and whether a bib entry defines it."
    )]
    fn citations(
        &self,
        Parameters(p): Parameters<MainParams>,
    ) -> Result<Json<core::structure::Citations>, String> {
        Ok(Json(core::structure::citations(&p.root_id, &p.main_rel)?))
    }

    #[tool(
        description = "Structured diagnostics from main_rel's last compile log: root-relative path, line, message, severity. Entries outside the project are flagged external and carry no path. max caps rows (default 100)."
    )]
    fn diagnostics(
        &self,
        Parameters(p): Parameters<DiagnosticsParams>,
    ) -> Result<Json<core::structure::Diagnostics>, String> {
        Ok(Json(core::structure::diagnostics(
            &p.root_id,
            &p.main_rel,
            p.max.unwrap_or(100),
        )?))
    }
}

pub async fn run_stdio() -> anyhow::Result<()> {
    let service = Maleficium.serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::synctex::{forward_sync, inverse_sync};

    /// Both adapters sit on one implementation: every escape the desktop
    /// command refuses, the MCP tool refuses with the same error.
    #[test]
    fn synctex_adapters_reject_identically() {
        let dir = std::env::temp_dir().join(format!("maleficium-sym-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("main.tex"), "x").unwrap();
        std::os::unix::fs::symlink("/etc/hostname", dir.join("link.tex")).unwrap();
        let canon = dir.canonicalize().unwrap();
        core::grant_root("sym", &canon.to_string_lossy()).unwrap();
        // A compiled output, so tex_rel validation is reached.
        let out = core::main_outputs(&canon.join("main.tex")).unwrap();
        std::fs::create_dir_all(&out.outdir).unwrap();
        std::fs::write(out.outdir.join(&out.pdf_name), "%PDF").unwrap();
        let m = Maleficium;
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
            let desktop = forward_sync(
                root_id.to_string(),
                main_rel.clone(),
                tex_rel.to_string(),
                1,
            );
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

            let desktop = inverse_sync(root_id.to_string(), main_rel.clone(), 1, 0.0, 0.0);
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
