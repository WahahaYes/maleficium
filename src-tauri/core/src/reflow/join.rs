//! Joining the article's widget placeholders to the manifest's widget records by document order.
//!
//! The converter binding (`engine/embedded/maleficium-interactive.sty.rhai`)
//! turns every `\interactive*` macro into one empty placeholder element:
//!
//! ```html
//! <span class="ltx_text m-widget m-widget-model"></span>
//! ```
//!
//! The class token `m-widget-KIND` names the widget type (`model`, `video`,
//! `table`, `chart`, `html`). The element carries nothing else: the PDF
//! compile resolved ids, labels, posters and figure numbers into the
//! `<jobname>.mfw` sidecar, and the i-th placeholder in document order is the
//! i-th `widget|` record. [`join`] checks that the counts agree and that each
//! pair has the same type, and refuses otherwise. [`apply`] then replaces each
//! placeholder with a [`mount_marker`], an HTML comment naming the record id,
//! which the reader turns into its mount unit.
//!
//! The scanner is purpose-built for latexml's HTML5 output and our own
//! placeholder markup, not a general HTML parser. It tokenizes tags properly
//! (quoted attribute values may hold `<` and `>`), skips comments, doctype and
//! processing instructions, and skips the raw text of `script`, `style`,
//! `textarea` and `title`. Text and code listings never match because the
//! serializer escapes `<` as `&lt;` there. Limits: a `<![CDATA[ ... ]]>`
//! section containing `>` ends early (latexml's HTML5 output has none); a
//! placeholder must be a `span` whose start tag is immediately followed by
//! `</span>` (whitespace allowed), anything else is [`JoinError::Malformed`];
//! only `span` elements are placeholders.

use crate::widgets::{parse_sidecar, Record, WidgetType};

/// The class token every placeholder carries.
const WIDGET_CLASS: &str = "m-widget";
/// Prefix of the kind token: `m-widget-model`.
const KIND_PREFIX: &str = "m-widget-";
/// Opening of the marker [`apply`] leaves where a placeholder was.
pub const MOUNT_PREFIX: &str = "<!--mount:";

/// One placeholder found in the article, in document order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placeholder {
    /// Position among the placeholders, from 0.
    pub index: usize,
    /// The widget type its `m-widget-KIND` token names.
    pub kind: WidgetType,
    /// Byte range of the whole element (start tag through `</span>`) in the
    /// html it was found in.
    pub start: usize,
    pub end: usize,
}

/// A placeholder paired with the sidecar record it stands for.
#[derive(Debug, Clone, PartialEq)]
pub struct Joined {
    pub placeholder: Placeholder,
    pub record: Record,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JoinError {
    /// The article and the sidecar disagree on how many widgets there are:
    /// recompile the document or fix the conversion.
    CountMismatch { placeholders: usize, records: usize },
    /// The i-th placeholder and the i-th record are different widget types.
    KindMismatch {
        index: usize,
        id: String,
        placeholder: WidgetType,
        record: WidgetType,
    },
    /// A placeholder this scanner cannot read (byte offset in the html).
    Malformed { offset: usize, reason: String },
    /// The sidecar did not parse.
    Sidecar(String),
    /// The joined placeholders do not belong to the html given to [`apply`].
    Stale,
}

impl std::fmt::Display for JoinError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CountMismatch {
                placeholders,
                records,
            } => write!(
                f,
                "the article has {placeholders} widget placeholders but the widget sidecar records {records}; recompile the document"
            ),
            Self::KindMismatch {
                index,
                id,
                placeholder,
                record,
            } => write!(
                f,
                "widget {} (`{id}`) is a {record:?} in the sidecar but a {placeholder:?} in the article; recompile the document",
                index + 1
            ),
            Self::Malformed { offset, reason } => {
                write!(f, "unreadable widget placeholder at byte {offset}: {reason}")
            }
            Self::Sidecar(e) => f.write_str(e),
            Self::Stale => f.write_str("the joined placeholders do not match this html"),
        }
    }
}

impl std::error::Error for JoinError {}

/// The marker [`apply`] writes for a record: an HTML comment, so it nests
/// anywhere a span did and survives a sanitizer only if the reader keeps
/// comments (it should consume markers before sanitizing).
pub fn mount_marker(id: &str) -> String {
    format!("{MOUNT_PREFIX}{id}-->")
}

/// Every widget placeholder of `html`, in document order.
pub fn extract_placeholders(html: &str) -> Result<Vec<Placeholder>, JoinError> {
    let b = html.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(off) = html[i..].find('<') {
        let lt = i + off;
        let rest = &html[lt..];
        if rest.starts_with("<!--") {
            i = after(html, lt + 4, "-->");
        } else if rest.starts_with("<!") || rest.starts_with("<?") || rest.starts_with("</") {
            i = after(html, lt + 2, ">");
        } else if b.get(lt + 1).is_some_and(u8::is_ascii_alphabetic) {
            let tag = read_tag(html, lt);
            i = tag.end;
            if matches!(tag.name.as_str(), "script" | "style" | "textarea" | "title") {
                i = raw_text_end(html, tag.end, &tag.name);
            } else if tag.name == "span" {
                if let Some(class) = tag.class.as_deref() {
                    if let Some(kind) = placeholder_kind(class, lt)? {
                        let end = placeholder_end(html, &tag, lt)?;
                        out.push(Placeholder {
                            index: out.len(),
                            kind,
                            start: lt,
                            end,
                        });
                        i = end;
                    }
                }
            }
        } else {
            i = lt + 1;
        }
    }
    Ok(out)
}

/// Pair the placeholders of `html` with `records` by order. Refuses a count
/// mismatch (carrying both counts) and a type mismatch at any position.
pub fn join(html: &str, records: Vec<Record>) -> Result<Vec<Joined>, JoinError> {
    let found = extract_placeholders(html)?;
    if found.len() != records.len() {
        return Err(JoinError::CountMismatch {
            placeholders: found.len(),
            records: records.len(),
        });
    }
    let mut out = Vec::with_capacity(found.len());
    for (placeholder, record) in found.into_iter().zip(records) {
        if placeholder.kind != record.kind {
            return Err(JoinError::KindMismatch {
                index: placeholder.index,
                id: record.id,
                placeholder: placeholder.kind,
                record: record.kind,
            });
        }
        out.push(Joined {
            placeholder,
            record,
        });
    }
    Ok(out)
}

/// [`join`] against the text of a `<jobname>.mfw` sidecar, parsed by the
/// widget module's own parser.
pub fn join_sidecar(html: &str, sidecar: &str) -> Result<Vec<Joined>, JoinError> {
    join(html, parse_sidecar(sidecar).map_err(JoinError::Sidecar)?)
}

/// `html` with each joined placeholder replaced by its [`mount_marker`].
/// `joined` must come from [`join`] on this same `html`.
pub fn apply(html: &str, joined: &[Joined]) -> Result<String, JoinError> {
    let mut out = String::with_capacity(html.len());
    let mut at = 0;
    for j in joined {
        let p = &j.placeholder;
        if p.start < at
            || p.end > html.len()
            || !html.is_char_boundary(p.start)
            || !html.is_char_boundary(p.end)
        {
            return Err(JoinError::Stale);
        }
        out.push_str(&html[at..p.start]);
        out.push_str(&mount_marker(&j.record.id));
        at = p.end;
    }
    out.push_str(&html[at..]);
    Ok(out)
}

/// Index just past the first `pat` at or after `from`, or the end.
fn after(html: &str, from: usize, pat: &str) -> usize {
    html[from..]
        .find(pat)
        .map_or(html.len(), |o| from + o + pat.len())
}

/// Index just past the end tag of a raw-text element opened at `from`.
fn raw_text_end(html: &str, from: usize, name: &str) -> usize {
    let close = format!("</{name}");
    let lower = html[from..].to_ascii_lowercase();
    match lower.find(&close) {
        Some(o) => after(html, from + o, ">"),
        None => html.len(),
    }
}

struct Tag {
    name: String,
    class: Option<String>,
    /// Index just past the closing `>`.
    end: usize,
    self_closed: bool,
}

/// Read the start tag at `lt` (`html[lt]` is `<` and a letter follows).
fn read_tag(html: &str, lt: usize) -> Tag {
    let b = html.as_bytes();
    let mut i = lt + 1;
    while i < b.len() && !b[i].is_ascii_whitespace() && b[i] != b'>' && b[i] != b'/' {
        i += 1;
    }
    let name = html[lt + 1..i].to_ascii_lowercase();
    let mut class = None;
    let mut self_closed = false;
    loop {
        while i < b.len() && (b[i].is_ascii_whitespace() || b[i] == b'/') {
            self_closed = b[i] == b'/';
            i += 1;
        }
        if i >= b.len() {
            break;
        }
        if b[i] == b'>' {
            i += 1;
            break;
        }
        self_closed = false;
        let ns = i;
        while i < b.len() && !b[i].is_ascii_whitespace() && !matches!(b[i], b'=' | b'>' | b'/') {
            i += 1;
        }
        let attr = html[ns..i].to_ascii_lowercase();
        while i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        }
        let mut value = "";
        if i < b.len() && b[i] == b'=' {
            i += 1;
            while i < b.len() && b[i].is_ascii_whitespace() {
                i += 1;
            }
            if i < b.len() && (b[i] == b'"' || b[i] == b'\'') {
                let q = b[i];
                let vs = i + 1;
                i = vs;
                while i < b.len() && b[i] != q {
                    i += 1;
                }
                value = &html[vs..i.min(b.len())];
                i = (i + 1).min(b.len());
            } else {
                let vs = i;
                while i < b.len() && !b[i].is_ascii_whitespace() && b[i] != b'>' {
                    i += 1;
                }
                value = &html[vs..i];
            }
        }
        if attr == "class" && class.is_none() {
            class = Some(value.to_string());
        }
    }
    Tag {
        name,
        class,
        end: i,
        self_closed,
    }
}

/// The widget type of a `class` value, `None` when it is not a placeholder.
fn placeholder_kind(class: &str, at: usize) -> Result<Option<WidgetType>, JoinError> {
    let tokens = || class.split_ascii_whitespace();
    if !tokens().any(|t| t == WIDGET_CLASS) {
        return Ok(None);
    }
    let bad = |reason: &str| JoinError::Malformed {
        offset: at,
        reason: reason.to_string(),
    };
    let kinds: Vec<&str> = tokens()
        .filter_map(|t| t.strip_prefix(KIND_PREFIX))
        .collect();
    match kinds.as_slice() {
        [k] => WidgetType::parse(k)
            .map(Some)
            .ok_or_else(|| bad(&format!("unknown widget kind `{k}`"))),
        [] => Err(bad("no m-widget-KIND class token")),
        _ => Err(bad("more than one m-widget-KIND class token")),
    }
}

/// End of the placeholder whose start tag is `tag`: the `</span>` that
/// follows it with nothing but white space between.
fn placeholder_end(html: &str, tag: &Tag, at: usize) -> Result<usize, JoinError> {
    let bad = |reason: &str| JoinError::Malformed {
        offset: at,
        reason: reason.to_string(),
    };
    if tag.self_closed {
        return Err(bad(
            "self-closed placeholder (HTML does not close a span with />)",
        ));
    }
    let rest = &html[tag.end..];
    let trimmed = rest.trim_start();
    let skipped = rest.len() - trimmed.len();
    if trimmed.len() >= 7 && trimmed[..7].eq_ignore_ascii_case("</span>") {
        Ok(tag.end + skipped + 7)
    } else {
        Err(bad("placeholder is not empty"))
    }
}

#[cfg(test)]
mod tests;
