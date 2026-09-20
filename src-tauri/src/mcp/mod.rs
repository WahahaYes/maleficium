//! Automation API over stdio: every tool validates its session root and
//! rejects escapes before touching the fs.

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
struct GrantOut {
    ok: bool,
    path: Option<String>,
    error: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct RootParams {
    root_id: String,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct InfoOut {
    ok: bool,
    path: Option<String>,
    error: Option<String>,
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
    ok: bool,
    entries: Vec<EntryOut>,
    error: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct ReadParams {
    root_id: String,
    rel: String,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct ReadOut {
    ok: bool,
    text: Option<String>,
    error: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct CompileRunParams {
    root_id: String,
    rel: String,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct CompileRunOut {
    ok: bool,
    job_id: Option<String>,
    error: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct CompilePollParams {
    job_id: String,
    tail_lines: Option<usize>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct CompilePollOut {
    ok: bool,
    status: String,
    pdf_path: Option<String>,
    log: String,
    lines: Vec<String>,
    error: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct CancelParams {
    job_id: String,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct CancelOut {
    ok: bool,
    error: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct ForwardParams {
    root_id: String,
    pdf_rel: String,
    tex_rel: String,
    line: u32,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct ForwardOut {
    ok: bool,
    text: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct InverseParams {
    root_id: String,
    dir_rel: String,
    pdf_name: String,
    page: u32,
    x: Option<f32>,
    y: Option<f32>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct InverseOut {
    ok: bool,
    text: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct DeleteParams {
    root_id: String,
    rel: String,
    confirm: Option<String>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct DeleteOut {
    ok: bool,
    trash_path: Option<String>,
    error: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct UndoParams {
    root_id: String,
    trash_path: String,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct UndoOut {
    ok: bool,
    path: Option<String>,
    error: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct LogTailParams {
    root_id: String,
    rel: String,
    max_lines: Option<usize>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct LogTailOut {
    ok: bool,
    text: Option<String>,
    error: Option<String>,
}

#[tool_router(server_handler)]
impl Maleficium {
    #[tool(description = "Grant a session project root (absolute directory, canonicalized)")]
    fn grant(&self, Parameters(p): Parameters<GrantParams>) -> Json<GrantOut> {
        match core::grant_root(&p.root_id, &p.root) {
            Ok(path) => Json(GrantOut {
                ok: true,
                path: Some(path.to_string_lossy().to_string()),
                error: None,
            }),
            Err(e) => Json(GrantOut {
                ok: false,
                path: None,
                error: Some(e),
            }),
        }
    }

    #[tool(description = "Show a granted session root")]
    fn info(&self, Parameters(p): Parameters<RootParams>) -> Json<InfoOut> {
        match core::session_root(&p.root_id) {
            Ok(path) => Json(InfoOut {
                ok: true,
                path: Some(path.to_string_lossy().to_string()),
                error: None,
            }),
            Err(e) => Json(InfoOut {
                ok: false,
                path: None,
                error: Some(e),
            }),
        }
    }

    #[tool(description = "List one directory level inside a session root")]
    fn list(&self, Parameters(p): Parameters<ListParams>) -> Json<ListOut> {
        match core::list_dir(&p.root_id, p.rel.as_deref().unwrap_or(".")) {
            Ok(entries) => Json(ListOut {
                ok: true,
                entries: entries
                    .into_iter()
                    .map(|e| EntryOut {
                        name: e.name,
                        entry_type: e.entry_type,
                    })
                    .collect(),
                error: None,
            }),
            Err(e) => Json(ListOut {
                ok: false,
                entries: Vec::new(),
                error: Some(e),
            }),
        }
    }

    #[tool(description = "Read a project file as UTF-8 text")]
    fn read(&self, Parameters(p): Parameters<ReadParams>) -> Json<ReadOut> {
        match core::read_text(&p.root_id, &p.rel) {
            Ok(text) => Json(ReadOut {
                ok: true,
                text: Some(text),
                error: None,
            }),
            Err(e) => Json(ReadOut {
                ok: false,
                text: None,
                error: Some(e),
            }),
        }
    }

    #[tool(description = "Start a compile job; poll for the result")]
    fn compile_run(&self, Parameters(p): Parameters<CompileRunParams>) -> Json<CompileRunOut> {
        match core::run_job(&p.root_id, &p.rel, 120) {
            Ok(job_id) => Json(CompileRunOut {
                ok: true,
                job_id: Some(job_id),
                error: None,
            }),
            Err(e) => Json(CompileRunOut {
                ok: false,
                job_id: None,
                error: Some(e),
            }),
        }
    }

    #[tool(description = "Poll a compile job; running jobs report lines so far")]
    fn compile_poll(&self, Parameters(p): Parameters<CompilePollParams>) -> Json<CompilePollOut> {
        match core::poll_job(&p.job_id, p.tail_lines.unwrap_or(50)) {
            Ok(r) => Json(CompilePollOut {
                ok: true,
                status: r.status.as_str().to_string(),
                pdf_path: r.pdf_path,
                log: r.log,
                lines: r.lines,
                error: None,
            }),
            Err(e) => Json(CompilePollOut {
                ok: false,
                status: "error".to_string(),
                pdf_path: None,
                log: String::new(),
                lines: Vec::new(),
                error: Some(e),
            }),
        }
    }

    #[tool(description = "Cancel a running compile job")]
    fn compile_cancel(&self, Parameters(p): Parameters<CancelParams>) -> Json<CancelOut> {
        match core::cancel_job(&p.job_id) {
            Ok(_) => Json(CancelOut {
                ok: true,
                error: None,
            }),
            Err(e) => Json(CancelOut {
                ok: false,
                error: Some(e),
            }),
        }
    }

    #[tool(description = "Forward SyncTeX query (editor line to PDF page)")]
    fn synctex_forward(&self, Parameters(p): Parameters<ForwardParams>) -> Json<ForwardOut> {
        match core::forward_query(&p.root_id, &p.pdf_rel, &p.tex_rel, p.line) {
            Ok(text) => Json(ForwardOut { ok: true, text }),
            Err(e) => Json(ForwardOut { ok: false, text: e }),
        }
    }

    #[tool(description = "Inverse SyncTeX query (PDF position to editor line)")]
    fn synctex_inverse(&self, Parameters(p): Parameters<InverseParams>) -> Json<InverseOut> {
        match core::inverse_query(
            &p.root_id,
            &p.dir_rel,
            &p.pdf_name,
            p.page,
            p.x.unwrap_or(0.0),
            p.y.unwrap_or(0.0),
        ) {
            Ok(text) => Json(InverseOut { ok: true, text }),
            Err(e) => Json(InverseOut { ok: false, text: e }),
        }
    }

    #[tool(
        description = "Delete a project file to the app-local trash. Pass the file path back as confirm to act; without it returns the required token."
    )]
    fn delete(&self, Parameters(p): Parameters<DeleteParams>) -> Json<DeleteOut> {
        let confirm = p.confirm.as_deref().unwrap_or("");
        match core::trash_file(&p.root_id, &p.rel, confirm) {
            Ok(trash_path) => Json(DeleteOut {
                ok: true,
                trash_path: Some(trash_path),
                error: None,
            }),
            Err(e) => Json(DeleteOut {
                ok: false,
                trash_path: None,
                error: Some(e),
            }),
        }
    }

    #[tool(description = "Restore a trashed file to its original path")]
    fn undo(&self, Parameters(p): Parameters<UndoParams>) -> Json<UndoOut> {
        match core::undo_trash(&p.root_id, &p.trash_path) {
            Ok(path) => Json(UndoOut {
                ok: true,
                path: Some(path),
                error: None,
            }),
            Err(e) => Json(UndoOut {
                ok: false,
                path: None,
                error: Some(e),
            }),
        }
    }

    #[tool(description = "Tail the engine log for one main-file dir shard")]
    fn log_tail(&self, Parameters(p): Parameters<LogTailParams>) -> Json<LogTailOut> {
        match core::log_tail(&p.root_id, &p.rel, p.max_lines.unwrap_or(50)) {
            Ok(text) => Json(LogTailOut {
                ok: true,
                text: Some(text),
                error: None,
            }),
            Err(e) => Json(LogTailOut {
                ok: false,
                text: None,
                error: Some(e),
            }),
        }
    }
}

pub async fn run_stdio() -> anyhow::Result<()> {
    let service = Maleficium.serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
