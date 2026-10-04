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
use std::collections::{BTreeMap, BTreeSet};

/// The widget document's policy before any declared origins: inline only.
/// Sources reach a runtime as bytes over `postMessage`, so nothing here
/// needs a host.
const WIDGET_POLICY: &str = "default-src 'none'; script-src 'unsafe-inline'{res}; style-src 'unsafe-inline'{res}; img-src data: blob:{res}; media-src data: blob:{res}; font-src data:{res}; connect-src {connect}; {frame}form-action 'none'; base-uri 'none'";

/// The reader page of a folder or hosted bundle. Its `frame-src 'self'` is
/// what stops a widget navigating its own frame off-site.
pub const FOLDER_READER_POLICY: &str = "default-src 'none'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; connect-src 'self'; frame-src 'self'; object-src 'none'; form-action 'none'; base-uri 'none'";

/// The reader page of a single-file bundle. No `frame-src`: it falls back
/// to `'none'`, which admits srcdoc frames and blocks `data:` ones and any
/// navigation.
pub const SINGLE_FILE_READER_POLICY: &str = "default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src data: blob:; media-src data: blob:; font-src data:; connect-src 'none'; object-src 'none'; form-action 'none'; base-uri 'none'";

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

/// The policy a page's first meta CSP carries (tests read the page's own
/// policy, not its script text).
#[cfg(test)]
pub(crate) fn policy_of(html: &str) -> &str {
    let tag = "<meta http-equiv=\"Content-Security-Policy\" content=\"";
    html.split_once(tag)
        .and_then(|(_, rest)| rest.split_once('"'))
        .map_or("", |(p, _)| p)
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
    /// The bundle's files by `/`-separated path: nothing else is ever read.
    files: &'a BTreeMap<String, Vec<u8>>,
    inlined: BTreeSet<String>,
    external: BTreeSet<String>,
    notes: Vec<String>,
}

/// The folder part of a `/`-separated path (`""` for the bundle's top).
fn parent(p: &str) -> &str {
    p.rsplit_once('/').map_or("", |(d, _)| d)
}

/// A relative reference made from `from_dir`, as a bundle path: `None` when
/// it is absolute or climbs out of the bundle.
fn join(from_dir: &str, reference: &str) -> Option<String> {
    if reference.starts_with('/') || reference.contains('\\') || reference.contains(':') {
        return None;
    }
    let mut parts: Vec<&str> = from_dir.split('/').filter(|s| !s.is_empty()).collect();
    for seg in reference.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            s => parts.push(s),
        }
    }
    Some(parts.join("/"))
}

impl<'a> Folder<'a> {
    /// The bytes of a local reference made from a file in `from_dir`, or
    /// `None` when it is not one of the bundle's files (a url, an anchor, a
    /// data url, a path out). The first reading is recorded as inlined.
    fn read(&mut self, from_dir: &str, reference: &str) -> Option<(String, &'a [u8])> {
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
        let path = join(from_dir, &percent_decode(path_part))?;
        let bytes = self.files.get(&path)?;
        self.inlined.insert(path.clone());
        Some((path, bytes.as_slice()))
    }

    fn as_data_url(&mut self, from_dir: &str, reference: &str) -> Option<String> {
        let (path, bytes) = self.read(from_dir, reference)?;
        let ext = path.rsplit_once('.').map_or("", |(_, e)| e);
        Some(data_url(mime_for(ext), bytes))
    }

    /// `url(...)` references in css, rewritten to data URLs relative to the
    /// css file's own folder.
    fn css(&mut self, css: &str, css_dir: &str) -> String {
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

/// The bundle's entry page.
pub const ENTRY: &str = "index.html";

/// Folds an author bundle into one document. `files` is the bundle read
/// once (a [`crate::widget_approval::WidgetSnapshot`]'s files): the fold
/// reads nothing else, so the document holds exactly the bytes that were
/// hashed.
pub fn fold_bundle(files: &BTreeMap<String, Vec<u8>>, policy: &str) -> Result<Folded, String> {
    let html = files
        .get(ENTRY)
        .ok_or_else(|| format!("the bundle has no {ENTRY}"))?;
    let html = std::str::from_utf8(html)
        .map_err(|_| format!("the bundle's {ENTRY} is not UTF-8"))?
        .to_string();
    let entry_dir = parent(ENTRY).to_string();
    let mut f = Folder {
        files,
        inlined: BTreeSet::new(),
        external: BTreeSet::new(),
        notes: Vec::new(),
    };
    f.inlined.insert(ENTRY.to_string());

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
            let body = String::from_utf8_lossy(bytes).into_owned();
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
                let Some((path, bytes)) = f.read(&entry_dir, &href) else {
                    return c[0].to_string();
                };
                let css = f.css(&String::from_utf8_lossy(bytes), parent(&path));
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

    let unfolded: Vec<String> = files
        .keys()
        .filter(|p| !f.inlined.contains(*p) && p.as_str() != crate::widgets::BUNDLE_MANIFEST)
        .cloned()
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
