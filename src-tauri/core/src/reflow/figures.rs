//! Figures in the converted article, made self-contained.
//!
//! latexml's own graphics conversion is off (we ship no ImageMagick, MuPDF or
//! Ghostscript), so the converter hands over `<img class="ltx_graphics">`
//! elements. [`embed`] turns each into a file or inline data:
//!
//! - raster and SVG files are inlined as `data:` URIs (single-file) or copied
//!   to `figures/<content hash>.<ext>` (folder, deduplicated);
//! - a PDF figure is rasterized (first page, 150 dpi, 2400 px longest side);
//! - EPS, other formats, missing files, remote references and anything
//!   outside the project become a visible placeholder plus a typed warning.
//!
//! Input contract. The converter must record the path exactly as written in
//! `\includegraphics{...}` in a `data-graphic` attribute on the `<img>`.
//! Today latexml's HTML does not: it emits `src=""` with
//! `ltx_missing ltx_missing_image` and drops the path, the options and the
//! graphicspath (see `fixtures/figures-probe.frag`). An `ltx_graphics` image
//! with no `data-graphic` therefore becomes a placeholder with a
//! [`FigureReason::NoSource`] warning, never a silent hole. Whatever `src` it
//! carried is discarded too, so no author-supplied URL survives.
//!
//! Trust boundary. The HTML comes from the author's own source, but a figure
//! path is still untrusted text: it is only ever used to read a file under
//! the canonical project root. Rejected without touching the disk: empty or
//! NUL, absolute (`/x`, `\x`, `C:\x`), any `..` component (also when it would
//! stay inside), and any URL scheme (`http:`, `https:`, `file:`, `data:`, `//`).
//! Nothing is ever fetched; a remote reference becomes a placeholder, so the
//! reader never loads an external image. A candidate is canonicalized and must
//! stay under the root, which catches symlinks that leave it. File names the
//! output uses are made here from a content hash, never from the author's
//! text. The file is read through the canonical path with a hard byte limit,
//! so a file that grows between the check and the read is still capped. A race
//! that swaps a path for a symlink after canonicalization is not defended.
//! The format is decided by the file's magic bytes, not its name.

use crate::guard;
use base64::Engine;
use regex::Regex;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Largest figure file read, in bytes.
pub const MAX_FILE_BYTES: u64 = 10 * 1024 * 1024;
/// Most figure bytes one article may add: each inlined copy counts in
/// single-file mode, each distinct file once in folder mode.
pub const MAX_TOTAL_BYTES: u64 = 64 * 1024 * 1024;
/// PDF figures render at this resolution, capped to [`PDF_MAX_PX`] a side.
pub const PDF_DPI: f32 = 150.0;
pub const PDF_MAX_PX: u32 = 2400;

/// Where the figures go.
#[derive(Debug, Clone)]
pub enum Mode {
    /// Inline every figure as a `data:` URI.
    SingleFile,
    /// Write figures into `<dir>/figures/` and reference them as
    /// `figures/<hash>.<ext>`. `dir` is the bundle folder, chosen by the app.
    Folder { dir: PathBuf },
}

#[derive(Debug, Clone)]
pub struct Options {
    /// `\graphicspath` directories, relative to the project root, in the
    /// order LaTeX searches them after the root itself. Each follows the same
    /// confinement rules as a figure path.
    pub graphics_paths: Vec<String>,
    pub max_file_bytes: u64,
    pub max_total_bytes: u64,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            graphics_paths: Vec::new(),
            max_file_bytes: MAX_FILE_BYTES,
            max_total_bytes: MAX_TOTAL_BYTES,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FigureReason {
    /// The converter recorded no path for this image (`data-graphic` absent).
    NoSource,
    /// Empty, NUL, absolute, `..`, or it resolves outside the project root.
    Escapes,
    /// An `http:`, `https:` or other URL; never fetched.
    Remote,
    /// No file matched, even after trying the usual extensions.
    Missing,
    /// EPS cannot be shown (no Ghostscript); convert it to PDF or PNG.
    Eps,
    /// A format we do not embed, or content that is not an image.
    Unsupported(String),
    /// Over the per-file limit.
    TooLarge(u64),
    /// The article's figures would exceed the total limit.
    TotalCap,
    /// Reading, rendering or writing failed.
    Failed(String),
}

impl FigureReason {
    fn describe(&self) -> String {
        match self {
            FigureReason::NoSource => "the converter recorded no file name".into(),
            FigureReason::Escapes => "the path is not inside the project".into(),
            FigureReason::Remote => "remote images are not loaded".into(),
            FigureReason::Missing => "file not found".into(),
            FigureReason::Eps => "EPS is not supported, use PDF or PNG".into(),
            FigureReason::Unsupported(what) => format!("unsupported format: {what}"),
            FigureReason::TooLarge(n) => format!("file is {n} bytes, over the limit"),
            FigureReason::TotalCap => "the article's figures are over the total size limit".into(),
            FigureReason::Failed(why) => why.clone(),
        }
    }
}

/// A figure that could not be embedded; its place in the article holds a
/// placeholder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FigureWarning {
    /// The reference as the document wrote it (empty for `NoSource`).
    pub file: String,
    pub reason: FigureReason,
}

#[derive(Debug)]
pub struct Embedded {
    pub html: String,
    pub warnings: Vec<FigureWarning>,
}

/// Rewrites every `ltx_graphics` image of `html`. Fails only when the root is
/// not a usable directory or the folder mode cannot create `figures/`; a bad
/// figure is a warning, not an error. Run it once on converter output (an
/// already rewritten article has no `data-graphic` left and would turn into
/// placeholders).
pub fn embed(html: &str, root: &Path, mode: &Mode, opts: &Options) -> Result<Embedded, String> {
    let root = guard::canonical_root(&root.to_string_lossy())?;
    let mut warnings = Vec::new();
    let bases = graphics_bases(opts, &mut warnings);
    let figures_dir = match mode {
        Mode::Folder { dir } => {
            let d = dir.join("figures");
            std::fs::create_dir_all(&d)
                .map_err(|e| format!("cannot create {}: {e}", d.display()))?;
            Some(d)
        }
        Mode::SingleFile => None,
    };
    let mut run = Run {
        root,
        bases,
        opts,
        figures_dir,
        cache: HashMap::new(),
        total: 0,
        counted: Vec::new(),
    };

    let mut out = String::with_capacity(html.len());
    let mut last = 0;
    for m in img_re().find_iter(html) {
        let attrs = parse_attrs(m.as_str());
        let is_figure = attrs
            .iter()
            .any(|(k, v)| k == "class" && v.split_whitespace().any(|c| c == "ltx_graphics"));
        if !is_figure {
            continue;
        }
        out.push_str(&html[last..m.start()]);
        last = m.end();
        let graphic = attrs
            .iter()
            .find(|(k, _)| k == "data-graphic")
            .map(|(_, v)| unescape(v));
        let replacement = match &graphic {
            None => Err(FigureReason::NoSource),
            Some(g) => run.figure(g),
        };
        match replacement {
            Ok(src) => out.push_str(&image_tag(&attrs, &src)),
            Err(reason) => {
                let file = graphic.unwrap_or_default();
                out.push_str(&placeholder(&attrs, &file, &reason));
                warnings.push(FigureWarning { file, reason });
            }
        }
    }
    out.push_str(&html[last..]);
    Ok(Embedded {
        html: out,
        warnings,
    })
}

#[derive(Clone)]
struct Embedding {
    /// `figures/<hash>.<ext>` in folder mode, else the data URI.
    src: String,
    /// Bytes this figure adds to the output.
    size: u64,
}

struct Run<'a> {
    root: PathBuf,
    bases: Vec<String>,
    opts: &'a Options,
    figures_dir: Option<PathBuf>,
    /// By canonical source path, so a figure used twice is read and
    /// rendered once.
    cache: HashMap<PathBuf, Result<std::rc::Rc<Embedding>, FigureReason>>,
    total: u64,
    /// Folder mode: sources already counted toward the total.
    counted: Vec<PathBuf>,
}

impl Run<'_> {
    fn figure(&mut self, graphic: &str) -> Result<String, FigureReason> {
        check_reference(graphic)?;
        let path = self.resolve(graphic)?;
        let emb = match self.cache.get(&path) {
            Some(r) => r.clone(),
            None => {
                let r = self.build(&path).map(std::rc::Rc::new);
                self.cache.insert(path.clone(), r.clone());
                r
            }
        }?;
        // Single-file output repeats an inlined copy per use; folder output
        // holds each distinct file once.
        let counts = self.figures_dir.is_none() || !self.counted.contains(&path);
        if counts {
            if self.total + emb.size > self.opts.max_total_bytes {
                return Err(FigureReason::TotalCap);
            }
            self.total += emb.size;
            if self.figures_dir.is_some() {
                self.counted.push(path);
            }
        }
        Ok(emb.src.clone())
    }

    /// The file `graphic` names, the way LaTeX looks for it: the root first,
    /// then each graphicspath entry; as written, then with the usual
    /// extensions. Every hit must stay under the root after symlinks resolve.
    fn resolve(&self, graphic: &str) -> Result<PathBuf, FigureReason> {
        let names = candidate_names(graphic);
        let mut dirs: Vec<&str> = vec![""];
        dirs.extend(self.bases.iter().map(String::as_str));
        for dir in dirs {
            for name in &names {
                let rel = format!("{dir}{name}");
                let joined = self.root.join(&rel);
                let Ok(canon) = dunce::canonicalize(&joined) else {
                    continue;
                };
                if !canon.starts_with(&self.root) {
                    return Err(FigureReason::Escapes);
                }
                if canon.is_file() {
                    return Ok(canon);
                }
            }
        }
        Err(FigureReason::Missing)
    }

    fn build(&self, path: &Path) -> Result<Embedding, FigureReason> {
        let bytes = read_capped(path, self.opts.max_file_bytes)?;
        let (bytes, kind) = match sniff(&bytes) {
            Kind::Raster(k) => (bytes, k),
            Kind::Pdf => {
                let png = crate::snippet::pdf_first_page_png(bytes, PDF_DPI, PDF_MAX_PX)
                    .map_err(FigureReason::Failed)?;
                (png, RasterKind::Png)
            }
            Kind::Eps => return Err(FigureReason::Eps),
            Kind::Other => {
                let ext = path
                    .extension()
                    .map(|e| e.to_string_lossy().to_lowercase())
                    .unwrap_or_default();
                return Err(FigureReason::Unsupported(if ext.is_empty() {
                    "unrecognized content".into()
                } else {
                    format!("{ext}, unrecognized content")
                }));
            }
        };
        let size = bytes.len() as u64;
        let src = match &self.figures_dir {
            None => format!(
                "data:{};base64,{}",
                kind.mime(),
                base64::engine::general_purpose::STANDARD.encode(&bytes)
            ),
            Some(dir) => {
                let hash = hex(&Sha256::digest(&bytes));
                let name = format!("{}.{}", &hash[..24], kind.ext());
                let target = dir.join(&name);
                if !target.exists() {
                    let tmp = dir.join(format!("{name}.part"));
                    std::fs::write(&tmp, &bytes)
                        .and_then(|_| std::fs::rename(&tmp, &target))
                        .map_err(|e| FigureReason::Failed(format!("cannot write {name}: {e}")))?;
                }
                format!("figures/{name}")
            }
        };
        Ok(Embedding { src, size })
    }
}

/// `\graphicspath` entries that pass the same rules as a figure path, each
/// normalized to end in `/`. A bad entry is dropped with a warning.
fn graphics_bases(opts: &Options, warnings: &mut Vec<FigureWarning>) -> Vec<String> {
    let mut out = Vec::new();
    for entry in &opts.graphics_paths {
        let trimmed = entry.trim_start_matches("./");
        let probe = if trimmed.is_empty() { "x" } else { trimmed };
        match check_reference(probe) {
            Ok(()) => {
                let mut b = trimmed.trim_end_matches('/').to_string();
                if !b.is_empty() {
                    b.push('/');
                }
                out.push(b);
            }
            Err(reason) => warnings.push(FigureWarning {
                file: entry.clone(),
                reason,
            }),
        }
    }
    out
}

/// The pure part of confinement: what a reference may look like.
fn check_reference(raw: &str) -> Result<(), FigureReason> {
    if guard::reject_empty_nul(raw).is_err() {
        return Err(FigureReason::Escapes);
    }
    let lower = raw.to_ascii_lowercase();
    if raw.starts_with("//") || raw.starts_with("\\\\") || has_scheme(&lower) {
        return Err(
            if lower.starts_with("http:") || lower.starts_with("https:") || raw.starts_with("//") {
                FigureReason::Remote
            } else {
                FigureReason::Escapes
            },
        );
    }
    if raw.starts_with('/')
        || raw.starts_with('\\')
        || is_drive_path(raw)
        || Path::new(raw).is_absolute()
    {
        return Err(FigureReason::Escapes);
    }
    if raw.split(['/', '\\']).any(|c| c == "..") {
        return Err(FigureReason::Escapes);
    }
    Ok(())
}

/// `scheme:` at the start: two or more scheme characters before the colon
/// (a single letter is a Windows drive, rejected as absolute).
fn has_scheme(s: &str) -> bool {
    match s.find(':') {
        Some(i) if i >= 2 => s[..i]
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '.' | '-')),
        _ => false,
    }
}

/// `C:\x` or `C:/x`, whatever platform we run on.
fn is_drive_path(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':'
}

const EXTENSIONS: [&str; 8] = ["pdf", "png", "jpg", "jpeg", "gif", "webp", "svg", "eps"];

/// The file names to try for a reference: itself, then with each extension
/// appended (a bare `fig` and `fig.v2` both work), lower and upper case.
fn candidate_names(graphic: &str) -> Vec<String> {
    let mut v = vec![graphic.to_string()];
    for e in EXTENSIONS {
        v.push(format!("{graphic}.{e}"));
    }
    for e in EXTENSIONS {
        v.push(format!("{graphic}.{}", e.to_ascii_uppercase()));
    }
    v
}

/// Reads at most `max` bytes; a longer file is an error, however it grew.
fn read_capped(path: &Path, max: u64) -> Result<Vec<u8>, FigureReason> {
    let file = std::fs::File::open(path)
        .map_err(|e| FigureReason::Failed(format!("cannot read the file: {e}")))?;
    let len = file.metadata().map(|m| m.len()).unwrap_or(0);
    if len > max {
        return Err(FigureReason::TooLarge(len));
    }
    let mut bytes = Vec::new();
    file.take(max + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| FigureReason::Failed(format!("cannot read the file: {e}")))?;
    if bytes.len() as u64 > max {
        return Err(FigureReason::TooLarge(bytes.len() as u64));
    }
    Ok(bytes)
}

#[derive(Clone, Copy)]
enum RasterKind {
    Png,
    Jpeg,
    Gif,
    Webp,
    Svg,
}

impl RasterKind {
    fn mime(self) -> &'static str {
        match self {
            RasterKind::Png => "image/png",
            RasterKind::Jpeg => "image/jpeg",
            RasterKind::Gif => "image/gif",
            RasterKind::Webp => "image/webp",
            RasterKind::Svg => "image/svg+xml",
        }
    }
    fn ext(self) -> &'static str {
        match self {
            RasterKind::Png => "png",
            RasterKind::Jpeg => "jpg",
            RasterKind::Gif => "gif",
            RasterKind::Webp => "webp",
            RasterKind::Svg => "svg",
        }
    }
}

enum Kind {
    Raster(RasterKind),
    Pdf,
    Eps,
    Other,
}

/// The format by magic bytes. A name can lie; the content decides.
fn sniff(b: &[u8]) -> Kind {
    if b.starts_with(b"\x89PNG\r\n\x1a\n") {
        Kind::Raster(RasterKind::Png)
    } else if b.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Kind::Raster(RasterKind::Jpeg)
    } else if b.starts_with(b"GIF87a") || b.starts_with(b"GIF89a") {
        Kind::Raster(RasterKind::Gif)
    } else if b.len() >= 12 && &b[..4] == b"RIFF" && &b[8..12] == b"WEBP" {
        Kind::Raster(RasterKind::Webp)
    } else if b.starts_with(b"%PDF-") {
        Kind::Pdf
    } else if b.starts_with(b"%!PS") || b.starts_with(&[0xC5, 0xD0, 0xD3, 0xC6]) {
        Kind::Eps
    } else if looks_like_svg(b) {
        Kind::Raster(RasterKind::Svg)
    } else {
        Kind::Other
    }
}

fn looks_like_svg(b: &[u8]) -> bool {
    let head = &b[..b.len().min(4096)];
    let Ok(text) =
        std::str::from_utf8(head).or_else(|e| std::str::from_utf8(&head[..e.valid_up_to()]))
    else {
        return false;
    };
    text.trim_start_matches('\u{feff}').contains("<svg")
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn img_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"(?is)<img\b(?:"[^"]*"|'[^']*'|[^>"'])*>"#).unwrap())
}

fn attr_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"(?s)([a-zA-Z_:][-a-zA-Z0-9_:.]*)\s*=\s*(?:"([^"]*)"|'([^']*)')"#).unwrap()
    })
}

/// Attribute name/raw value pairs of one tag, in order. Names are lowercased;
/// values stay as written (entity-escaped).
fn parse_attrs(tag: &str) -> Vec<(String, String)> {
    let inner = tag.trim_start_matches("<img");
    attr_re()
        .captures_iter(inner)
        .map(|c| {
            (
                c[1].to_ascii_lowercase(),
                c.get(2)
                    .or_else(|| c.get(3))
                    .map_or("", |m| m.as_str())
                    .to_string(),
            )
        })
        .collect()
}

fn unescape(v: &str) -> String {
    v.replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

fn escape(v: &str) -> String {
    v.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// The `<img>` with its real `src`, without the bookkeeping attributes and
/// the "missing" classes the converter put on it.
fn image_tag(attrs: &[(String, String)], src: &str) -> String {
    let mut tag = String::from("<img");
    let mut wrote_src = false;
    for (k, v) in attrs {
        match k.as_str() {
            "data-graphic" => {}
            "src" => {
                if !wrote_src {
                    tag.push_str(&format!(" src=\"{src}\""));
                    wrote_src = true;
                }
            }
            "class" => {
                let kept: Vec<&str> = v
                    .split_whitespace()
                    .filter(|c| *c != "ltx_missing" && *c != "ltx_missing_image")
                    .collect();
                tag.push_str(&format!(" class=\"{}\"", kept.join(" ")));
            }
            _ => tag.push_str(&format!(" {k}=\"{v}\"")),
        }
    }
    if !wrote_src {
        tag.push_str(&format!(" src=\"{src}\""));
    }
    tag.push('>');
    tag
}

/// A visible stand-in that keeps the image's id and alt text.
fn placeholder(attrs: &[(String, String)], file: &str, reason: &FigureReason) -> String {
    let id = attrs
        .iter()
        .find(|(k, _)| k == "id")
        .map(|(_, v)| format!(" id=\"{v}\""))
        .unwrap_or_default();
    let name = if file.is_empty() { "a figure" } else { file };
    let label = escape(&format!("Figure not shown: {name} ({})", reason.describe()));
    format!(
        "<span{id} class=\"ltx_missing_figure\" role=\"img\" aria-label=\"{label}\">[{label}]</span>"
    )
}

#[cfg(test)]
mod tests;
