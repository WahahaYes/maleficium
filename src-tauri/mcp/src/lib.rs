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

/// The snippet View's `ui://` resource. A contract change ships `/v2` and
/// deletes `/v1` in the same change.
const SNIPPET_VIEW_URI: &str = "ui://maleficium/snippet/v1";
const SNIPPET_VIEW_HTML: &str = include_str!("../views/snippet.html");
/// The compile dashboard View: a `compile_run` job's status, phase, log
/// tail, findings and a cancel button, polled through `compile_poll`.
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
    /// Main file rel; absent resolves the same main file the app would.
    rel: Option<String>,
    /// Fetch everything online first, then prove it compiles from the cache
    /// alone (the app's Make Available Offline).
    networked: Option<bool>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct CompileRunOut {
    job_id: String,
    /// The main file the job compiles (root-relative).
    main_rel: String,
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
    /// The phase the run is in (or last reached) and the files it downloaded.
    progress: core::Progress,
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

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct InteractiveInstallParams {
    root_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct WidgetsParams {
    root_id: String,
    main_rel: String,
}

/// Unknown fields are refused: the export has no approval argument for an
/// agent to pass, so an `approved_fetch` (or anything like it) is an error,
/// not something quietly ignored.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct ExportBundleParams {
    root_id: String,
    main_rel: String,
    /// Absolute path outside the project: a new or empty folder (folder,
    /// hosted), or the file to write (single-file).
    dest: String,
    profile: maleficium_events::BundleProfile,
    /// Single-file warning threshold in bytes; default 50 MiB.
    size_cap_bytes: Option<u64>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct PreviewBundleParams {
    root_id: String,
    main_rel: String,
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

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct SnippetParams {
    root_id: String,
    main_rel: String,
    /// Target a source line: tex_rel and line together.
    tex_rel: Option<String>,
    line: Option<u32>,
    /// Or target a label's definition.
    label: Option<String>,
    /// Or a whole page (1-based).
    page: Option<u32>,
    /// Also return the region as a PNG image (default false: images cost context).
    with_image: Option<bool>,
}

impl SnippetParams {
    fn target(&self) -> Result<core::snippet::Target, String> {
        use core::snippet::Target;
        match (&self.tex_rel, self.line, &self.label, self.page) {
            (Some(tex_rel), Some(line), None, None) => Ok(Target::Line {
                tex_rel: tex_rel.clone(),
                line,
            }),
            (None, None, Some(label), None) => Ok(Target::Label {
                label: label.clone(),
            }),
            (None, None, None, Some(page)) => Ok(Target::Page { page }),
            _ => Err("give exactly one target: tex_rel with line, or label, or page".to_string()),
        }
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct SnippetRenderParams {
    root_id: String,
    main_rel: String,
    /// The page to render (1-based).
    page: u32,
    /// Crop to a full-width band around this region (pdf points from the
    /// page's top left); omit for the whole page.
    region: Option<core::snippet::Region>,
    /// Pixels per point before the size caps (default 2, clamped to 0.5..3).
    scale: Option<f32>,
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
    #[tool(description = "Grant a session project root (absolute directory, canonicalized)")]
    fn grant(&self, Parameters(p): Parameters<GrantParams>) -> Result<Json<PathOut>, String> {
        self.tool("grant", || {
            let path = path_string(core::grant_root(&self.cx, &p.root_id, &p.root)?);
            Ok(Json(PathOut { path }))
        })
    }

    #[tool(description = "Show a granted session root")]
    fn info(&self, Parameters(p): Parameters<RootParams>) -> Result<Json<PathOut>, String> {
        self.tool("info", || {
            let path = path_string(core::session_root(&self.cx, &p.root_id)?);
            Ok(Json(PathOut { path }))
        })
    }

    #[tool(description = "List one directory level inside a session root")]
    fn list(&self, Parameters(p): Parameters<ListParams>) -> Result<Json<ListOut>, String> {
        self.tool("list", || {
            let entries = core::list_dir(&self.cx, &p.root_id, p.rel.as_deref().unwrap_or("."))?
                .into_iter()
                .map(|e| EntryOut {
                    name: e.name,
                    entry_type: e.entry_type,
                })
                .collect();
            Ok(Json(ListOut { entries }))
        })
    }

    #[tool(description = "Read a project file as UTF-8 text")]
    fn read(&self, Parameters(p): Parameters<FileParams>) -> Result<Json<TextOut>, String> {
        self.tool("read", || {
            let text = core::read_text(&self.cx, &p.root_id, &p.rel)?;
            Ok(Json(TextOut { text }))
        })
    }

    #[tool(
        description = "Start a compile job; poll for the result. Offline-first: compiles from cached TeX files, fetching what the cache lacks only when the machine has network. rel optional: without it the job compiles the same main file the app would",
        meta = ui_meta(Some(COMPILE_VIEW_URI), &["model", "app"])
    )]
    fn compile_run(
        &self,
        Parameters(p): Parameters<CompileRunParams>,
    ) -> Result<Json<CompileRunOut>, String> {
        self.tool("compile_run", || {
            let rel = match p.rel {
                Some(r) => r,
                None => {
                    let resolved = core::mainfile::resolve(&self.cx, &p.root_id, None)?;
                    let abs = resolved
                        .main
                        .ok_or_else(|| format!("no main file resolved for root {}", p.root_id))?;
                    let root = core::session_root(&self.cx, &p.root_id)?;
                    std::path::Path::new(&abs)
                        .strip_prefix(&root)
                        .map_err(|_| format!("resolved main file is outside the project: {abs}"))?
                        .to_string_lossy()
                        .to_string()
                }
            };
            let job_id = core::run_job(
                &self.cx,
                &p.root_id,
                &rel,
                p.networked.unwrap_or(false),
                core::compile::COMPILE_TIMEOUT_SECS,
            )?;
            Ok(Json(CompileRunOut {
                job_id,
                main_rel: rel,
            }))
        })
    }

    #[tool(
        description = "Poll a compile job; running jobs report lines so far. Pass wait_ms (up to 60000) to wait for the job to finish instead of polling in a loop. pdf_url locates the output: treat it as opaque. missing names the dependency a finished run lacked (a file, font, tool or package) and why: not-cached, fetch-failed, not-in-bundle, cache-empty, bundle-unreachable, bundle-invalid, bundle-changed, system-font, external-tool, shell-escape-required"
    )]
    async fn compile_poll(
        &self,
        Parameters(p): Parameters<CompilePollParams>,
    ) -> Result<Json<CompilePollOut>, String> {
        self.tool_async("compile_poll", async {
            let tail = p.tail_lines.unwrap_or(50);
            let wait =
                std::time::Duration::from_millis(p.wait_ms.unwrap_or(0).min(MAX_POLL_WAIT_MS));
            let deadline = tokio::time::Instant::now() + wait;
            let mut r = core::poll_job(&self.cx, &p.job_id, tail)?;
            while r.status == core::JobStatus::Running && tokio::time::Instant::now() < deadline {
                tokio::time::sleep_until(deadline.min(tokio::time::Instant::now() + POLL_STEP))
                    .await;
                r = core::poll_job(&self.cx, &p.job_id, tail)?;
            }
            Ok(Json(CompilePollOut {
                status: r.status.as_str().to_string(),
                pdf_url: r.pdf_url,
                log: r.log,
                lines: r.lines,
                missing: r.missing,
                progress: r.progress,
            }))
        })
        .await
    }

    #[tool(
        description = "Offline readiness of a project: ready (its last compile succeeded from cached TeX files alone, and every tool and system font it used is present), needs-network, needs-tool, needs-font, blocked or unverified, with what it needs and the dependency its last compile lacked"
    )]
    fn offline_readiness(
        &self,
        Parameters(p): Parameters<RootParams>,
    ) -> Result<Json<maleficium_events::OfflineReadiness>, String> {
        self.tool("offline_readiness", || {
            Ok(Json(core::compile::offline_readiness(
                &self.cx, &p.root_id,
            )?))
        })
    }

    #[tool(
        description = "Stamp of main_rel's compiled pdf (mtimeMs, bytes), or null before any compile: it changes whenever anyone recompiles, so a viewer polls it to refresh"
    )]
    fn output_stamp(
        &self,
        Parameters(p): Parameters<MainParams>,
    ) -> Result<Json<OutputStampOut>, String> {
        self.tool("output_stamp", || {
            Ok(Json(OutputStampOut {
                stamp: core::output_stamp(&self.cx, &p.root_id, &p.main_rel)?,
            }))
        })
    }

    #[tool(
        description = "Copy main_rel's compiled pdf to dest, an absolute path outside the project. Fails before any compile."
    )]
    fn export_pdf(
        &self,
        Parameters(p): Parameters<ExportPdfParams>,
    ) -> Result<Json<core::export::Exported>, String> {
        self.tool("export_pdf", || {
            Ok(Json(core::export::export_pdf(
                &self.cx,
                &p.root_id,
                &p.main_rel,
                &p.dest,
            )?))
        })
    }

    #[tool(
        description = "Zip the project's sources to dest, an absolute path outside the project: every file but build outputs, trash, dot files and symlinks, listed root-relative in files."
    )]
    fn export_zip(
        &self,
        Parameters(p): Parameters<ExportZipParams>,
    ) -> Result<Json<core::export::Exported>, String> {
        self.tool("export_zip", || {
            Ok(Json(core::export::export_zip(
                &self.cx, &p.root_id, &p.dest,
            )?))
        })
    }

    #[tool(
        description = "Project templates: the bundled set (article, report, book, letter, beamer, assignment, cv, resume, journal) and the user's own, each with name, description, category and main file"
    )]
    fn templates(&self) -> Result<Json<core::templates::TemplateList>, String> {
        self.tool("templates", || Ok(Json(core::templates::list())))
    }

    #[tool(
        description = "Create parent_dir/name from a template (refused when that folder exists and is not empty). Returns the new root and its main file; grant the root to work in it."
    )]
    fn new_from_template(
        &self,
        Parameters(p): Parameters<NewFromTemplateParams>,
    ) -> Result<Json<core::templates::Created>, String> {
        self.tool("new_from_template", || {
            Ok(Json(core::templates::instantiate(
                &p.template,
                &p.parent_dir,
                &p.name,
            )?))
        })
    }

    #[tool(
        description = "Install the embedded maleficium-interactive.sty into a granted project root so its documents can declare interactive widgets. Explicit user action: the only writer of project sources on this path."
    )]
    fn interactive_install(
        &self,
        Parameters(p): Parameters<InteractiveInstallParams>,
    ) -> Result<Json<core::interactive::Installed>, String> {
        self.tool("interactive_install", || {
            Ok(Json(core::interactive::install(&self.cx, &p.root_id)?))
        })
    }

    #[tool(
        description = "The interactive widgets of main_rel's last compile: for each, its page, rect (PDF units, origin bottom-left), type, runtime, sources, options, alt, float label and figure number, and declared csp origins, in document order. Empty when the document declares none. Fails (isError) when main_rel was never compiled, or when the widget sidecar and the pdf disagree (recompile), or when a widget bundle's widget.json is invalid.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    fn widgets(
        &self,
        Parameters(p): Parameters<WidgetsParams>,
    ) -> Result<Json<core::widgets::WidgetList>, String> {
        self.tool("widgets", || {
            let r = core::widgets::widgets(&self.cx, &p.root_id, &p.main_rel);
            let _ = core::eventlog::append(std::slice::from_ref(&core::widgets::event(
                &p.main_rel,
                &r,
                maleficium_events::Actor::Agent,
            )));
            Ok(Json(r?))
        })
    }

    #[tool(
        description = "Export main_rel's last compile as a paper bundle at dest, an absolute path outside the project: profile folder or hosted writes a folder (manifest.json, index.html, paper.pdf, theme/, widgets/<id>/index.html, content-addressed assets/), single-file writes one html file with everything inline. Every widget is one self-contained document with a strict CSP; assets are sha256-hashed from the project's files. Writes only to dest (an earlier bundle there is replaced; any other non-empty folder is refused) and never fetches: a remote asset with no local copy is refused, since hashing it would need a download that only the user can approve in the app. Returns the path, bytes, counts and warnings (size cap, runtimes missing from this build, files an author bundle could not inline). Fails before any compile, for a destination inside the project, and for an invalid manifest.",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    fn export_bundle(
        &self,
        Parameters(p): Parameters<ExportBundleParams>,
    ) -> Result<Json<core::bundle::BundleExported>, String> {
        self.tool("export_bundle", || {
            let r = core::bundle::export_bundle(
                &self.cx,
                &p.root_id,
                &p.main_rel,
                &p.dest,
                p.profile,
                p.size_cap_bytes,
            );
            let _ = core::eventlog::append(std::slice::from_ref(&core::bundle::event(
                &p.main_rel,
                p.profile,
                &r,
                maleficium_events::Actor::Agent,
            )));
            Ok(Json(r?))
        })
    }

    #[tool(
        description = "Preview main_rel's last compile as a paper bundle: exports the single-file profile (everything inline, opens from file://) to index.html in a scratch folder under the app data dir, never in the project, and returns its absolute path. It does not open anything: open the path in a browser yourself, or tell the user to (the app's File menu has Preview in Browser). One scratch folder per project; the next preview replaces it. Same rules as export_bundle: never fetches, no approvals, fails before any compile and for an invalid manifest. The browser sandbox is the only isolation: it is not egress-proof, so treat widgets as untrusted.",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    fn preview_bundle(
        &self,
        Parameters(p): Parameters<PreviewBundleParams>,
    ) -> Result<Json<PreviewBundleOut>, String> {
        self.tool("preview_bundle", || {
            let r = core::bundle::preview_bundle(&self.cx, &p.root_id, &p.main_rel);
            let _ = core::eventlog::append(std::slice::from_ref(&core::bundle::event(
                &p.main_rel,
                core::bundle::BundleProfile::SingleFile,
                &r,
                maleficium_events::Actor::Agent,
            )));
            let r = r?;
            Ok(Json(PreviewBundleOut {
                hint: format!("open {} in a browser to view it", r.path),
                path: r.path,
                bytes: r.bytes,
                widgets: r.widgets,
                warnings: r.warnings,
            }))
        })
    }

    #[tool(description = "Cancel a running compile job")]
    fn compile_cancel(
        &self,
        Parameters(p): Parameters<CancelParams>,
    ) -> Result<Json<CancelOut>, String> {
        self.tool("compile_cancel", || {
            let status = core::cancel_job(&self.cx, &p.job_id)?;
            Ok(Json(CancelOut { status }))
        })
    }

    #[tool(
        description = "Forward SyncTeX query: the PDF page showing a line of tex_rel, in the output of main_rel"
    )]
    fn synctex_forward(
        &self,
        Parameters(p): Parameters<ForwardParams>,
    ) -> Result<Json<core::ForwardHit>, String> {
        self.tool("synctex_forward", || {
            Ok(Json(core::forward(
                &self.cx,
                &p.root_id,
                &p.main_rel,
                &p.tex_rel,
                p.line,
            )?))
        })
    }

    #[tool(
        description = "See how part of main_rel's compiled PDF looks: the page and region where a source line (tex_rel + line), a label, or a page landed, with the source lines around it. with_image: true also returns that region rendered as a PNG; use it for layout questions (placement, width, overflow, how a figure or table looks), not to read text. stale is true when the source changed after the last compile.",
        output_schema = rmcp::handler::server::common::schema_for_output::<core::snippet::Snippet>(),
        meta = ui_meta(Some(SNIPPET_VIEW_URI), &["model", "app"])
    )]
    fn snippet(
        &self,
        Parameters(p): Parameters<SnippetParams>,
    ) -> Result<rmcp::model::CallToolResult, String> {
        self.tool("snippet", || {
            use base64::Engine as _;
            let target = p.target()?;
            let s = core::snippet::snippet(
                &self.cx,
                &p.root_id,
                &p.main_rel,
                &target,
                p.with_image.unwrap_or(false),
            )?;
            let value = serde_json::to_value(&s).map_err(|e| e.to_string())?;
            let mut result = rmcp::model::CallToolResult::structured(value);
            if let Some(png) = &s.png {
                let data = base64::engine::general_purpose::STANDARD.encode(png);
                result
                    .content
                    .insert(0, rmcp::model::ContentBlock::image(data, "image/png"));
            }
            Ok(result)
        })
    }

    #[tool(
        description = "For the snippet View only: re-render a page, or a band of it, as a PNG at another scale",
        output_schema = rmcp::handler::server::common::schema_for_output::<core::snippet::Rendered>(),
        meta = ui_meta(Some(SNIPPET_VIEW_URI), &["app"])
    )]
    fn snippet_render(
        &self,
        Parameters(p): Parameters<SnippetRenderParams>,
    ) -> Result<rmcp::model::CallToolResult, String> {
        self.tool("snippet_render", || {
            use base64::Engine as _;
            let r = core::snippet::render_page(
                &self.cx,
                &p.root_id,
                &p.main_rel,
                p.page,
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
        })
    }

    #[tool(
        description = "Inverse SyncTeX query: the root-relative source file and line at a position in the output of main_rel"
    )]
    fn synctex_inverse(
        &self,
        Parameters(p): Parameters<InverseParams>,
    ) -> Result<Json<core::InverseHit>, String> {
        self.tool("synctex_inverse", || {
            Ok(Json(core::inverse(
                &self.cx,
                &p.root_id,
                &p.main_rel,
                p.page,
                p.x.unwrap_or(0.0),
                p.y.unwrap_or(0.0),
            )?))
        })
    }

    #[tool(
        description = "Delete a project file to the app-local trash, in two calls: without confirm it is refused with the file's absolute path; call again with that absolute path as confirm to delete."
    )]
    fn delete(&self, Parameters(p): Parameters<DeleteParams>) -> Result<Json<DeleteOut>, String> {
        self.tool("delete", || {
            let confirm = p.confirm.as_deref().unwrap_or("");
            let trash_path = core::trash_file(&self.cx, &p.root_id, &p.rel, confirm)?;
            Ok(Json(DeleteOut { trash_path }))
        })
    }

    #[tool(description = "Restore a trashed file to its original path")]
    fn undo(&self, Parameters(p): Parameters<UndoParams>) -> Result<Json<PathOut>, String> {
        self.tool("undo", || {
            let path = core::undo_trash(&self.cx, &p.root_id, &p.trash_path)?;
            Ok(Json(PathOut { path }))
        })
    }

    #[tool(description = "Tail the engine log for one main-file dir shard")]
    fn log_tail(&self, Parameters(p): Parameters<LogTailParams>) -> Result<Json<TextOut>, String> {
        self.tool("log_tail", || {
            let text = core::log_tail(&self.cx, &p.root_id, &p.rel, p.max_lines.unwrap_or(50))?;
            Ok(Json(TextOut { text }))
        })
    }

    #[tool(
        description = "Outline of one saved file: sections (level 0 chapter .. 4 paragraph) with labels, figures, tables, and input boundaries as marker rows, each with its 1-based line. Reads disk, not unsaved editor buffers; revision changes when the file does."
    )]
    fn outline(
        &self,
        Parameters(p): Parameters<FileParams>,
    ) -> Result<Json<core::structure::OutlineDoc>, String> {
        self.tool("outline", || {
            Ok(Json(core::structure::outline_of(
                &self.cx, &p.root_id, &p.rel,
            )?))
        })
    }

    #[tool(
        description = "File graph of a document: every file reachable from main_rel over \\input/\\include/\\subfile (resolved from the main file's directory, as the engine does), with missing files and edges that leave the project flagged. Paths are root-relative."
    )]
    fn file_graph(
        &self,
        Parameters(p): Parameters<MainParams>,
    ) -> Result<Json<core::structure::FileGraph>, String> {
        self.tool("file_graph", || {
            Ok(Json(core::structure::file_graph(
                &self.cx,
                &p.root_id,
                &p.main_rel,
            )?))
        })
    }

    #[tool(
        description = "Labels and references across the whole document from main_rel: each \\label with file and line (duplicate keys flagged), each \\ref-family use with whether its key is defined."
    )]
    fn labels_refs(
        &self,
        Parameters(p): Parameters<MainParams>,
    ) -> Result<Json<core::structure::LabelsRefs>, String> {
        self.tool("labels_refs", || {
            Ok(Json(core::structure::labels_refs(
                &self.cx,
                &p.root_id,
                &p.main_rel,
            )?))
        })
    }

    #[tool(
        description = "Citations across the whole document from main_rel, checked against its \\bibliography/\\addbibresource files: each cite key with file, line, and whether a bib entry defines it."
    )]
    fn citations(
        &self,
        Parameters(p): Parameters<MainParams>,
    ) -> Result<Json<core::structure::Citations>, String> {
        self.tool("citations", || {
            Ok(Json(core::structure::citations(
                &self.cx,
                &p.root_id,
                &p.main_rel,
            )?))
        })
    }

    #[tool(
        description = "Dependency checks over the whole document from main_rel, before compiling: every package or class neither the project nor the TeX bundle provides (all at once), biblatex needing biber (suggests backend=bibtex), shell-escape packages and \\write18, and fontspec fonts not installed. bundleChecked is false until a first compile has cached the bundle index."
    )]
    fn precompile_checks(
        &self,
        Parameters(p): Parameters<MainParams>,
    ) -> Result<Json<core::structure::Precheck>, String> {
        self.tool("precompile_checks", || {
            Ok(Json(core::structure::precompile_checks(
                &self.cx,
                &p.root_id,
                &p.main_rel,
            )?))
        })
    }

    #[tool(
        description = "Search every text file of the project for literal text (default, case-insensitive) or a regex (regex=true; may span lines). Hits carry root-relative path, 1-based line, UTF-16 column and length, and the line as preview; each file carries its revision. Files reachable from main_rel rank first. Reads saved files. truncated counts hits past max (default 1000); unsearched counts files with no text (binary, over 2 MB, not UTF-8)."
    )]
    fn search(
        &self,
        Parameters(p): Parameters<SearchParams>,
    ) -> Result<Json<maleficium_index::search::SearchResult>, String> {
        self.tool("search", || {
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
        })
    }

    #[tool(
        description = "Preview replacing every match of a search (same pattern and flags as search; no cap) across the project's saved files. Writes nothing. Returns each file's revision, replacement count and before/after lines, plus a token for replace_apply. Regex replacements expand $1 / ${name}."
    )]
    fn replace_preview(
        &self,
        Parameters(p): Parameters<ReplacePreviewParams>,
    ) -> Result<Json<maleficium_index::replace::ReplacePreview>, String> {
        self.tool("replace_preview", || {
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
        })
    }

    #[tool(
        description = "Apply a previewed replace by its token. Refused whole, with nothing written, if any file changed since the preview (preview again). Each file's prior content is kept as one history batch; returns the batch for replace_undo and the files written."
    )]
    fn replace_apply(
        &self,
        Parameters(p): Parameters<ReplaceApplyParams>,
    ) -> Result<Json<maleficium_index::replace::ReplaceApplied>, String> {
        self.tool("replace_apply", || {
            Ok(Json(core::replace::apply(
                &self.cx,
                &p.root_id,
                &p.token,
                &[],
            )?))
        })
    }

    #[tool(
        description = "Undo an applied replace: every file of the batch back as it was before (the replaced state is kept in history too). Returns the files restored."
    )]
    fn replace_undo(
        &self,
        Parameters(p): Parameters<ReplaceUndoParams>,
    ) -> Result<Json<ReplaceUndoOut>, String> {
        self.tool("replace_undo", || {
            Ok(Json(ReplaceUndoOut {
                restored: core::replace::undo(&self.cx, &p.root_id, &p.batch)?,
            }))
        })
    }

    #[tool(
        description = "Where a label, citation key, macro or input is defined, across the project's saved files: pass kind + key, or rel + line + col to look up what sits there. Returns every definition (a duplicate label lists each) with root-relative path, line, and a one-line summary (the defining line, a bib entry's author, title and year, the macro definition, or the input's first line)."
    )]
    fn definition(
        &self,
        Parameters(p): Parameters<DefinitionParams>,
    ) -> Result<Json<DefinitionOut>, String> {
        self.tool("definition", || {
            use core::search::DefinitionTarget;
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
                (_, _, Some(rel), Some(line), Some(col)) => DefinitionTarget::At { rel, line, col },
                _ => return Err("pass kind and key, or rel, line and col".to_string()),
            };
            Ok(Json(DefinitionOut {
                lookup: core::search::definition(
                    &self.cx,
                    &p.root_id,
                    target,
                    p.main_rel.as_deref(),
                )?,
            }))
        })
    }

    #[tool(
        description = "Find project files by fuzzy name: every query character in order, case-insensitive; file-name and segment-start matches rank first. Returns root-relative paths, best first (default 50)."
    )]
    fn find_files(
        &self,
        Parameters(p): Parameters<FindFilesParams>,
    ) -> Result<Json<FindFilesOut>, String> {
        self.tool("find_files", || {
            Ok(Json(FindFilesOut {
                files: core::search::find_files(
                    &self.cx,
                    &p.root_id,
                    &p.query,
                    p.max.unwrap_or(core::search::MAX_FILE_MATCHES),
                )?,
            }))
        })
    }

    #[tool(
        description = "Structured diagnostics from main_rel's last compile: the engine's errors, and TeX's warnings for undefined references and citations and duplicate labels (placed on the line that uses or defines the key), each with root-relative path, line, message, severity. Entries outside the project are flagged external and carry no path. missing names the dependency that compile lacked and why. max caps rows (default 100)."
    )]
    fn diagnostics(
        &self,
        Parameters(p): Parameters<DiagnosticsParams>,
    ) -> Result<Json<core::structure::Diagnostics>, String> {
        self.tool("diagnostics", || {
            Ok(Json(core::structure::diagnostics(
                &self.cx,
                &p.root_id,
                &p.main_rel,
                p.max.unwrap_or(100),
            )?))
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

    /// Only the snippet tools and compile_run name a View; the re-render
    /// tool is hidden from the model, and the dashboard's own calls
    /// (compile_poll, compile_cancel, diagnostics, offline_readiness) stay
    /// plain tools so text-only hosts see them as before.
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
        assert_eq!(with, ["compile_run", "snippet", "snippet_render"]);
        let get = |n: &str| ui(tools.iter().find(|t| t.name == n).unwrap()).unwrap();
        assert!(get("snippet").contains(SNIPPET_VIEW_URI) && get("snippet").contains("model"));
        assert_eq!(
            get("snippet_render"),
            format!(r#"{{"resourceUri":"{SNIPPET_VIEW_URI}","visibility":["app"]}}"#)
        );
        assert_eq!(
            get("compile_run"),
            format!(r#"{{"resourceUri":"{COMPILE_VIEW_URI}","visibility":["model","app"]}}"#)
        );
    }

    /// The widget list only reads: hosts may call it without confirmation.
    /// Its failures reach the client as tool errors, never as an empty list.
    #[test]
    fn widgets_is_a_read_only_tool_and_failures_are_errors() {
        let tools = Maleficium::tool_router().list_all();
        let t = tools.iter().find(|t| t.name == "widgets").expect("widgets");
        let a = t.annotations.as_ref().expect("annotations");
        assert_eq!(a.read_only_hint, Some(true));
        assert_eq!(a.destructive_hint, Some(false));
        assert_eq!(a.idempotent_hint, Some(true));
        assert_eq!(a.open_world_hint, Some(false));
        assert!(t.output_schema.is_some());

        let m = Maleficium::default();
        let err = m
            .widgets(Parameters(WidgetsParams {
                root_id: "nope".into(),
                main_rel: "main.tex".into(),
            }))
            .err()
            .expect("an ungranted root fails");
        assert!(!err.is_empty());
    }

    /// The export writes a new place and replaces only an earlier bundle, so
    /// it is not read-only; it never fetches, so it is not open-world. An
    /// agent has no way to approve a download: the parameters refuse any
    /// field the schema does not list, and the tool has no approval argument.
    /// The preview tool takes no destination (the core picks the scratch
    /// folder) and no approval, and it only returns a path: opening it is the
    /// app menu's job.
    #[test]
    fn preview_bundle_takes_a_project_and_nothing_else() {
        let tools = Maleficium::tool_router().list_all();
        let t = tools
            .iter()
            .find(|t| t.name == "preview_bundle")
            .expect("preview_bundle");
        let a = t.annotations.as_ref().expect("annotations");
        assert_eq!(a.read_only_hint, Some(false));
        assert_eq!(a.open_world_hint, Some(false));
        assert_eq!(a.idempotent_hint, Some(true));
        let schema = serde_json::to_value(&*t.input_schema).unwrap();
        let props: Vec<&str> = schema["properties"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(props, ["main_rel", "root_id"]);
        assert!(serde_json::from_value::<PreviewBundleParams>(
            serde_json::json!({"root_id": "r", "main_rel": "m.tex", "dest": "/tmp/x"})
        )
        .is_err());
    }

    #[test]
    fn export_bundle_cannot_be_given_an_approval_and_fails_closed() {
        let tools = Maleficium::tool_router().list_all();
        let t = tools
            .iter()
            .find(|t| t.name == "export_bundle")
            .expect("export_bundle");
        let a = t.annotations.as_ref().expect("annotations");
        assert_eq!(a.read_only_hint, Some(false));
        assert_eq!(a.destructive_hint, Some(true));
        assert_eq!(a.idempotent_hint, Some(true));
        assert_eq!(a.open_world_hint, Some(false));
        assert!(t.output_schema.is_some());

        let schema = serde_json::to_value(&*t.input_schema).unwrap();
        let props: Vec<&str> = schema["properties"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(
            props,
            ["dest", "main_rel", "profile", "root_id", "size_cap_bytes"]
        );
        assert_eq!(schema["additionalProperties"], false);
        let base = serde_json::json!({
            "root_id": "r", "main_rel": "main.tex", "dest": "/tmp/x", "profile": "folder"
        });
        assert!(serde_json::from_value::<ExportBundleParams>(base.clone()).is_ok());
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
                serde_json::from_value::<ExportBundleParams>(v).is_err(),
                "{extra} must not be accepted"
            );
        }
        let m = Maleficium::default();
        let err = m
            .export_bundle(Parameters(
                serde_json::from_value::<ExportBundleParams>(base).unwrap(),
            ))
            .err()
            .expect("an ungranted root fails");
        assert!(!err.is_empty());
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
        use maleficium_structure::{line_signal, CompilePhase};
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
    fn snippet_takes_exactly_one_target() {
        let p = |tex_rel: Option<&str>, line, label: Option<&str>, page| SnippetParams {
            root_id: "r".into(),
            main_rel: "main.tex".into(),
            tex_rel: tex_rel.map(Into::into),
            line,
            label: label.map(Into::into),
            page,
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
