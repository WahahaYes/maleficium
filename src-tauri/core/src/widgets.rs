//! The typed widget list of one compiled main file: the sidecar
//! `maleficium-interactive.sty` leaves in the outdir (what each widget is)
//! joined with the named annotations in the compiled PDF (where each one
//! sits). Neither half is trusted alone: an id on one side only means the
//! pair is stale or broken, and that is an error, never a shorter list.
//!
//! Annotations live in object streams, so they are read through the PDF
//! parser (which decompresses them), never by scanning bytes. The CSP of an
//! `html` widget is its bundle's `widget.json`, read from inside the project.

use crate::Core;

pub mod params;
pub mod poster;

use hayro::hayro_syntax::object::{Array, Dict, Rect, String as PdfString};
use hayro::hayro_syntax::Pdf;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use ts_rs::TS;

/// The annotation name prefix the package writes: `/NM (mfw:<id>)`.
const NAME_PREFIX: &str = "mfw:";
/// The sidecar's first line: the format and its version.
const SIDECAR_HEADER: &str = "mfw 1";
/// Fields of one `widget|...` line, the tag included.
const SIDECAR_FIELDS: usize = 11;
/// The optional per-bundle declaration beside an `html` widget's entry.
pub(crate) const BUNDLE_MANIFEST: &str = "widget.json";
/// The largest `widget.json` read: it declares a few origins, nothing more.
pub const MAX_MANIFEST_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, TS)]
#[serde(rename_all = "lowercase")]
pub enum WidgetType {
    Model,
    Video,
    Table,
    Chart,
    Html,
}

impl WidgetType {
    pub(crate) fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "model" => Self::Model,
            "video" => Self::Video,
            "table" => Self::Table,
            "chart" => Self::Chart,
            "html" => Self::Html,
            _ => return None,
        })
    }
}

/// An axis-aligned rectangle in PDF user units, origin bottom-left,
/// `x0 < x1` and `y0 < y1`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, schemars::JsonSchema, TS)]
pub struct WidgetRect {
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
}

/// A runtime input: its role (`model`, `video`, `data`, `spec`, `bundle`)
/// and the path the document gave, relative to the main file's folder. A
/// role can repeat (a chart's data files).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, TS)]
pub struct WidgetSource {
    pub role: String,
    pub path: String,
}

/// A runtime option as the document gave it (`height` in pt, `pdfrows`,
/// `remote`, `sha256`). Options left empty are not listed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, TS)]
pub struct WidgetOption {
    pub key: String,
    pub value: String,
}

/// Extra origins a widget declares it needs, as `widget.json` writes them
/// (an `html` widget may also give frame and resource origins as macro
/// options). A host renders these into that widget's policy only; they are
/// part of the approval digest, so they widen nothing the user has not
/// approved.
#[derive(
    Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, TS,
)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct WidgetCsp {
    pub connect_domains: Vec<String>,
    pub resource_domains: Vec<String>,
    pub frame_domains: Vec<String>,
}

impl WidgetCsp {
    pub(crate) fn is_empty(&self) -> bool {
        self.connect_domains.is_empty()
            && self.resource_domains.is_empty()
            && self.frame_domains.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct Widget {
    pub id: String,
    #[serde(rename = "type")]
    #[ts(rename = "type")]
    pub kind: WidgetType,
    /// Built-in runtime and major version (`model@1`); absent for `html`.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub runtime: Option<String>,
    /// LaTeX label of the enclosing float; absent when the macro sat before
    /// the caption (recorded empty rather than guessed).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub label: Option<String>,
    /// The float's rendered number, absent with the label.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub figure: Option<String>,
    pub theme: String,
    /// Poster image path; absent for a table (the typeset rows are it).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub poster: Option<String>,
    pub sources: Vec<WidgetSource>,
    pub options: Vec<WidgetOption>,
    pub alt: String,
    /// 1-based page of the annotation.
    pub page: u32,
    pub rect: WidgetRect,
    /// Declared extra origins; absent when the widget declares none.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub csp: Option<WidgetCsp>,
}

/// Every widget of one compiled main file, in document order (page, then
/// top to bottom, then left to right). Empty only when the document
/// declares none.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct WidgetList {
    pub widgets: Vec<Widget>,
}

/// One parsed `widget|...` line, before the PDF supplies page and rect.
#[derive(Debug, Clone, PartialEq)]
pub struct Record {
    pub id: String,
    pub kind: WidgetType,
    pub runtime: Option<String>,
    pub label: Option<String>,
    pub figure: Option<String>,
    pub theme: String,
    pub poster: Option<String>,
    pub sources: Vec<WidgetSource>,
    pub options: Vec<WidgetOption>,
    pub alt: String,
}

fn opt(s: &str) -> Option<String> {
    (!s.is_empty()).then(|| s.to_string())
}

fn valid_id(id: &str) -> bool {
    let b = id.as_bytes();
    let edge = |c: u8| c.is_ascii_lowercase() || c.is_ascii_digit();
    !b.is_empty()
        && b.len() <= 63
        && edge(b[0])
        && edge(b[b.len() - 1])
        && b.iter().all(|&c| edge(c) || c == b'-')
}

/// `k=v,k=v`: every entry must carry a `=`; entries with an empty value are
/// left out. A segment without `=` is a malformed line, not something to
/// skip (a value containing a comma lands here and says so).
fn pairs(field: &str, what: &str, id: &str) -> Result<Vec<(String, String)>, String> {
    let mut out = Vec::new();
    if field.is_empty() {
        return Ok(out);
    }
    for seg in field.split(',') {
        let (k, v) = seg
            .split_once('=')
            .ok_or_else(|| format!("widget {id}: malformed {what} entry `{seg}` in the sidecar"))?;
        if k.is_empty() {
            return Err(format!("widget {id}: empty {what} key in the sidecar"));
        }
        if !v.is_empty() {
            out.push((k.to_string(), v.to_string()));
        }
    }
    Ok(out)
}

pub fn parse_sidecar(text: &str) -> Result<Vec<Record>, String> {
    let mut lines = text.lines();
    match lines.next() {
        Some(SIDECAR_HEADER) => {}
        other => {
            return Err(format!(
                "unsupported widget sidecar: expected `{SIDECAR_HEADER}`, found `{}`",
                other.unwrap_or("")
            ))
        }
    }
    let mut out: Vec<Record> = Vec::new();
    for (n, line) in lines.enumerate() {
        let n = n + 2;
        if line.trim().is_empty() {
            continue;
        }
        let f: Vec<&str> = line.split('|').collect();
        if f[0] != "widget" {
            return Err(format!(
                "widget sidecar line {n}: unknown record `{}`",
                f[0]
            ));
        }
        if f.len() != SIDECAR_FIELDS {
            return Err(format!(
                "widget sidecar line {n}: {} fields, expected {SIDECAR_FIELDS}",
                f.len()
            ));
        }
        let id = f[1];
        if !valid_id(id) {
            return Err(format!("widget sidecar line {n}: invalid id `{id}`"));
        }
        if out.iter().any(|r| r.id == id) {
            return Err(format!("widget sidecar line {n}: duplicate id `{id}`"));
        }
        let kind = WidgetType::parse(f[2])
            .ok_or_else(|| format!("widget {id}: unknown type `{}` in the sidecar", f[2]))?;
        if f[10].trim().is_empty() {
            return Err(format!("widget {id}: the sidecar records no alt text"));
        }
        out.push(Record {
            id: id.to_string(),
            kind,
            runtime: opt(f[3]),
            label: opt(f[4]),
            figure: opt(f[5]),
            theme: f[6].to_string(),
            poster: opt(f[7]),
            sources: pairs(f[8], "source", id)?
                .into_iter()
                .map(|(role, path)| WidgetSource { role, path })
                .collect(),
            options: pairs(f[9], "option", id)?
                .into_iter()
                .map(|(key, value)| {
                    params::canonical(kind, &key, &value)
                        .map(|value| WidgetOption { key, value })
                        .map_err(|e| format!("widget {id}: {e}"))
                })
                .collect::<Result<_, _>>()?,
            alt: f[10].to_string(),
        });
    }
    Ok(out)
}

/// A named annotation found on a page.
struct Mark {
    id: String,
    page: u32,
    rect: WidgetRect,
}

fn round3(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}

/// Every `/NM (mfw:<id>)` annotation of the PDF with its page and `/Rect`.
/// The parser resolves object streams; annotations carrying other names are
/// not ours and are left alone.
fn marks(bytes: Vec<u8>) -> Result<Vec<Mark>, String> {
    let pdf = Pdf::new(bytes).map_err(|e| format!("cannot read the pdf: {e:?}"))?;
    let mut out: Vec<Mark> = Vec::new();
    for (i, page) in pdf.pages().iter().enumerate() {
        let page_no = i as u32 + 1;
        let Some(annots) = page.raw().get::<Array>("Annots") else {
            continue;
        };
        for annot in annots.iter::<Dict>() {
            let Some(name) = annot.get::<PdfString>("NM") else {
                continue;
            };
            let Some(id) = std::str::from_utf8(name.as_bytes())
                .ok()
                .and_then(|n| n.strip_prefix(NAME_PREFIX))
            else {
                continue;
            };
            let r = annot.get::<Rect>("Rect").ok_or_else(|| {
                format!("annotation {NAME_PREFIX}{id} on page {page_no} has no /Rect")
            })?;
            if out.iter().any(|m| m.id == id) {
                return Err(format!(
                    "annotation {NAME_PREFIX}{id} appears twice in the pdf"
                ));
            }
            out.push(Mark {
                id: id.to_string(),
                page: page_no,
                rect: WidgetRect {
                    x0: round3(r.x0),
                    y0: round3(r.y0),
                    x1: round3(r.x1),
                    y1: round3(r.y1),
                },
            });
        }
    }
    Ok(out)
}

/// The most origins one widget may declare, all directives together.
pub const MAX_ORIGINS: usize = 16;
/// The longest origin accepted: `https://`, a full-length DNS name, a port.
const MAX_ORIGIN_LEN: usize = 8 + 253 + 6;

/// One declared origin: `https://` then a lowercase DNS name (or IPv4
/// address) of 1-63 character labels, and an optional port 1-65535 written
/// without leading zeros. Nothing else: no other scheme (`http:`, `data:`,
/// `blob:`, `javascript:`), no wildcard, no credentials, path, query or
/// trailing dot, and no character that could end a directive or a policy.
/// The origin lands verbatim in a content security policy.
pub(crate) fn origin_ok(s: &str) -> bool {
    if s.len() > MAX_ORIGIN_LEN {
        return false;
    }
    let Some(rest) = s.strip_prefix("https://") else {
        return false;
    };
    let (host, port) = match rest.split_once(':') {
        Some((h, p)) => (h, Some(p)),
        None => (rest, None),
    };
    let label_ok = |l: &str| {
        let b = l.as_bytes();
        !b.is_empty()
            && b.len() <= 63
            && b[0] != b'-'
            && b[b.len() - 1] != b'-'
            && b.iter()
                .all(|&c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
    };
    let port_ok = |p: &str| {
        !p.is_empty()
            && p.len() <= 5
            && !p.starts_with('0')
            && p.bytes().all(|c| c.is_ascii_digit())
            && p.parse::<u32>().is_ok_and(|n| (1..=65535).contains(&n))
    };
    host.len() <= 253 && host.split('.').all(label_ok) && port.is_none_or(port_ok)
}

/// The option keys an `html` widget may declare origins with in its macro,
/// and the directive each one feeds. Values are origins separated by spaces.
pub const ORIGIN_OPTIONS: [&str; 2] = ["framedomains", "resourcedomains"];

/// One origin option's value checked and in canonical form: sorted,
/// deduplicated, space separated.
pub(crate) fn canonical_origins(key: &str, value: &str) -> Result<String, String> {
    let mut list: Vec<&str> = value.split_whitespace().collect();
    if list.is_empty() {
        return Err(format!("{key}= names no origin"));
    }
    for o in &list {
        if !origin_ok(o) {
            return Err(format!(
                "{key}= names `{o}`; only https origins (lowercase host and optional port) are allowed"
            ));
        }
    }
    list.sort_unstable();
    list.dedup();
    if list.len() > MAX_ORIGINS {
        return Err(format!(
            "{key}= names {} origins; at most {MAX_ORIGINS}",
            list.len()
        ));
    }
    Ok(list.join(" "))
}

/// The origins a widget's macro options declare (already canonical: the
/// widget list checked them).
pub(crate) fn option_origins(options: &[WidgetOption]) -> WidgetCsp {
    let list = |key: &str| -> Vec<String> {
        options
            .iter()
            .filter(|o| o.key == key)
            .flat_map(|o| o.value.split_whitespace().map(str::to_string))
            .collect()
    };
    WidgetCsp {
        connect_domains: Vec::new(),
        resource_domains: list("resourcedomains"),
        frame_domains: list("framedomains"),
    }
}

/// The origins a widget declares, `widget.json` and macro options together:
/// every origin checked again, each directive sorted and deduplicated, and
/// at most [`MAX_ORIGINS`] in all. The one place the two sources meet, for
/// the widget list and the approval snapshot alike.
pub(crate) fn declared_origins(
    id: &str,
    manifest: &WidgetCsp,
    options: &WidgetCsp,
) -> Result<WidgetCsp, String> {
    let join = |a: &Vec<String>, b: &Vec<String>| {
        let mut v: Vec<String> = a.iter().chain(b).cloned().collect();
        v.sort();
        v.dedup();
        v
    };
    let out = WidgetCsp {
        connect_domains: join(&manifest.connect_domains, &options.connect_domains),
        resource_domains: join(&manifest.resource_domains, &options.resource_domains),
        frame_domains: join(&manifest.frame_domains, &options.frame_domains),
    };
    let all: Vec<&String> = out
        .connect_domains
        .iter()
        .chain(&out.resource_domains)
        .chain(&out.frame_domains)
        .collect();
    if let Some(bad) = all.iter().find(|o| !origin_ok(o)) {
        return Err(format!(
            "widget {id}: declares `{bad}`; only https origins (lowercase host and optional port) are allowed"
        ));
    }
    if all.len() > MAX_ORIGINS {
        return Err(format!(
            "widget {id}: declares {} origins; at most {MAX_ORIGINS}",
            all.len()
        ));
    }
    Ok(out)
}

/// The `csp` of an `html` widget's bundle folder: `widget.json` beside its
/// `index.html`, if there is one. The folder must exist and sit inside the
/// project; a manifest that does not parse, or names anything but `https`
/// origins, fails the whole call (an origin lands in a response header).
fn bundle_csp(
    cx: &Core,
    root_id: &str,
    main_dir_rel: &std::path::Path,
    id: &str,
    bundle: &str,
) -> Result<Option<WidgetCsp>, String> {
    let dir_rel = main_dir_rel.join(bundle);
    let dir = crate::fs::resolve_in(cx, root_id, &dir_rel.to_string_lossy())
        .map_err(|e| format!("widget {id}: bundle folder {bundle}: {e}"))?;
    let manifest = dir.join(BUNDLE_MANIFEST);
    if std::fs::symlink_metadata(&manifest).is_err() {
        return Ok(None);
    }
    let rel = dir_rel.join(BUNDLE_MANIFEST);
    let path = crate::fs::resolve_in(cx, root_id, &rel.to_string_lossy())
        .map_err(|e| format!("widget {id}: {BUNDLE_MANIFEST}: {e}"))?;
    use std::io::Read as _;
    let cant = |e: std::io::Error| format!("widget {id}: cannot read {BUNDLE_MANIFEST}: {e}");
    let mut bytes = Vec::new();
    std::fs::File::open(&path)
        .map_err(cant)?
        .take(MAX_MANIFEST_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(cant)?;
    if bytes.len() > MAX_MANIFEST_BYTES {
        return Err(too_big(id));
    }
    let text = String::from_utf8(bytes)
        .map_err(|_| format!("widget {id}: {BUNDLE_MANIFEST} is not UTF-8"))?;
    manifest_csp(id, &text)
}

fn too_big(id: &str) -> String {
    format!(
        "widget {id}: {BUNDLE_MANIFEST} is larger than {} KiB",
        MAX_MANIFEST_BYTES / 1024
    )
}

/// The `csp` a bundle's `widget.json` text declares, validated: one parser
/// for the widget list and the approval digest. Text over
/// [`MAX_MANIFEST_BYTES`] is refused before it is parsed.
pub(crate) fn manifest_csp(id: &str, text: &str) -> Result<Option<WidgetCsp>, String> {
    if text.len() > MAX_MANIFEST_BYTES {
        return Err(too_big(id));
    }
    let json: serde_json::Value = serde_json::from_str(text)
        .map_err(|e| format!("widget {id}: {BUNDLE_MANIFEST} is not valid JSON: {e}"))?;
    let Some(csp) = json.get("csp") else {
        return Ok(None);
    };
    let csp: WidgetCsp = serde_json::from_value(csp.clone())
        .map_err(|e| format!("widget {id}: {BUNDLE_MANIFEST} csp: {e}"))?;
    for origin in csp
        .connect_domains
        .iter()
        .chain(&csp.resource_domains)
        .chain(&csp.frame_domains)
    {
        if !origin_ok(origin) {
            return Err(format!(
                "widget {id}: {BUNDLE_MANIFEST} declares `{origin}`; only https origins (lowercase host and optional port) are allowed"
            ));
        }
    }
    let csp = declared_origins(id, &csp, &WidgetCsp::default())
        .map_err(|e| format!("{e} ({BUNDLE_MANIFEST})"))?;
    Ok((!csp.is_empty()).then_some(csp))
}

/// Join sidecar records with annotation marks. Pure, so the stale cases pin
/// without a project.
fn join(records: Vec<Record>, marks: Vec<Mark>) -> Result<Vec<(Record, Mark)>, String> {
    let mut by_id: BTreeMap<String, Mark> = marks.into_iter().map(|m| (m.id.clone(), m)).collect();
    let mut missing = Vec::new();
    let mut joined = Vec::new();
    for r in records {
        match by_id.remove(&r.id) {
            Some(m) => joined.push((r, m)),
            None => missing.push(r.id),
        }
    }
    let orphans: Vec<String> = by_id.into_keys().collect();
    if !missing.is_empty() || !orphans.is_empty() {
        let mut parts = Vec::new();
        if !missing.is_empty() {
            parts.push(format!(
                "in the sidecar but not the pdf: {}",
                missing.join(", ")
            ));
        }
        if !orphans.is_empty() {
            parts.push(format!(
                "in the pdf but not the sidecar: {}",
                orphans.join(", ")
            ));
        }
        return Err(format!(
            "the widget sidecar and the pdf disagree ({}); recompile the document",
            parts.join("; ")
        ));
    }
    Ok(joined)
}

/// Where the package leaves the sidecar: `<jobname>.mfw` in the outdir.
fn sidecar_file(o: &crate::MainOutputs) -> std::path::PathBuf {
    o.outdir.join(format!(
        "{}.mfw",
        o.main_file.strip_suffix(".tex").unwrap_or(&o.main_file)
    ))
}

/// The widgets of `main_rel`'s last compile. Fails when the document was
/// never compiled, when the sidecar is unreadable or malformed, when the
/// PDF carries widgets its sidecar does not (or the reverse), and when a
/// widget's bundle manifest is invalid. A compile of a document without
/// widgets leaves neither sidecar nor annotations and lists nothing.
pub fn widgets(cx: &Core, root_id: &str, main_rel: &str) -> Result<WidgetList, String> {
    let o = super::outputs::outputs_of(cx, root_id, main_rel)?;
    let pdf_path = o.outdir.join(&o.pdf_name);
    let bytes = std::fs::read(&pdf_path)
        .map_err(|_| format!("{main_rel} has no compiled pdf: compile it first"))?;
    let found = marks(bytes).map_err(|e| format!("{main_rel}: {e}"))?;

    let sidecar_path = sidecar_file(&o);
    let records = match std::fs::read_to_string(&sidecar_path) {
        Ok(text) => parse_sidecar(&text).map_err(|e| format!("{main_rel}: {e}"))?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            if found.is_empty() {
                return Ok(WidgetList {
                    widgets: Vec::new(),
                });
            }
            return Err(format!(
                "{main_rel}: the pdf carries widget annotations ({}) but the compile left no widget sidecar; recompile the document",
                found.iter().map(|m| m.id.as_str()).collect::<Vec<_>>().join(", ")
            ));
        }
        Err(e) => return Err(format!("{main_rel}: cannot read the widget sidecar: {e}")),
    };

    let main_dir_rel = std::path::Path::new(main_rel)
        .parent()
        .map(std::path::Path::to_path_buf)
        .unwrap_or_default();
    let mut out = Vec::new();
    for (r, m) in join(records, found).map_err(|e| format!("{main_rel}: {e}"))? {
        let csp = match r.kind {
            WidgetType::Html => match r.sources.iter().find(|s| s.role == "bundle") {
                Some(s) => {
                    let file = bundle_csp(
                        cx,
                        root_id,
                        &main_dir_rel,
                        &r.id,
                        s.path.trim_end_matches('/'),
                    )?;
                    let all = declared_origins(
                        &r.id,
                        &file.unwrap_or_default(),
                        &option_origins(&r.options),
                    )?;
                    (!all.is_empty()).then_some(all)
                }
                None => return Err(format!("widget {}: an html widget records no bundle", r.id)),
            },
            _ => None,
        };
        out.push(Widget {
            id: r.id,
            kind: r.kind,
            runtime: r.runtime,
            label: r.label,
            figure: r.figure,
            theme: r.theme,
            poster: r.poster,
            sources: r.sources,
            options: r.options,
            alt: r.alt,
            page: m.page,
            rect: m.rect,
            csp,
        });
    }
    out.sort_by(|a, b| {
        a.page
            .cmp(&b.page)
            .then(b.rect.y1.total_cmp(&a.rect.y1))
            .then(a.rect.x0.total_cmp(&b.rect.x0))
    });
    Ok(WidgetList { widgets: out })
}

/// Each widget's sidecar record as the package wrote it, runtime, sources
/// and options joined by `|`, by id: the guard a poster map entry carries,
/// so the package uses a cached poster only for the exact record it was
/// rendered from. Empty when the document was never compiled or records
/// no widgets.
pub(crate) fn sidecar_guards(
    cx: &Core,
    root_id: &str,
    main_rel: &str,
) -> Result<BTreeMap<String, String>, String> {
    let o = super::outputs::outputs_of(cx, root_id, main_rel)?;
    let path = sidecar_file(&o);
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeMap::new()),
        Err(e) => return Err(format!("{main_rel}: cannot read the widget sidecar: {e}")),
    };
    Ok(guards_of(&text))
}

fn guards_of(text: &str) -> BTreeMap<String, String> {
    text.lines()
        .skip(1)
        .map(|l| l.split('|').collect::<Vec<_>>())
        .filter(|f| f.len() == SIDECAR_FIELDS && f[0] == "widget")
        .map(|f| (f[1].to_string(), format!("{}|{}|{}", f[3], f[8], f[9])))
        .collect()
}

/// The bus event for one `widgets` call, shared by the adapters that log it.
pub fn event(
    main_rel: &str,
    r: &Result<WidgetList, String>,
    actor: maleficium_events::Actor,
) -> maleficium_events::BusEvent {
    use maleficium_events::{AppEvent, BusEvent, EventKind, EventScope};
    let (kind, message, event) = match r {
        Ok(l) => (
            EventKind::Success,
            format!("{} widgets in {main_rel}", l.widgets.len()),
            AppEvent::WidgetsRead {
                main: main_rel.to_string(),
                count: l.widgets.len() as u32,
            },
        ),
        Err(e) => (
            EventKind::Error,
            format!("widgets of {main_rel} unavailable"),
            AppEvent::WidgetsFailed {
                main: main_rel.to_string(),
                error: e.chars().take(300).collect(),
            },
        ),
    };
    BusEvent {
        at: crate::eventlog::now_ms(),
        scope: EventScope::App,
        kind,
        actor,
        message,
        event,
    }
}

#[cfg(test)]
mod tests;
