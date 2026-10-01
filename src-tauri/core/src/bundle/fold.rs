//! Folding a widget into one inline document, and the content security
//! policy that document carries.
//!
//! Every widget of an exported bundle is one self-contained html document in
//! every profile (browser export spike, 2026-09-30): scripts and styles
//! inline, local files as data URLs, nothing loaded by path. The strict
//! policy then needs no `'self'` and no host, so it behaves the same for any
//! base URL and from file://. A file the author's page fetches at run time
//! cannot be seen from here; the exporter lists every file it did not inline.

use crate::widgets::WidgetCsp;

use base64::Engine;
use regex::{Captures, Regex};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// The widget document's policy before any declared origins: inline only.
/// Sources reach a runtime as bytes over `postMessage`, so nothing here
/// needs a host.
const WIDGET_POLICY: &str = "default-src 'none'; script-src 'unsafe-inline'{res}; style-src 'unsafe-inline'{res}; img-src data: blob:{res}; media-src data: blob:{res}; font-src data:{res}; connect-src {connect}; {frame}form-action 'none'; base-uri 'none'";

/// The reader page of a folder or hosted bundle. Its `frame-src 'self'` is
/// what stops a widget navigating its own frame off-site.
pub const FOLDER_READER_POLICY: &str = "default-src 'none'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; connect-src 'self'; frame-src 'self'; form-action 'none'; base-uri 'none'";

/// The reader page of a single-file bundle. No `frame-src`: it falls back
/// to `'none'`, which admits srcdoc frames and blocks `data:` ones and any
/// navigation.
pub const SINGLE_FILE_READER_POLICY: &str = "default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src data: blob:; media-src data: blob:; font-src data:; connect-src 'none'; form-action 'none'; base-uri 'none'";

/// The policy a widget document carries: strict, plus the origins its
/// `widget.json` declared (an origin is already checked as `https` by the
/// widget list).
pub fn widget_policy(csp: Option<&WidgetCsp>) -> String {
    let empty = WidgetCsp::default();
    let c = csp.unwrap_or(&empty);
    let res = if c.resource_domains.is_empty() {
        String::new()
    } else {
        format!(" {}", c.resource_domains.join(" "))
    };
    let connect = if c.connect_domains.is_empty() {
        "'none'".to_string()
    } else {
        c.connect_domains.join(" ")
    };
    let frame = if c.frame_domains.is_empty() {
        String::new()
    } else {
        format!("frame-src {}; ", c.frame_domains.join(" "))
    };
    WIDGET_POLICY
        .replace("{res}", &res)
        .replace("{connect}", &connect)
        .replace("{frame}", &frame)
}

fn meta_csp(policy: &str) -> String {
    format!("<meta http-equiv=\"Content-Security-Policy\" content=\"{policy}\">")
}

/// Puts the policy meta first in `<head>` (creating the head position when
/// the document has none), and a charset meta behind it when the document
/// declares none, so the policy is the first element that exists.
pub fn with_policy(html: &str, policy: &str) -> String {
    let meta = meta_csp(policy);
    let has_charset = Regex::new(r#"(?i)<meta[^>]*\bcharset\s*="#)
        .unwrap()
        .is_match(html);
    let ins = if has_charset {
        meta
    } else {
        format!("{meta}<meta charset=\"utf-8\">")
    };
    let head = Regex::new(r"(?i)<head(\s[^>]*)?>").unwrap();
    if let Some(m) = head.find(html) {
        return format!("{}{ins}{}", &html[..m.end()], &html[m.end()..]);
    }
    let root = Regex::new(r"(?i)<html(\s[^>]*)?>").unwrap();
    if let Some(m) = root.find(html) {
        return format!("{}<head>{ins}</head>{}", &html[..m.end()], &html[m.end()..]);
    }
    let doctype = Regex::new(r"(?i)^\s*<!doctype[^>]*>").unwrap();
    if let Some(m) = doctype.find(html) {
        return format!("{}{ins}{}", &html[..m.end()], &html[m.end()..]);
    }
    format!("{ins}{html}")
}

/// Escapes text for a double-quoted html attribute.
pub fn attr(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Escapes text for html element content.
pub fn text(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// The media type an extension stands for; unknown ones are opaque bytes.
pub fn mime_for(ext: &str) -> &'static str {
    match ext.to_ascii_lowercase().as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "svg" => "image/svg+xml",
        "ico" => "image/x-icon",
        "glb" => "model/gltf-binary",
        "gltf" => "model/gltf+json",
        "stl" => "model/stl",
        "obj" => "model/obj",
        "mp4" | "m4v" => "video/mp4",
        "webm" => "video/webm",
        "ogv" => "video/ogg",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "ogg" => "audio/ogg",
        "csv" => "text/csv",
        "tsv" => "text/tab-separated-values",
        "txt" => "text/plain",
        "json" => "application/json",
        "css" => "text/css",
        "js" | "mjs" => "text/javascript",
        "html" | "htm" => "text/html",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        "wasm" => "application/wasm",
        "pdf" => "application/pdf",
        _ => "application/octet-stream",
    }
}

fn data_url(mime: &str, bytes: &[u8]) -> String {
    format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    )
}

/// The outcome of folding one author bundle.
#[derive(Debug, Default)]
pub struct Folded {
    pub html: String,
    /// Bundle files that nothing inlined: they cannot load from the folded
    /// document (relative paths inside the bundle folder).
    pub unfolded: Vec<String>,
    /// References to other places (urls) left as written; the policy blocks
    /// them unless the widget declared the origin.
    pub external: Vec<String>,
    /// Other things the author should know (module imports, `@import`).
    pub notes: Vec<String>,
}

struct Folder<'a> {
    /// The canonical bundle folder: nothing outside it is ever read.
    dir: &'a Path,
    inlined: BTreeSet<PathBuf>,
    external: BTreeSet<String>,
    notes: Vec<String>,
}

const MAX_FOLD_FILE: u64 = 64 * 1024 * 1024;

impl Folder<'_> {
    /// The bytes of a local reference made from a file in `from_dir`, or
    /// `None` when it is not a plain local file inside the bundle folder (a
    /// url, an anchor, a data url, a symlink out). The first reading is
    /// recorded as inlined.
    fn read(&mut self, from_dir: &Path, reference: &str) -> Option<(PathBuf, Vec<u8>)> {
        let r = reference.trim();
        if r.is_empty() || r.starts_with('#') {
            return None;
        }
        let lower = r.to_ascii_lowercase();
        if lower.starts_with("data:") || lower.starts_with("about:") || lower.starts_with("blob:") {
            return None;
        }
        if lower.starts_with("http:")
            || lower.starts_with("https:")
            || r.starts_with("//")
            || lower.starts_with("javascript:")
            || lower.starts_with("mailto:")
        {
            self.external.insert(r.to_string());
            return None;
        }
        let path_part = r.split(['?', '#']).next().unwrap_or(r);
        let decoded = percent_decode(path_part);
        let canon = dunce::canonicalize(from_dir.join(decoded)).ok()?;
        if !canon.starts_with(self.dir) || !canon.is_file() {
            return None;
        }
        if std::fs::metadata(&canon).ok()?.len() > MAX_FOLD_FILE {
            self.notes
                .push(format!("{reference} is too large to fold and was left out"));
            return None;
        }
        let bytes = std::fs::read(&canon).ok()?;
        self.inlined.insert(canon.clone());
        Some((canon, bytes))
    }

    fn as_data_url(&mut self, from_dir: &Path, reference: &str) -> Option<String> {
        let (canon, bytes) = self.read(from_dir, reference)?;
        let ext = canon.extension().and_then(|e| e.to_str()).unwrap_or("");
        Some(data_url(mime_for(ext), &bytes))
    }

    /// `url(...)` references in css, rewritten to data URLs relative to the
    /// css file's own folder.
    fn css(&mut self, css: &str, css_dir: &Path) -> String {
        let url = Regex::new(r#"(?i)url\(\s*(?:"([^"]*)"|'([^']*)'|([^)\s]*))\s*\)"#).unwrap();
        let out = url.replace_all(css, |c: &Captures| {
            let r = c
                .get(1)
                .or_else(|| c.get(2))
                .or_else(|| c.get(3))
                .map_or("", |m| m.as_str());
            match self.as_data_url(css_dir, r) {
                Some(d) => format!("url(\"{d}\")"),
                None => c[0].to_string(),
            }
        });
        if Regex::new(r"(?i)@import").unwrap().is_match(&out) {
            self.notes
                .push("a stylesheet uses @import, which is not folded".to_string());
        }
        out.into_owned()
    }
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Some(v) = s
                .get(i + 1..i + 3)
                .and_then(|h| u8::from_str_radix(h, 16).ok())
            {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Content that sits inside `<script>` or `<style>` must not close it.
fn escape_raw(s: &str, tag: &str) -> String {
    let re = Regex::new(&format!(r"(?i)</({tag})")).unwrap();
    re.replace_all(s, "<\\/$1").into_owned()
}

fn attr_value(tag: &str, name: &str) -> Option<String> {
    let re = Regex::new(&format!(
        r#"(?is)(?:^|\s){name}\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s>]+))"#
    ))
    .unwrap();
    let c = re.captures(tag)?;
    Some(
        c.get(1)
            .or_else(|| c.get(2))
            .or_else(|| c.get(3))
            .map_or(String::new(), |m| m.as_str().to_string()),
    )
}

fn without_attr(attrs: &str, name: &str) -> String {
    Regex::new(&format!(
        r#"(?is)(?:^|\s){name}(?:\s*=\s*(?:"[^"]*"|'[^']*'|[^\s>]+))?"#
    ))
    .unwrap()
    .replace_all(attrs, "")
    .into_owned()
}

fn rel_name(dir: &Path, p: &Path) -> String {
    p.strip_prefix(dir)
        .unwrap_or(p)
        .to_string_lossy()
        .replace('\\', "/")
}

fn list_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = rd.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let Ok(t) = e.file_type() else { continue };
        let p = e.path();
        if t.is_dir() {
            list_files(&p, out);
        } else if t.is_file() {
            out.push(p);
        }
    }
}

/// Folds the author bundle at `dir` (already resolved inside the project)
/// into one document. `entry` is the bundle's `index.html`.
pub fn fold_bundle(dir: &Path, entry: &Path, policy: &str) -> Result<Folded, String> {
    let dir = dunce::canonicalize(dir).map_err(|e| format!("bundle folder: {e}"))?;
    let entry = dunce::canonicalize(entry).map_err(|e| format!("bundle entry: {e}"))?;
    if !entry.starts_with(&dir) {
        return Err("bundle entry resolves outside its folder".to_string());
    }
    let html = std::fs::read_to_string(&entry)
        .map_err(|e| format!("cannot read the bundle's index.html: {e}"))?;
    let entry_dir = entry.parent().unwrap_or(&dir).to_path_buf();
    let mut f = Folder {
        dir: &dir,
        inlined: BTreeSet::new(),
        external: BTreeSet::new(),
        notes: Vec::new(),
    };
    f.inlined.insert(entry.clone());

    // <script src=...></script>
    let script = Regex::new(r"(?is)<script\b([^>]*)>\s*</script\s*>").map_err(|e| e.to_string())?;
    let html = script
        .replace_all(&html, |c: &Captures| {
            let attrs = c[1].to_string();
            let Some(src) = attr_value(&attrs, "src") else {
                return c[0].to_string();
            };
            let Some((_, bytes)) = f.read(&entry_dir, &src) else {
                return c[0].to_string();
            };
            let body = String::from_utf8_lossy(&bytes).into_owned();
            let is_module = attr_value(&attrs, "type").is_some_and(|t| t.eq_ignore_ascii_case("module"));
            if is_module
                && Regex::new(r#"(?m)\bimport\s*(?:[\w{*]|["'(])"#)
                    .unwrap()
                    .is_match(&body)
            {
                f.notes.push(format!(
                    "module script {src} imports other files: bundle it to one classic script, or it will not run"
                ));
            }
            let kept = without_attr(&without_attr(&without_attr(&attrs, "src"), "integrity"), "crossorigin");
            format!("<script{kept}>{}</script>", escape_raw(&body, "script"))
        })
        .into_owned();

    // <link rel=stylesheet href=...> and other local link targets.
    let link = Regex::new(r"(?is)<link\b([^>]*)>").map_err(|e| e.to_string())?;
    let html = link
        .replace_all(&html, |c: &Captures| {
            let attrs = format!(" {}", c[1].trim().trim_end_matches('/'));
            let Some(href) = attr_value(&attrs, "href") else {
                return c[0].to_string();
            };
            let rel = attr_value(&attrs, "rel")
                .unwrap_or_default()
                .to_ascii_lowercase();
            if rel.split_whitespace().any(|r| r == "stylesheet") {
                let Some((canon, bytes)) = f.read(&entry_dir, &href) else {
                    return c[0].to_string();
                };
                let css_dir = canon.parent().unwrap_or(&entry_dir).to_path_buf();
                let css = f.css(&String::from_utf8_lossy(&bytes), &css_dir);
                let media = attr_value(&attrs, "media")
                    .map(|m| format!(" media=\"{}\"", attr(&m)))
                    .unwrap_or_default();
                return format!("<style{media}>{}</style>", escape_raw(&css, "style"));
            }
            match f.as_data_url(&entry_dir, &href) {
                Some(d) => format!(
                    "<link{}>",
                    without_attr(&attrs, "href") + &format!(" href=\"{d}\"")
                ),
                None => c[0].to_string(),
            }
        })
        .into_owned();

    // <style> blocks: url() references.
    let style = Regex::new(r"(?is)<style\b([^>]*)>(.*?)</style\s*>").map_err(|e| e.to_string())?;
    let html = style
        .replace_all(&html, |c: &Captures| {
            let css = f.css(&c[2], &entry_dir);
            format!("<style{}>{}</style>", &c[1], escape_raw(&css, "style"))
        })
        .into_owned();

    // Media elements: src and poster.
    let media = Regex::new(r"(?is)<(img|source|video|audio|track|image|embed|input)\b([^>]*)>")
        .map_err(|e| e.to_string())?;
    let html = media
        .replace_all(&html, |c: &Captures| {
            let mut attrs = format!(" {}", c[2].trim().trim_end_matches('/'));
            for name in ["src", "poster"] {
                if let Some(v) = attr_value(&attrs, name) {
                    if let Some(d) = f.as_data_url(&entry_dir, &v) {
                        attrs = without_attr(&attrs, name) + &format!(" {name}=\"{d}\"");
                    }
                }
            }
            format!("<{}{}>", &c[1], attrs)
        })
        .into_owned();

    let mut all = Vec::new();
    list_files(&dir, &mut all);
    let unfolded: Vec<String> = all
        .iter()
        .filter(|p| !f.inlined.contains(*p))
        .map(|p| rel_name(&dir, p))
        .filter(|n| n != "widget.json")
        .collect();

    Ok(Folded {
        html: with_policy(&html, policy),
        unfolded,
        external: f.external.into_iter().collect(),
        notes: f.notes,
    })
}

#[cfg(test)]
mod tests;
