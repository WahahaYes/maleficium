//! The reader's trust boundary: the converted article, allow-list sanitized.
//!
//! The HTML comes from the author's own source through latexml, but the page
//! runs the widgets' host script, so nothing in the article may run, load or
//! navigate. [`sanitize`] parses the input with html5ever (a spec-compliant
//! HTML parser, through `dom_query`) and writes a new document from that
//! tree, emitting only what the tables below allow. Nothing of the input is
//! copied through as text: every element name and attribute name written is
//! one of ours, every text node and attribute value is escaped, comments,
//! doctypes and processing instructions are never written, and no raw-text
//! element (`script`, `style`, ...) survives, so the output holds no context
//! where a re-parse could turn text into markup.
//!
//! Rules, by element (namespace-aware: MathML names count only inside
//! `<math>`, and anything in another foreign namespace such as SVG is
//! dropped):
//!
//! - [`HTML_KEEP`] and [`MATHML_KEEP`] elements are written with their
//!   allowed attributes;
//! - [`DROP`] elements are removed with everything inside them;
//! - any other element is unwrapped: its children are kept, the tag is not.
//!
//! Attributes: [`GLOBAL_ATTRS`] on any kept element, plus the per-element
//! entries of [`ELEMENT_ATTRS`] and [`MATHML_ATTRS`]. Classes pass through
//! (the reader styles latexml's `ltx_*` classes). An attribute with a
//! namespace (`xlink:href`, `xml:lang`) is never kept. Then the values that
//! can do something are checked:
//!
//! - `a href`: `#fragment`, `http:` or `https:` only;
//! - `img src`: a base64 `data:image/...` URI of a raster or SVG type, or a
//!   bundle-relative `figures/NAME` or `assets/NAME`;
//! - `id`: never one the page itself owns ([`reserved_id`]), so the article
//!   cannot shadow the reader's own elements;
//! - the mount unit: `data-widget`, `data-type` and the one `style` the page
//!   needs (`--ar:W / H`) are kept only on a `figure` whose `data-widget`
//!   names a widget of this export, at most once per widget; the in-app
//!   article's `data-approval="required"` marker passes on the same figures,
//!   with that exact value only.

use std::collections::BTreeSet;

/// HTML elements written as themselves.
pub const HTML_KEEP: &[&str] = &[
    "a",
    "abbr",
    "address",
    "article",
    "aside",
    "b",
    "bdi",
    "bdo",
    "blockquote",
    "br",
    "caption",
    "cite",
    "code",
    "col",
    "colgroup",
    "dd",
    "del",
    "details",
    "dfn",
    "div",
    "dl",
    "dt",
    "em",
    "figcaption",
    "figure",
    "footer",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "header",
    "hr",
    "i",
    "img",
    "ins",
    "kbd",
    "li",
    "mark",
    "nav",
    "ol",
    "p",
    "pre",
    "q",
    "s",
    "samp",
    "section",
    "small",
    "span",
    "strong",
    "sub",
    "summary",
    "sup",
    "table",
    "tbody",
    "td",
    "tfoot",
    "th",
    "thead",
    "tr",
    "u",
    "ul",
    "var",
    "wbr",
];

/// MathML elements written as themselves (inside `<math>` only).
/// `annotation-xml` (an HTML/SVG integration point) and `mglyph` (loads an
/// image) are not here; they are in [`DROP`].
pub const MATHML_KEEP: &[&str] = &[
    "math",
    "annotation",
    "semantics",
    "menclose",
    "merror",
    "mfenced",
    "mfrac",
    "mi",
    "mlabeledtr",
    "mmultiscripts",
    "mn",
    "mo",
    "mover",
    "mpadded",
    "mphantom",
    "mprescripts",
    "mroot",
    "mrow",
    "ms",
    "mspace",
    "msqrt",
    "mstyle",
    "msub",
    "msubsup",
    "msup",
    "mtable",
    "mtd",
    "mtext",
    "mtr",
    "munder",
    "munderover",
    "none",
];

/// Removed together with their content, in any namespace.
pub const DROP: &[&str] = &[
    "annotation-xml",
    "applet",
    "area",
    "audio",
    "base",
    "basefont",
    "bgsound",
    "button",
    "canvas",
    "dialog",
    "embed",
    "fieldset",
    "form",
    "frame",
    "frameset",
    "head",
    "iframe",
    "input",
    "isindex",
    "keygen",
    "link",
    "map",
    "meta",
    "mglyph",
    "noembed",
    "noframes",
    "noscript",
    "object",
    "optgroup",
    "option",
    "param",
    "picture",
    "plaintext",
    "portal",
    "script",
    "select",
    "slot",
    "source",
    "style",
    "svg",
    "template",
    "textarea",
    "title",
    "track",
    "video",
    "xmp",
];

/// Attributes any kept element may carry.
pub const GLOBAL_ATTRS: &[&str] = &[
    "aria-hidden",
    "aria-label",
    "class",
    "dir",
    "id",
    "lang",
    "role",
    "title",
];

/// Per-element HTML attributes, beyond [`GLOBAL_ATTRS`].
pub const ELEMENT_ATTRS: &[(&str, &[&str])] = &[
    ("a", &["href"]),
    ("col", &["span"]),
    ("colgroup", &["span"]),
    (
        "figure",
        &["data-widget", "data-type", "data-approval", "style"],
    ),
    ("img", &["alt", "height", "src", "width"]),
    ("li", &["value"]),
    ("ol", &["reversed", "start", "type"]),
    ("td", &["colspan", "headers", "rowspan"]),
    ("th", &["colspan", "headers", "rowspan", "scope"]),
];

/// Attributes any kept MathML element may carry, beyond [`GLOBAL_ATTRS`].
/// None of them takes a URL or script.
pub const MATHML_ATTRS: &[&str] = &[
    "accent",
    "accentunder",
    "align",
    "alttext",
    "close",
    "columnalign",
    "columnlines",
    "columnspacing",
    "columnspan",
    "depth",
    "display",
    "displaystyle",
    "encoding",
    "fence",
    "form",
    "frame",
    "height",
    "largeop",
    "linethickness",
    "lspace",
    "mathbackground",
    "mathcolor",
    "mathsize",
    "mathvariant",
    "maxsize",
    "minsize",
    "movablelimits",
    "notation",
    "open",
    "rowalign",
    "rowlines",
    "rowspacing",
    "rowspan",
    "rspace",
    "scriptlevel",
    "separator",
    "separators",
    "stretchy",
    "symmetric",
    "voffset",
    "width",
];

/// Widget kinds a mount unit's `data-type` may name.
pub const MOUNT_KINDS: &[&str] = &["model", "video", "table", "chart", "html", "custom"];

/// Element ids the reader page owns: its JSON islands and controls use the
/// `mfw-` prefix, and `pdf-link` is the download link.
pub fn reserved_id(id: &str) -> bool {
    id.to_ascii_lowercase().starts_with("mfw-") || id.eq_ignore_ascii_case("pdf-link")
}

const HTML_NS: &str = "http://www.w3.org/1999/xhtml";
const MATHML_NS: &str = "http://www.w3.org/1998/Math/MathML";
/// HTML elements with no end tag.
const VOID: &[&str] = &["br", "col", "hr", "img", "wbr"];

/// The article fragment `html`, reduced to what the tables allow. `mounts`
/// are the widget ids of this export: the only values a mount unit's
/// `data-widget` may carry.
pub fn sanitize(html: &str, mounts: &BTreeSet<String>) -> String {
    let doc = dom_query::Document::fragment(html);
    let mut out = String::with_capacity(html.len());
    let mut mounted: BTreeSet<String> = BTreeSet::new();
    // Iterative, so a deeply nested input cannot overflow the stack.
    enum Step<'a> {
        Node(dom_query::NodeRef<'a>),
        Close(String),
    }
    let mut stack: Vec<Step> = doc
        .root()
        .children()
        .into_iter()
        .rev()
        .map(Step::Node)
        .collect();
    while let Some(step) = stack.pop() {
        let node = match step {
            Step::Close(tag) => {
                out.push_str("</");
                out.push_str(&tag);
                out.push('>');
                continue;
            }
            Step::Node(n) => n,
        };
        if node.is_text() {
            escape_text(&node.text(), &mut out);
            continue;
        }
        if !node.is_element() {
            continue; // comments, doctypes, processing instructions
        }
        let Some((ns, name)) = node
            .qual_name_ref()
            .map(|q| (q.ns.to_string(), q.local.to_string().to_ascii_lowercase()))
        else {
            continue;
        };
        let children = || node.children().into_iter().rev().map(Step::Node);
        if DROP.contains(&name.as_str()) || (ns != HTML_NS && ns != MATHML_NS) {
            continue;
        }
        let keep = if ns == HTML_NS {
            HTML_KEEP.contains(&name.as_str())
        } else {
            MATHML_KEEP.contains(&name.as_str())
        };
        if !keep {
            stack.extend(children());
            continue;
        }
        out.push('<');
        out.push_str(&name);
        let attrs: Vec<(String, String)> = node
            .attrs()
            .into_iter()
            .filter(|a| a.name.ns.is_empty())
            .map(|a| {
                (
                    a.name.local.to_string().to_ascii_lowercase(),
                    a.value.to_string(),
                )
            })
            .collect();
        for (k, v) in kept_attrs(&ns, &name, &attrs, mounts, &mut mounted) {
            out.push(' ');
            out.push_str(k);
            out.push_str("=\"");
            escape_attr(&v, &mut out);
            out.push('"');
        }
        out.push('>');
        if ns == HTML_NS && VOID.contains(&name.as_str()) {
            continue;
        }
        if name == "pre"
            && node
                .first_child()
                .is_some_and(|c| c.is_text() && c.text().starts_with('\n'))
        {
            // The parser drops one leading newline after `<pre>`; keep the
            // author's by writing it back (the HTML serialization rule).
            out.push('\n');
        }
        stack.push(Step::Close(name));
        stack.extend(children());
    }
    out
}

/// The attributes of one kept element that pass, as (our name, value).
fn kept_attrs(
    ns: &str,
    name: &str,
    attrs: &[(String, String)],
    mounts: &BTreeSet<String>,
    mounted: &mut BTreeSet<String>,
) -> Vec<(&'static str, String)> {
    let get = |k: &str| attrs.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str());
    let allowed: Vec<&'static str> = if ns == MATHML_NS {
        GLOBAL_ATTRS.iter().chain(MATHML_ATTRS).copied().collect()
    } else {
        let own = ELEMENT_ATTRS
            .iter()
            .find(|(e, _)| *e == name)
            .map_or(&[][..], |(_, a)| *a);
        GLOBAL_ATTRS.iter().chain(own).copied().collect()
    };
    // A mount unit: a figure naming one of this export's widgets, once.
    let mount = name == "figure"
        && ns == HTML_NS
        && get("data-widget").is_some_and(|id| mounts.contains(id) && !mounted.contains(id))
        && get("data-type").is_some_and(|t| MOUNT_KINDS.contains(&t));
    if mount {
        if let Some(id) = get("data-widget") {
            mounted.insert(id.to_string());
        }
    }
    let mut out: Vec<(&'static str, String)> = Vec::new();
    for (raw, value) in attrs {
        // Our own spelling of the name, so nothing of the input's is written.
        let Some(key) = allowed.iter().copied().find(|k| k == raw) else {
            continue;
        };
        if out.iter().any(|(k, _)| *k == key) {
            continue;
        }
        let ok = match (name, key) {
            (_, "id") => !reserved_id(value),
            ("a", "href") => safe_href(value),
            ("img", "src") => safe_src(value),
            ("figure", "data-widget" | "data-type") => mount,
            // The in-app article marks html widgets held back for approval;
            // only the exact marker passes, never author text.
            ("figure", "data-approval") => mount && value == "required",
            ("figure", "style") => mount && aspect_style(value),
            _ => true,
        };
        if ok {
            out.push((key, value.to_string()));
        }
    }
    out
}

/// What a browser would see as the URL: tabs and newlines removed anywhere,
/// C0 controls and spaces trimmed at both ends.
fn url_of(raw: &str) -> String {
    raw.chars()
        .filter(|c| !matches!(c, '\t' | '\n' | '\r'))
        .collect::<String>()
        .trim_matches(|c: char| c <= ' ')
        .to_string()
}

/// `#fragment`, or an absolute `http:` / `https:` URL.
fn safe_href(raw: &str) -> bool {
    let url = url_of(raw);
    let lower = url.to_ascii_lowercase();
    url.starts_with('#') || lower.starts_with("http://") || lower.starts_with("https://")
}

/// A base64 image `data:` URI, or a bundle-relative figure or asset file.
fn safe_src(raw: &str) -> bool {
    let url = url_of(raw);
    let scheme = url.get(..5).map(str::to_ascii_lowercase);
    if let Some(rest) = url.get(5..).filter(|_| scheme.as_deref() == Some("data:")) {
        let Some((head, body)) = rest.split_once(',') else {
            return false;
        };
        let mime = head.to_ascii_lowercase();
        let types = [
            "image/png;base64",
            "image/jpeg;base64",
            "image/gif;base64",
            "image/webp;base64",
            "image/svg+xml;base64",
        ];
        return types.contains(&mime.as_str())
            && body
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'/' | b'='));
    }
    let Some(name) = url
        .strip_prefix("figures/")
        .or_else(|| url.strip_prefix("assets/"))
    else {
        return false;
    };
    !name.is_empty()
        && !name.starts_with('.')
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

/// Exactly `--ar:W / H` with plain decimal numbers: the mount unit's aspect.
fn aspect_style(v: &str) -> bool {
    let num = |s: &str| {
        let mut parts = s.splitn(2, '.');
        let int = parts.next().unwrap_or("");
        let frac = parts.next();
        (1..=6).contains(&int.len())
            && int.bytes().all(|b| b.is_ascii_digit())
            && frac
                .is_none_or(|f| (1..=4).contains(&f.len()) && f.bytes().all(|b| b.is_ascii_digit()))
    };
    v.strip_prefix("--ar:")
        .and_then(|r| r.split_once(" / "))
        .is_some_and(|(w, h)| num(w) && num(h))
}

fn escape_text(s: &str, out: &mut String) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '\u{a0}' => out.push_str("&nbsp;"),
            _ => out.push(c),
        }
    }
}

fn escape_attr(s: &str, out: &mut String) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '\u{a0}' => out.push_str("&nbsp;"),
            _ => out.push(c),
        }
    }
}

#[cfg(test)]
mod tests;
