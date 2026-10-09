//! Reader flags: the author's document-level settings for the reflowed page,
//! carried from the conversion as marker classes and resolved here.
//!
//! `maleficium-interactive.sty` checks `\maleficiumcontents` and
//! `\maleficiummeasure` against closed sets for the PDF, and the converter
//! binding (`engine/embedded/maleficium-interactive.sty.rhai`) carries each
//! call as one empty marker element:
//!
//! ```html
//! <span class="ltx_text m-flag m-flag-contents-off"></span>
//! ```
//!
//! [`scan`] finds those markers in the converted HTML where the exporter
//! already joins widgets, resolves them in document order (the last valid
//! marker of each kind wins) and strips every marker element from the
//! article, so none reaches the page. Anything outside the closed sets is a
//! hostile or stale marker: it is ignored with a warning, never coerced and
//! never passed to CSS. [`omit_contents`] then drops the generated contents
//! nav when the flags switch it off (not-emit, rather than hide).
//!
//! The scanner is purpose-built like [`crate::reflow::join`]'s: it tokenizes
//! tags properly (quoted attribute values may hold `<` and `>`), skips
//! comments and the raw text of `script`, `style`, `textarea` and `title`, so
//! a listing that shows a marker as text never votes.

/// The class token every flag marker carries.
const FLAG_CLASS: &str = "m-flag";
/// The value token of a contents marker: `m-flag-contents-on|off`.
const CONTENTS_PREFIX: &str = "m-flag-contents-";
/// The value token of a measure marker: `m-flag-measure-narrow|default|wide|full`.
const MEASURE_PREFIX: &str = "m-flag-measure-";

/// The article column width (`paper.reader.measure`): closed tokens only,
/// so no author string ever reaches CSS. `Default` is the theme's
/// `--m-measure` (`68ch`) and needs no override.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Measure {
    Narrow,
    Default,
    Wide,
    /// The whole window, less the page gutters.
    Full,
}

impl Measure {
    /// The manifest token.
    pub fn token(self) -> &'static str {
        match self {
            Measure::Narrow => "narrow",
            Measure::Default => "default",
            Measure::Wide => "wide",
            Measure::Full => "full",
        }
    }

    fn parse(s: &str) -> Option<Measure> {
        match s {
            "narrow" => Some(Measure::Narrow),
            "default" => Some(Measure::Default),
            "wide" => Some(Measure::Wide),
            "full" => Some(Measure::Full),
            _ => None,
        }
    }

    /// The `--m-measure` override the reader page writes for the token, or
    /// `None` when the theme's own value (`68ch`) is the default.
    pub fn width(self) -> Option<&'static str> {
        match self {
            Measure::Narrow => Some("56ch"),
            Measure::Default => None,
            Measure::Wide => Some("80ch"),
            Measure::Full => Some("100%"),
        }
    }
}

/// The resolved flags: what the manifest's `paper.reader` records.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Flags {
    /// Whether the article shows its contents list.
    pub contents: bool,
    /// The article column width.
    pub measure: Measure,
}

impl Default for Flags {
    fn default() -> Self {
        Flags {
            contents: true,
            measure: Measure::Default,
        }
    }
}

/// The converted HTML with every flag marker stripped, the resolved flags,
/// and one warning per problem found (never a reason to refuse the export).
pub struct Scan {
    pub html: String,
    pub flags: Flags,
    pub warnings: Vec<String>,
}

enum Vote {
    Contents(bool),
    Measure(Measure),
    Unknown(String),
}

/// Resolve the flag markers of `html` in document order and strip them: the
/// last valid marker of each kind wins. Only an empty marker votes; a marker
/// holding text keeps its text but sets nothing. Unknown value tokens,
/// markers naming no flag and unclosed markers are ignored with a warning.
/// A marker inside a comment or raw-text element is inert text and is left
/// alone.
pub fn scan(html: &str) -> Scan {
    let mut out = String::with_capacity(html.len());
    let mut at = 0;
    let mut flags = Flags::default();
    let mut warnings: Vec<String> = Vec::new();
    let mut bare_warned = false;
    let mut i = 0;
    while let Some(off) = html[i..].find('<') {
        let lt = i + off;
        let rest = &html[lt..];
        if rest.starts_with("<!--") {
            i = after(html, lt + 4, "-->");
        } else if rest.starts_with("<!") || rest.starts_with("<?") || rest.starts_with("</") {
            i = after(html, lt + 2, ">");
        } else if html
            .as_bytes()
            .get(lt + 1)
            .is_some_and(u8::is_ascii_alphabetic)
        {
            let tag = read_tag(html, lt);
            i = tag.end;
            if matches!(tag.name.as_str(), "script" | "style" | "textarea" | "title") {
                i = raw_text_end(html, tag.end, &tag.name);
            } else if is_marker(tag.class.as_deref()) {
                let votes = votes_of(tag.class.as_deref().unwrap_or_default());
                if votes.is_empty() && !bare_warned {
                    bare_warned = true;
                    warn(
                        &mut warnings,
                        "a reader flag marker names no flag; it was ignored".to_string(),
                    );
                }
                match element_end(html, &tag) {
                    Some((end, inner)) if inner.trim().is_empty() => {
                        apply(&mut flags, &mut warnings, &votes);
                        out.push_str(&html[at..lt]);
                        at = end;
                        i = end;
                    }
                    Some((end, inner)) => {
                        // Control channel, never content: the votes are void
                        // and only the text stays.
                        warn(
                            &mut warnings,
                            "a reader flag marker holds text; only empty markers set flags, so it was ignored"
                                .to_string(),
                        );
                        for v in &votes {
                            if let Vote::Unknown(t) = v {
                                warn(
                                    &mut warnings,
                                    format!(
                                        "unknown reader flag `{t}`; it was ignored and the default used"
                                    ),
                                );
                            }
                        }
                        out.push_str(&html[at..lt]);
                        out.push_str(inner);
                        at = end;
                        i = end;
                    }
                    None => {
                        warn(
                            &mut warnings,
                            "an unclosed reader flag marker was left in place; it sets no flag"
                                .to_string(),
                        );
                    }
                }
            }
        } else {
            i = lt + 1;
        }
    }
    out.push_str(&html[at..]);
    Scan {
        html: out,
        flags,
        warnings,
    }
}

/// The value tokens of one marker's `class` value, in class order.
fn votes_of(class: &str) -> Vec<Vote> {
    let mut votes = Vec::new();
    for t in class.split_ascii_whitespace() {
        if t == FLAG_CLASS {
            continue;
        }
        if let Some(v) = t.strip_prefix(CONTENTS_PREFIX) {
            votes.push(match v {
                "on" => Vote::Contents(true),
                "off" => Vote::Contents(false),
                _ => Vote::Unknown(t.to_string()),
            });
        } else if let Some(v) = t.strip_prefix(MEASURE_PREFIX) {
            votes.push(match Measure::parse(v) {
                Some(m) => Vote::Measure(m),
                None => Vote::Unknown(t.to_string()),
            });
        } else if t.starts_with("m-flag-") {
            votes.push(Vote::Unknown(t.to_string()));
        }
    }
    votes
}

/// Apply an empty marker's votes in order; unknown tokens warn and fall back
/// to the default (never coerce).
fn apply(flags: &mut Flags, warnings: &mut Vec<String>, votes: &[Vote]) {
    for v in votes {
        match v {
            Vote::Contents(on) => flags.contents = *on,
            Vote::Measure(m) => flags.measure = *m,
            Vote::Unknown(t) => warn(
                warnings,
                format!("unknown reader flag `{t}`; it was ignored and the default used"),
            ),
        }
    }
}

fn warn(warnings: &mut Vec<String>, message: String) {
    if !warnings.contains(&message) {
        warnings.push(message);
    }
}

/// Whether a `class` value marks a flag element.
fn is_marker(class: Option<&str>) -> bool {
    class
        .unwrap_or_default()
        .split_ascii_whitespace()
        .any(|t| t == FLAG_CLASS)
}

/// The article without its generated contents nav: `contents: false` omits
/// the nav (not-emit), rather than hiding it. The nav the pipeline builds
/// holds only a heading and nested lists, never another nav, so the first
/// close tag ends it. No nav, or no close tag, leaves the article as is.
pub fn omit_contents(article: &str) -> String {
    const OPEN: &str = "<nav class=\"m-contents\"";
    const CLOSE: &str = "</nav>";
    let Some(start) = article.find(OPEN) else {
        return article.to_string();
    };
    let Some(off) = article[start..].find(CLOSE) else {
        return article.to_string();
    };
    let end = start + off + CLOSE.len();
    format!("{}{}", &article[..start], &article[end..])
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
    loop {
        while i < b.len() && (b[i].is_ascii_whitespace() || b[i] == b'/') {
            i += 1;
        }
        if i >= b.len() {
            break;
        }
        if b[i] == b'>' {
            i += 1;
            break;
        }
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
    }
}

/// HTML elements with no end tag: they hold nothing and never nest.
const VOID: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "source", "track",
    "wbr",
];

/// The end of the marker element whose start tag is `tag`, with the text
/// between the tags: `None` when no close tag follows. Same-name elements
/// nest by depth; comments and raw-text innards never close it. A void
/// element (`br`, `img`, ...) holds nothing: it ends at its start tag.
fn element_end<'a>(html: &'a str, tag: &Tag) -> Option<(usize, &'a str)> {
    if VOID.contains(&tag.name.as_str()) {
        return Some((tag.end, ""));
    }
    let inner_from = tag.end;
    let mut depth = 1;
    let mut i = tag.end;
    while let Some(off) = html[i..].find('<') {
        let at = i + off;
        let rest = &html[at..];
        if rest.starts_with("<!--") {
            i = after(html, at + 4, "-->");
        } else if rest.starts_with("<!") || rest.starts_with("<?") {
            i = after(html, at + 2, ">");
        } else if rest.starts_with("</") {
            let mut j = at + 2;
            while j < html.len()
                && !html.as_bytes()[j].is_ascii_whitespace()
                && !matches!(html.as_bytes()[j], b'>' | b'/')
            {
                j += 1;
            }
            if html[at + 2..j].eq_ignore_ascii_case(&tag.name) {
                depth -= 1;
                if depth == 0 {
                    return Some((after(html, at + 2, ">"), &html[inner_from..at]));
                }
            }
            i = after(html, at + 2, ">");
        } else if html
            .as_bytes()
            .get(at + 1)
            .is_some_and(u8::is_ascii_alphabetic)
        {
            let inner = read_tag(html, at);
            i = inner.end;
            if matches!(
                inner.name.as_str(),
                "script" | "style" | "textarea" | "title"
            ) {
                i = raw_text_end(html, inner.end, &inner.name);
            } else if inner.name == tag.name && !VOID.contains(&inner.name.as_str()) {
                depth += 1;
            }
        } else {
            i = at + 1;
        }
    }
    None
}

#[cfg(test)]
mod tests;
