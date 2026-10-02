//! Auto-posters: everything about rendering one widget to a still image
//! except the webview. The app's one-shot renderer asks [`prepare`] for a
//! job (the runtime document under its widget policy, the bridge `init`
//! body, the source bytes, the frame size and the time limit), runs it in a
//! hidden window that is destroyed afterwards, and hands the snapshot back
//! to [`finish`], which checks it and writes the PNG to the caller's path.
//!
//! Only first-party runtimes render: `model@1` and `chart@1`. An `html`
//! widget is never auto-rendered here (running author code needs approval);
//! a table's poster is its typeset rows, and video posters are a separate
//! question.
//!
//! Poster precedence for a widget is [`poster_source`]: an explicit
//! `poster=` always wins, then a cached auto-poster, then a placeholder.

use super::{params, widgets, Widget, WidgetSource, WidgetType};
use crate::bundle::{fold, role_keys, runtime_host, runtime_options, sha_of, THEME_CSS};
use crate::Core;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
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

/// The house theme's light tokens, as the runtimes take them.
fn light_tokens() -> Map<String, Value> {
    let block = THEME_CSS
        .split_once('{')
        .and_then(|(_, rest)| rest.split_once('}'))
        .map(|(b, _)| b)
        .unwrap_or("");
    block
        .split(';')
        .filter_map(|d| d.split_once(':'))
        .filter(|(k, _)| k.trim().starts_with("--m-"))
        .map(|(k, v)| (k.trim().to_string(), Value::from(v.trim())))
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

/// Builds the render job for one widget of a compiled main file.
pub fn prepare(cx: &Core, req: &PosterRequest) -> Result<PosterJob, String> {
    let out = out_path(&req.out_path)?;
    let timeout = Duration::from_millis(
        req.timeout_ms
            .unwrap_or(DEFAULT_TIMEOUT_MS)
            .clamp(MIN_TIMEOUT_MS, MAX_TIMEOUT_MS),
    );
    let list = widgets(cx, &req.root_id, &req.main_rel)?;
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
        WidgetType::Html => {
            return Err(format!(
                "widget {id}: html widgets are not rendered to posters; give it poster="
            ))
        }
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
    let rect_css = (
        (w.rect.x1 - w.rect.x0) * CSS_PER_PT,
        (w.rect.y1 - w.rect.y0) * CSS_PER_PT,
    );
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

    let init = json!({
        "type": "init",
        "protocol": 1,
        "widgetId": id,
        "runtime": runtime,
        "alt": w.alt,
        "options": options,
        "theme": { "mode": "light", "tokens": light_tokens() },
        "sources": meta,
    });
    Ok(PosterJob {
        widget_id: id.clone(),
        document,
        init,
        sources,
        frame,
        expect,
        timeout,
        out,
    })
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
    use base64::Engine;
    let id = &job.widget_id;
    if data_url.len() >= MAX_SNAPSHOT {
        return Err(format!("widget {id}: the snapshot is over 8 MiB"));
    }
    let b64 = data_url
        .strip_prefix("data:image/png;base64,")
        .ok_or_else(|| format!("widget {id}: the snapshot is not a PNG data URL"))?;
    let png = base64::engine::general_purpose::STANDARD
        .decode(b64)
        .map_err(|e| format!("widget {id}: the snapshot is not valid base64: {e}"))?;
    let (width, height) =
        png_size(&png).ok_or_else(|| format!("widget {id}: the snapshot is not a PNG"))?;
    if let Some((ew, eh)) = job.expect {
        if (width, height) != (ew, eh) {
            return Err(format!(
                "widget {id}: the snapshot is {width}x{height}, expected {ew}x{eh}"
            ));
        }
    }
    let tmp = job.out.with_extension("png.part");
    std::fs::write(&tmp, &png)
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
        sha256: sha_of(&png),
    })
}

pub mod cache;

#[cfg(test)]
mod tests;
