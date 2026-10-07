//! Auto-posters: everything about rendering one widget to a still image
//! except the webview. The app's one-shot renderer asks [`run`] to render a
//! request: [`prepare`] builds the job (the runtime document under its
//! widget policy, the bridge `init` body, the source bytes, the frame size
//! and the time limit), the renderer runs it in a hidden window that is
//! destroyed afterwards, and [`finish`] checks the snapshot and writes the
//! PNG to the caller's path.
//!
//! First-party runtimes render without approval: `model@1` and `chart@1`
//! (a table's poster is its typeset rows, and video posters are a separate
//! question). An `html` widget is author code: it renders only when
//! [`crate::widget_approval`] approves its folder, and the check is made
//! here, at execution time, on one snapshot of the folder whose bytes are
//! folded into the job's document; nothing reads the folder again. Not
//! approved, the request comes back as `approval_required` and nothing
//! runs. An approved html widget runs under the strict widget policy: its
//! declared origins are not reachable from a poster render.
//!
//! A `custom` widget may propose its poster: when its runtime is approved
//! (allowed at this digest, or an auto-covered change; never when missing,
//! invalid, unapproved, denied or licence-changed) the judged snapshot's
//! files are folded like an export and the runtime renders them twice,
//! once per colour mode. Both snapshots must be sane, non-blank PNGs that
//! differ (the runtime answers the theme); the light one is the poster.
//! Failure writes nothing.
//!
//! Poster precedence for a widget is [`poster_source`]: an explicit
//! `poster=` always wins, then a cached auto-poster, then a placeholder.

use super::{params, read, Widget, WidgetSource, WidgetType};
use crate::bundle::{fold, role_keys, runtime_host, runtime_options, sha_of};
use crate::theme::{Mode, Theme};
use crate::widget_approval::{self, ApprovalRequired, WidgetApprovalStatus, WidgetTarget};
use crate::Core;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;
use ts_rs::TS;

/// The time limit when a request gives none, and the bounds of one it gives.
const DEFAULT_TIMEOUT_MS: u64 = 20_000;
const MIN_TIMEOUT_MS: u64 = 500;
const MAX_TIMEOUT_MS: u64 = 120_000;
/// CSS pixels per PDF point.
const CSS_PER_PT: f64 = 96.0 / 72.0;
/// Default poster pixels per CSS pixel (192 dpi against the page).
const DEFAULT_DENSITY: u32 = 2;
/// The largest snapshot the bridge carries (a data URL under 8 MiB).
const MAX_SNAPSHOT: usize = 8 * 1024 * 1024;
/// A proposal's sides must fit in these pixels.
const MAX_PROPOSAL_SIDE: u32 = 4096;

/// A compile's time limit for one custom widget's proposal: author code
/// that never answers the snapshot request costs each compile this long,
/// per colour mode.
pub(crate) const CUSTOM_TIMEOUT_MS: u64 = 10_000;

/// A debug-only runtime that announces itself, then spins forever on
/// `init`: the renderer's hard time limit is proven against it. Release
/// builds do not know the name.
#[cfg(debug_assertions)]
pub const TEST_HANG_RUNTIME: &str = "hang@test";
#[cfg(debug_assertions)]
const TEST_HANG_HOST: &str = "<!doctype html><html><head><meta charset=\"utf-8\"></head><body><script>\
addEventListener('message',function(e){if(e.source===parent&&e.data&&e.data.type==='init'){for(;;){}}});\
parent.postMessage({mfw:1,type:'ready'},'*');</script></body></html>";

/// Render one widget's poster to `out_path`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PosterRequest {
    pub root_id: String,
    /// The compiled main file, relative to the root.
    pub main_rel: String,
    pub widget_id: String,
    /// Absolute path of the PNG to write; its folder must exist.
    pub out_path: String,
    /// Hard limit for the whole render, in milliseconds (default 20000).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub timeout_ms: Option<u64>,
    /// An html widget only: render it only while its folder hashes to this
    /// approval digest (the one the poster's cache key was made from).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub digest: Option<String>,
}

/// A written poster.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct PosterRendered {
    pub widget_id: String,
    pub path: String,
    pub width: u32,
    pub height: u32,
    pub sha256: String,
}

/// What a poster request came to: a written poster, or an html widget the
/// user has not approved (a normal result: nothing ran and nothing was
/// written).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema, TS)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum PosterOutcome {
    Rendered(PosterRendered),
    ApprovalRequired(Box<ApprovalRequired>),
}

/// A request made ready: a job to run, or the approval it lacks.
#[derive(Debug, Clone)]
pub enum Prepared {
    Job(PosterJob),
    ApprovalRequired(Box<ApprovalRequired>),
}

/// One source the runtime receives as bytes, under its role key.
#[derive(Debug, Clone)]
pub struct PosterSourceBytes {
    pub key: String,
    pub bytes: Vec<u8>,
}

/// Everything the renderer needs, decided here.
#[derive(Debug, Clone)]
pub struct PosterJob {
    pub widget_id: String,
    /// The runtime document with the widget policy as its first element.
    pub document: String,
    /// The bridge `init` body: sources carry name, mime and sha256; the
    /// renderer adds each one's bytes.
    pub init: Value,
    /// A custom proposal's second `init` body, in dark mode: the renderer
    /// runs the job once per body and the pair must differ. None for every
    /// other kind.
    pub dark_init: Option<Value>,
    pub sources: Vec<PosterSourceBytes>,
    /// The frame's size in CSS pixels.
    pub frame: (u32, u32),
    /// The exact PNG size the runtime must return, when it is fixed.
    pub expect: Option<(u32, u32)>,
    pub timeout: Duration,
    pub out: PathBuf,
}

/// Where a widget's poster comes from, in precedence order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PosterSource {
    /// The document's own `poster=` (relative to the main file).
    Explicit(String),
    /// An auto-poster rendered earlier.
    Cached(PathBuf),
    /// Nothing yet: a labelled placeholder stands in.
    Placeholder,
}

/// An explicit `poster=` always wins; a cached auto-poster that exists comes
/// next; otherwise a placeholder.
pub fn poster_source(w: &Widget, cached: Option<&Path>) -> PosterSource {
    if let Some(p) = w.poster.as_deref().filter(|p| !p.is_empty()) {
        return PosterSource::Explicit(p.to_string());
    }
    match cached {
        Some(c) if c.is_file() => PosterSource::Cached(c.to_path_buf()),
        _ => PosterSource::Placeholder,
    }
}

fn host_document(runtime: &str) -> Option<&'static str> {
    #[cfg(debug_assertions)]
    if runtime == TEST_HANG_RUNTIME {
        return Some(TEST_HANG_HOST);
    }
    runtime_host(runtime)
}

/// The tokens a poster renders with: the paper's light set. Posters are
/// light in both reader modes (the pdf's picture, and what prints), so a
/// poster is rendered once, in the paper's own colours.
pub(crate) fn poster_tokens(theme: &Theme) -> Map<String, Value> {
    theme
        .tokens(Mode::Light)
        .iter()
        .map(|(k, v)| (k.clone(), Value::from(v.as_str())))
        .collect()
}

fn out_path(s: &str) -> Result<PathBuf, String> {
    let p = PathBuf::from(s);
    if !p.is_absolute() {
        return Err(format!("the poster path must be absolute, not `{s}`"));
    }
    if p.extension().and_then(|e| e.to_str()) != Some("png") {
        return Err(format!("the poster path must end in .png, not `{s}`"));
    }
    if p.is_dir() {
        return Err(format!("the poster path `{s}` is a folder"));
    }
    match p.parent() {
        Some(d) if d.is_dir() => Ok(p),
        _ => Err(format!("the poster path's folder does not exist: `{s}`")),
    }
}

fn clamp_side(v: f64) -> u32 {
    (v.round() as u32).clamp(params::MIN_SIDE, params::MAX_SIDE)
}

/// One source read: its role key, the widget's record of it, its bytes.
type SourceRead<'w> = (String, &'w WidgetSource, Vec<u8>);

/// A widget's sources under their role keys, read from inside the project
/// (paths are relative to the main file's folder).
fn read_sources<'w>(
    cx: &Core,
    root_id: &str,
    main_rel: &str,
    w: &'w Widget,
) -> Result<Vec<SourceRead<'w>>, String> {
    let id = &w.id;
    let main_dir = Path::new(main_rel)
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_default();
    let mut out = Vec::new();
    for (key, s) in role_keys(w)? {
        let path = crate::fs::resolve_in(cx, root_id, &main_dir.join(&s.path).to_string_lossy())
            .map_err(|e| format!("widget {id}: {}: {e}", s.path))?;
        let bytes = std::fs::read(&path)
            .map_err(|e| format!("widget {id}: cannot read {}: {e}", s.path))?;
        out.push((key, s, bytes));
    }
    Ok(out)
}

/// Builds the render job for one widget of a compiled main file. An html
/// widget's approval is checked here, on the snapshot the job is built from;
/// a custom widget's runtime decision is checked the same way, and only an
/// approved runtime is rendered.
pub fn prepare(cx: &Core, req: &PosterRequest) -> Result<Prepared, String> {
    prepare_at(&widget_approval::store_base(), cx, req)
}

/// [`prepare`] against the approval store at `base`.
pub(crate) fn prepare_at(base: &Path, cx: &Core, req: &PosterRequest) -> Result<Prepared, String> {
    let out = out_path(&req.out_path)?;
    let timeout = Duration::from_millis(
        req.timeout_ms
            .unwrap_or(DEFAULT_TIMEOUT_MS)
            .clamp(MIN_TIMEOUT_MS, MAX_TIMEOUT_MS),
    );
    let (list, theme) = read(cx, &req.root_id, &req.main_rel)?;
    let w = list
        .widgets
        .iter()
        .find(|w| w.id == req.widget_id)
        .ok_or_else(|| {
            format!(
                "{}: no widget `{}` (recompile if it was just added)",
                req.main_rel, req.widget_id
            )
        })?;
    let id = &w.id;
    match w.kind {
        WidgetType::Model | WidgetType::Chart => {}
        WidgetType::Html => return html_job(base, cx, req, w, &theme, out, timeout),
        WidgetType::Custom => return custom_job(base, cx, req, w, &theme, out, timeout),
        WidgetType::Table => {
            return Err(format!(
                "widget {id}: a table's poster is its typeset rows, nothing to render"
            ))
        }
        WidgetType::Video => {
            return Err(format!(
                "widget {id}: video posters are not rendered yet; give it poster="
            ))
        }
    }
    if req.digest.is_some() {
        return Err(format!(
            "widget {id}: a digest applies to html and custom widgets only"
        ));
    }
    let runtime = w.runtime.as_deref().unwrap_or("");
    let host = host_document(runtime)
        .ok_or_else(|| format!("widget {id}: no {runtime} runtime in this build"))?;
    let document = fold::with_policy(host, &fold::widget_policy(None));

    let mut sources = Vec::new();
    let mut meta = Map::new();
    for (key, s, bytes) in read_sources(cx, &req.root_id, &req.main_rel, w)? {
        let name = Path::new(&s.path)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| key.clone());
        let ext = Path::new(&s.path)
            .extension()
            .map(|e| e.to_string_lossy().into_owned())
            .unwrap_or_default();
        meta.insert(
            key.clone(),
            json!({ "name": name, "mime": fold::mime_for(&ext), "sha256": sha_of(&bytes) }),
        );
        sources.push(PosterSourceBytes { key, bytes });
    }

    let mut options = runtime_options(w);
    let rect_css = rect_css(w);
    let (frame, expect) = if w.kind == WidgetType::Model {
        let size = match options.get("size").and_then(Value::as_str) {
            Some(s) => params::size(s).map_err(|e| format!("widget {id}: {e}"))?,
            None => {
                let s = (
                    clamp_side(rect_css.0 * f64::from(DEFAULT_DENSITY)),
                    clamp_side(rect_css.1 * f64::from(DEFAULT_DENSITY)),
                );
                options.insert("size".into(), format!("{}x{}", s.0, s.1).into());
                s
            }
        };
        (size, Some(size))
    } else {
        if !options.contains_key("scale") {
            options.insert("scale".into(), DEFAULT_DENSITY.into());
        }
        ((clamp_side(rect_css.0), clamp_side(rect_css.1)), None)
    };

    Ok(Prepared::Job(PosterJob {
        widget_id: id.clone(),
        document,
        init: init(w, runtime, options, meta, &theme, Mode::Light),
        dark_init: None,
        sources,
        frame,
        expect,
        timeout,
        out,
    }))
}

/// The widget's box in CSS pixels.
fn rect_css(w: &Widget) -> (f64, f64) {
    (
        (w.rect.x1 - w.rect.x0) * CSS_PER_PT,
        (w.rect.y1 - w.rect.y0) * CSS_PER_PT,
    )
}

/// The bridge `init` body, in one colour mode.
fn init(
    w: &Widget,
    runtime: &str,
    options: Map<String, Value>,
    sources: Map<String, Value>,
    theme: &Theme,
    mode: Mode,
) -> Value {
    let tokens: Map<String, Value> = theme
        .tokens(mode)
        .iter()
        .map(|(k, v)| (k.clone(), Value::from(v.as_str())))
        .collect();
    let mode = match mode {
        Mode::Light => "light",
        Mode::Dark => "dark",
    };
    json!({
        "type": "init",
        "protocol": 1,
        "widgetId": w.id,
        "runtime": runtime,
        "alt": w.alt,
        "options": options,
        "theme": { "mode": mode, "tokens": tokens },
        "sources": sources,
    })
}

/// The runtime name an html widget's job announces.
pub const HTML_RUNTIME: &str = "html";

/// An html widget's job, built only when its folder is approved, from the
/// one snapshot the approval was judged on: the document is the folded
/// snapshot, so the bytes that run are the bytes that were approved. A
/// folder that cannot be snapshotted (a symlink, a special file, too
/// large, outside the project) fails; an unapproved one comes back as
/// approval_required.
fn html_job(
    base: &Path,
    cx: &Core,
    req: &PosterRequest,
    w: &Widget,
    theme: &Theme,
    out: PathBuf,
    timeout: Duration,
) -> Result<Prepared, String> {
    let id = &w.id;
    let target = WidgetTarget::of(&req.main_rel, w)?
        .ok_or_else(|| format!("widget {id}: an html widget has no bundle folder"))?;
    let checked = widget_approval::check_at(base, cx, &req.root_id, &target)?;
    if let Some(want) = &req.digest {
        if *want != checked.snapshot.digest {
            return Err(format!(
                "widget {id}: its folder changed since the poster was asked for; the next compile renders it"
            ));
        }
    }
    match checked.status {
        WidgetApprovalStatus::ApprovalRequired(r) => Ok(Prepared::ApprovalRequired(Box::new(r))),
        WidgetApprovalStatus::Approved(_) => {
            let folded = fold::fold_bundle(&checked.snapshot.files, &fold::widget_policy(None))
                .map_err(|e| format!("widget {id}: {e}"))?;
            let rect_css = rect_css(w);
            let mut options = runtime_options(w);
            options
                .entry("scale")
                .or_insert_with(|| DEFAULT_DENSITY.into());
            Ok(Prepared::Job(PosterJob {
                widget_id: id.clone(),
                document: folded.html,
                init: init(w, HTML_RUNTIME, options, Map::new(), theme, Mode::Light),
                dark_init: None,
                sources: Vec::new(),
                frame: (clamp_side(rect_css.0), clamp_side(rect_css.1)),
                expect: None,
                timeout,
                out,
            }))
        }
    }
}

/// A custom widget's proposal job, built only while its runtime is
/// approved: allowed at this digest, or an auto-covered content change. A
/// missing, invalid, unapproved, denied or licence-changed runtime renders
/// nothing (the first two are errors, the rest come back as
/// approval_required). The document is the judged snapshot's files folded
/// exactly as the export folds them, so the bytes that run are the bytes
/// that were approved; the runtime then renders them once per colour mode.
fn custom_job(
    base: &Path,
    cx: &Core,
    req: &PosterRequest,
    w: &Widget,
    theme: &Theme,
    out: PathBuf,
    timeout: Duration,
) -> Result<Prepared, String> {
    let id = &w.id;
    let reference = w.runtime.clone().unwrap_or_default();
    if reference.is_empty() {
        return Err(format!("widget {id}: a custom widget records no runtime"));
    }
    let list = read(cx, &req.root_id, &req.main_rel)?.0;
    let users: Vec<String> = list
        .widgets
        .iter()
        .filter(|v| {
            v.kind == WidgetType::Custom && v.runtime.as_deref() == Some(reference.as_str())
        })
        .map(|v| v.id.clone())
        .collect();
    let checked = widget_approval::check_runtime_at(base, cx, &req.root_id, &reference, &users)
        .map_err(|e| format!("widget {id}: {e}"))?;
    let snap = &checked.snapshot;
    let manifest = snap.manifest.as_ref().ok_or_else(|| {
        let reason = snap.invalid.as_deref().unwrap_or("it is not valid");
        let reason = reason
            .strip_prefix(&format!("runtime {reference}: "))
            .unwrap_or(reason);
        format!("widget {id}: runtime {reference} is invalid ({reason})")
    })?;
    if let Some(want) = &req.digest {
        if *want != snap.digest {
            return Err(format!(
                "widget {id}: its runtime changed since the poster was asked for; the next compile renders it"
            ));
        }
    }
    match checked.status {
        None => Err(format!("widget {id}: runtime {reference} cannot be judged")),
        Some(WidgetApprovalStatus::ApprovalRequired(r)) => {
            Ok(Prepared::ApprovalRequired(Box::new(r)))
        }
        Some(WidgetApprovalStatus::Approved(_)) => {
            let bound =
                crate::runtimes::bind(w, manifest).map_err(|e| format!("widget {id}: {e}"))?;
            let main_dir = Path::new(&req.main_rel)
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_default();
            let mut sources = Vec::new();
            let mut meta = Map::new();
            for (role, s) in &bound.sources {
                let path = crate::fs::resolve_in(
                    cx,
                    &req.root_id,
                    &main_dir.join(&s.path).to_string_lossy(),
                )
                .map_err(|e| format!("widget {id}: {}: {e}", s.path))?;
                let bytes = std::fs::read(&path)
                    .map_err(|e| format!("widget {id}: cannot read {}: {e}", s.path))?;
                crate::runtimes::check_size(w, manifest, role, bytes.len() as u64)?;
                let name = Path::new(&s.path)
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| role.clone());
                let ext = Path::new(&s.path)
                    .extension()
                    .map(|e| e.to_string_lossy().into_owned())
                    .unwrap_or_default();
                meta.insert(
                    role.clone(),
                    json!({ "name": name, "mime": fold::mime_for(&ext), "sha256": sha_of(&bytes) }),
                );
                sources.push(PosterSourceBytes {
                    key: role.clone(),
                    bytes,
                });
            }
            // The judged snapshot's files, never a second read of the
            // folder, folded exactly as the export folds them (with the
            // runtime's own policy, so a WASM runtime's proposal can run).
            let fold_input: BTreeMap<String, Vec<u8>> = snap
                .files
                .iter()
                .filter(|(path, _)| !manifest.is_metadata(path))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            let folded = fold::fold_bundle(
                &fold_input,
                &fold::widget_policy_for(None, manifest.capabilities.wasm),
            )
            .map_err(|e| format!("widget {id}: runtime {reference}: {e}"))?;
            let rect_css = rect_css(w);
            Ok(Prepared::Job(PosterJob {
                widget_id: id.clone(),
                document: folded.html,
                init: init(
                    w,
                    &reference,
                    bound.options.clone(),
                    meta.clone(),
                    theme,
                    Mode::Light,
                ),
                dark_init: Some(init(w, &reference, bound.options, meta, theme, Mode::Dark)),
                sources,
                frame: (clamp_side(rect_css.0), clamp_side(rect_css.1)),
                expect: None,
                timeout,
                out,
            }))
        }
    }
}

/// Prepares, executes and finishes one request. `exec` runs a job and
/// returns the runtime's PNG data URL; it is reached only with a job that
/// may run (a first-party runtime, an approved html widget, or an approved
/// custom runtime, each at the snapshot the job holds). A custom proposal
/// runs twice, once per colour mode, and only differing sane non-blank
/// snapshots are written.
pub fn run(
    cx: &Core,
    req: &PosterRequest,
    exec: impl FnMut(std::sync::Arc<PosterJob>) -> Result<String, String>,
) -> Result<PosterOutcome, String> {
    run_at(&widget_approval::store_base(), cx, req, exec)
}

/// [`run`] against the approval store at `base`.
pub(crate) fn run_at(
    base: &Path,
    cx: &Core,
    req: &PosterRequest,
    mut exec: impl FnMut(std::sync::Arc<PosterJob>) -> Result<String, String>,
) -> Result<PosterOutcome, String> {
    match prepare_at(base, cx, req)? {
        Prepared::ApprovalRequired(r) => Ok(PosterOutcome::ApprovalRequired(r)),
        Prepared::Job(job) => {
            let job = std::sync::Arc::new(job);
            if let Some(dark) = job.dark_init.clone() {
                let pair = PosterJob {
                    init: dark,
                    dark_init: None,
                    ..job.as_ref().clone()
                };
                let light = exec(job.clone())?;
                let dark_png = exec(std::sync::Arc::new(pair))?;
                finish_pair(&job, &light, &dark_png).map(PosterOutcome::Rendered)
            } else {
                let png = exec(job.clone())?;
                finish(&job, &png).map(PosterOutcome::Rendered)
            }
        }
    }
}

/// Width and height from a PNG's IHDR, or None when it is not a PNG.
pub fn png_size(b: &[u8]) -> Option<(u32, u32)> {
    const SIG: &[u8] = b"\x89PNG\r\n\x1a\n";
    if b.len() < 24 || &b[..8] != SIG || &b[12..16] != b"IHDR" {
        return None;
    }
    let w = u32::from_be_bytes(b[16..20].try_into().ok()?);
    let h = u32::from_be_bytes(b[20..24].try_into().ok()?);
    (w > 0 && h > 0).then_some((w, h))
}

/// Checks the runtime's snapshot (a PNG data URL) and writes it to the
/// job's path, replacing any earlier file there in one step.
pub fn finish(job: &PosterJob, data_url: &str) -> Result<PosterRendered, String> {
    let png = snapshot_bytes(&job.widget_id, data_url, None)?;
    let (width, height) = png_size(&png)
        .ok_or_else(|| format!("widget {}: the snapshot is not a PNG", job.widget_id))?;
    if let Some((ew, eh)) = job.expect {
        if (width, height) != (ew, eh) {
            return Err(format!(
                "widget {}: the snapshot is {width}x{height}, expected {ew}x{eh}",
                job.widget_id
            ));
        }
    }
    write_png(job, &png, width, height)
}

/// One snapshot data URL decoded: size-capped, PNG magic checked.
fn snapshot_bytes(id: &str, data_url: &str, which: Option<&str>) -> Result<Vec<u8>, String> {
    use base64::Engine;
    let what = match which {
        Some(mode) => format!("the {mode} snapshot"),
        None => "the snapshot".to_string(),
    };
    if data_url.len() >= MAX_SNAPSHOT {
        return Err(format!("widget {id}: {what} is over 8 MiB"));
    }
    let b64 = data_url
        .strip_prefix("data:image/png;base64,")
        .ok_or_else(|| format!("widget {id}: {what} is not a PNG data URL"))?;
    base64::engine::general_purpose::STANDARD
        .decode(b64)
        .map_err(|e| format!("widget {id}: {what} is not valid base64: {e}"))
}

/// A snapshot's pixels as RGB triples, or None when it is not a plain
/// non-interlaced 8-bit PNG (what a canvas snapshot is).
fn decode_png(b: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    use png::{BitDepth, ColorType};
    let mut read = png::Decoder::new(std::io::Cursor::new(b))
        .read_info()
        .ok()?;
    let info = read.info();
    if info.interlaced || info.bit_depth != BitDepth::Eight {
        return None;
    }
    let (w, h) = (info.width, info.height);
    let channels = match info.color_type {
        ColorType::Rgb => 3,
        ColorType::Rgba => 4,
        ColorType::Grayscale => 1,
        ColorType::GrayscaleAlpha => 2,
        _ => return None,
    };
    let size = read.output_buffer_size()?;
    let mut buf = vec![0u8; size];
    read.next_frame(&mut buf).ok()?;
    if buf.len() != w as usize * h as usize * channels {
        return None;
    }
    let rgb = match channels {
        3 => buf,
        4 => buf
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[1], p[2]])
            .collect(),
        1 => buf.iter().flat_map(|v| [*v, *v, *v]).collect(),
        _ => buf
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[0], p[0]])
            .collect(),
    };
    Some((w, h, rgb))
}

/// A proposal snapshot's size and pixels: magic, sane sides, readable
/// pixels.
fn proposal_png(id: &str, bytes: &[u8], which: &str) -> Result<(u32, u32, Vec<u8>), String> {
    let (w, h) =
        png_size(bytes).ok_or_else(|| format!("widget {id}: the {which} snapshot is not a PNG"))?;
    if w == 0 || h == 0 || w > MAX_PROPOSAL_SIDE || h > MAX_PROPOSAL_SIDE {
        return Err(format!(
            "widget {id}: the {which} snapshot is {w}x{h}: a poster side is 1 to {MAX_PROPOSAL_SIDE} pixels"
        ));
    }
    decode_png(bytes)
        .ok_or_else(|| format!("widget {id}: the {which} snapshot's pixels cannot be read"))
        .map(|(w, h, px)| {
            assert_eq!(px.len(), w as usize * h as usize * 3);
            (w, h, px)
        })
}

/// Whether every pixel is the same colour: an unpainted canvas.
fn is_blank(px: &[u8]) -> bool {
    let Some(first) = px.as_chunks::<3>().0.first() else {
        return true;
    };
    px.as_chunks::<3>().0.iter().all(|p| p == first)
}

/// Checks a custom proposal's two snapshots and writes the light one to
/// the job's path, replacing any earlier file there in one step. Each must
/// be a sane non-blank PNG and the two must differ (the runtime answers
/// the theme); anything else writes nothing.
pub fn finish_pair(
    job: &PosterJob,
    light_url: &str,
    dark_url: &str,
) -> Result<PosterRendered, String> {
    let id = &job.widget_id;
    let light = snapshot_bytes(id, light_url, Some("light"))?;
    let dark = snapshot_bytes(id, dark_url, Some("dark"))?;
    let (lw, lh, lp) = proposal_png(id, &light, "light")?;
    let (dw, dh, dp) = proposal_png(id, &dark, "dark")?;
    if is_blank(&lp) {
        return Err(format!("widget {id}: the light snapshot is blank"));
    }
    if is_blank(&dp) {
        return Err(format!("widget {id}: the dark snapshot is blank"));
    }
    if (lw, lh, &lp) == (dw, dh, &dp) {
        return Err(format!(
            "widget {id}: its light and dark snapshots match, so it ignores the theme; no poster was written"
        ));
    }
    if let Some((ew, eh)) = job.expect {
        if (lw, lh) != (ew, eh) {
            return Err(format!(
                "widget {id}: the light snapshot is {lw}x{lh}, expected {ew}x{eh}"
            ));
        }
    }
    write_png(job, &light, lw, lh)
}

/// Writes `png` to the job's path, replacing any earlier file there in one
/// step.
fn write_png(
    job: &PosterJob,
    png: &[u8],
    width: u32,
    height: u32,
) -> Result<PosterRendered, String> {
    let id = &job.widget_id;
    let tmp = job.out.with_extension("png.part");
    std::fs::write(&tmp, png)
        .and_then(|_| std::fs::rename(&tmp, &job.out))
        .map_err(|e| {
            let _ = std::fs::remove_file(&tmp);
            format!("widget {id}: cannot write {}: {e}", job.out.display())
        })?;
    Ok(PosterRendered {
        widget_id: id.clone(),
        path: job.out.to_string_lossy().into_owned(),
        width,
        height,
        sha256: sha_of(png),
    })
}

pub mod cache;

#[cfg(test)]
mod tests;
