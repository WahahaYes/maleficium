//! The article the reader page shows: the converted document through the
//! reader pipeline, in the contract's order.
//!
//! 1. (the caller converted the main file: [`super::convert`])
//! 2. [`figures::embed`] makes every figure a bundle file or inline data;
//! 3. [`join`] pairs the widget placeholders with the sidecar and
//!    [`join::apply`] leaves a `<!--mount:ID-->` marker in each one's place;
//! 4. the markers become mount units ([`mount_unit`]) right away, before
//!    anything that drops comments;
//! 5. post-processing: the article element alone (no latexml page chrome),
//!    a contents list after the abstract, in-page links (citations to the
//!    bibliography, references to figures) made `#fragment` links and dead
//!    ones unlinked;
//! 6. [`sanitize`] last.
//!
//! Headings keep the numbers latexml typeset (`ltx_tag`): unnumbered
//! sections stay unnumbered. None of the problems found stops the article:
//! each becomes an [`Issue`] for the export report.

use super::figures::{self, FigureWarning};
use super::join::{self, JoinError};
use super::sanitize;
use crate::bundle::fold;

use std::collections::{BTreeSet, HashSet};
use std::path::Path;

/// One widget's mount unit, as the page's script finds and mounts it.
pub struct Mount<'a> {
    pub id: &'a str,
    pub kind: &'a str,
    /// The rendered figure number or label, when the document gave one.
    pub figure: Option<&'a str>,
    pub label: Option<&'a str>,
    pub alt: &'a str,
    /// The frame's `--ar` pair: the author's box when the sidecar recorded
    /// author dims (see [`frame`]), else the pdf rect.
    pub width: f64,
    pub height: f64,
    /// The author's own box in points, when the sidecar recorded author
    /// dims: the frame keeps this width (at most the column) so a tall
    /// poster never stretches the frame past what the author asked.
    pub author_width: Option<f64>,
    pub author_height: Option<f64>,
    /// An `<img src>`: a bundled path or a `data:` url.
    pub poster: String,
    /// Why the widget shows only its poster (a custom runtime left out of
    /// this copy), shown as text under the caption.
    pub note: Option<String>,
    /// An html widget the in-app article holds back for approval: its figure
    /// carries `data-approval="required"` so the page can offer approval
    /// without running the widget.
    pub approval_required: bool,
}

/// What the pipeline found wrong; never a reason to drop the article.
#[derive(Debug, Clone, PartialEq)]
pub enum Issue {
    /// A figure became a visible placeholder.
    Figure(FigureWarning),
    /// The widget placeholders and the sidecar disagree: the widgets are
    /// listed at the end of the article instead of in place.
    Join(JoinError),
    /// Elements latexml marked `ltx_ERROR` (an undefined macro shows as its
    /// raw TeX), grouped by macro name with the spots each one owns.
    Undefined(Vec<UndefinedMacro>),
}

/// One undefined macro and how many article spots show its raw TeX.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UndefinedMacro {
    pub name: String,
    pub spots: usize,
}

/// The macro an `ltx_ERROR` span shows: its first `\name`, or the trimmed
/// raw TeX when the span holds none (an environment error shows `{name}`).
fn macro_name(text: &str) -> String {
    let mut rest = text;
    while let Some(at) = rest.find('\\') {
        let tail = &rest[at + 1..];
        let len = tail
            .char_indices()
            .take_while(|(_, c)| c.is_ascii_alphabetic() || *c == '@')
            .map(|(i, c)| i + c.len_utf8())
            .last()
            .unwrap_or(0);
        if len > 0 {
            return format!("\\{}", &tail[..len]);
        }
        rest = tail;
    }
    let flat: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut short = flat.trim().to_string();
    if short.is_empty() {
        short = String::from("(unreadable TeX)");
    }
    const MAX: usize = 32;
    if short.len() > MAX {
        short.truncate(MAX);
    }
    short
}

/// Group span texts into per-macro counts, most spots first, ties by name.
fn group_undefined(texts: Vec<String>) -> Vec<UndefinedMacro> {
    let mut counts: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    for t in texts {
        *counts.entry(macro_name(&t)).or_default() += 1;
    }
    let mut groups: Vec<UndefinedMacro> = counts
        .into_iter()
        .map(|(name, spots)| UndefinedMacro { name, spots })
        .collect();
    groups.sort_by(|a, b| b.spots.cmp(&a.spots).then(a.name.cmp(&b.name)));
    groups
}

pub struct Input<'a> {
    /// The converted HTML document.
    pub html: &'a str,
    /// The directory figures resolve against: the main file's, inside the
    /// project. A figure outside it becomes a placeholder; it is never widened.
    pub root: &'a Path,
    pub mode: figures::Mode,
    /// `\graphicspath` entries, relative to `root`.
    pub graphics_paths: Vec<String>,
    /// The `<jobname>.mfw` sidecar text, absent when the compile left none.
    pub sidecar: Option<&'a str>,
    pub mounts: &'a [Mount<'a>],
    /// The page title, shown when the article has none of its own.
    pub title: &'a str,
}

pub struct Article {
    /// The sanitized `<article>` element.
    pub html: String,
    pub issues: Vec<Issue>,
}

/// A widget frame's box: the `--ar` pair and the author's box in points,
/// when the sidecar recorded author dims.
///
/// The author's dims win. Both given is the author's box; one given keeps
/// that side and sizes the other from the content; neither keeps the pdf
/// rect. The content is the model's `size=` when it has one, else the rect
/// (a table's rows box already is its content; a poster the author chose
/// stands in for the rest). A model without `size=` is 4:3, the box the
/// package draws for it.
pub fn frame(
    kind: &str,
    size: Option<(u32, u32)>,
    author: (Option<f64>, Option<f64>),
    rect: (f64, f64),
) -> ((f64, f64), Option<(f64, f64)>) {
    let content = if kind == "model" {
        size.map(|(w, h)| (f64::from(w), f64::from(h)))
            .filter(|(w, h)| *w > 0.0 && *h > 0.0)
            .unwrap_or((4.0, 3.0))
    } else if rect.0 > 0.0 && rect.1 > 0.0 {
        rect
    } else {
        (4.0, 3.0)
    };
    match author {
        (Some(w), Some(h)) if w > 0.0 && h > 0.0 => ((w, h), Some((w, h))),
        (Some(w), None) if w > 0.0 => {
            ((content.0, content.1), Some((w, w * content.1 / content.0)))
        }
        (None, Some(h)) if h > 0.0 => {
            ((content.0, content.1), Some((h * content.0 / content.1, h)))
        }
        _ => {
            let ar = if rect.0 > 0.0 && rect.1 > 0.0 {
                rect
            } else {
                (4.0, 3.0)
            };
            (ar, None)
        }
    }
}

/// The mount unit markup `reader.js` mounts a widget into.
pub fn mount_unit(m: &Mount) -> String {
    let ar = if m.width > 0.0 && m.height > 0.0 {
        format!("{:.2} / {:.2}", m.width, m.height)
    } else {
        "4 / 3".to_string()
    };
    let author = match (m.author_width, m.author_height) {
        (Some(w), Some(h)) if w > 0.0 && h > 0.0 => format!("; --aw:{w:.2}pt; --ah:{h:.2}pt"),
        _ => String::new(),
    };
    let name = m
        .figure
        .filter(|f| !f.is_empty())
        .or(m.label.filter(|l| !l.is_empty()));
    let cap = match name {
        Some(n) => format!("<strong>{}</strong> {}", fold::text(n), fold::text(m.alt)),
        None => fold::text(m.alt),
    };
    let note = m.note.as_deref().map_or(String::new(), |n| {
        format!("<p class=\"m-widget-note\">{}</p>", fold::text(n))
    });
    let approval = if m.approval_required {
        " data-approval=\"required\""
    } else {
        ""
    };
    format!(
        "<figure id=\"{id}\" data-widget=\"{id}\" data-type=\"{kind}\"{approval} style=\"--ar:{ar}{author}\"><div class=\"frame\"><img class=\"poster\" src=\"{poster}\" alt=\"{alt}\"></div><figcaption>{cap}</figcaption>{note}</figure>",
        id = fold::attr(m.id),
        kind = fold::attr(m.kind),
        poster = fold::attr(&m.poster),
        alt = fold::attr(m.alt),
    )
}

/// Runs steps 2 to 6. Fails only when the figures cannot be written at all
/// (the root is not a directory, or `figures/` cannot be created).
pub fn build(input: Input) -> Result<Article, String> {
    let mut issues = Vec::new();
    let opts = figures::Options {
        graphics_paths: input.graphics_paths.clone(),
        ..figures::Options::default()
    };
    let embedded = figures::embed(input.html, input.root, &input.mode, &opts)?;
    issues.extend(embedded.warnings.into_iter().map(Issue::Figure));

    let joined = match input.sidecar {
        Some(text) => join::join_sidecar(&embedded.html, text),
        None => join::join(&embedded.html, Vec::new()),
    }
    .and_then(|j| join::apply(&embedded.html, &j).map(|html| (html, j)));
    let (mut html, joined) = match joined {
        Ok(ok) => ok,
        Err(e) => {
            issues.push(Issue::Join(e));
            (embedded.html, Vec::new())
        }
    };

    // Step 4: markers to mount units, before any comment is dropped.
    let mut placed: HashSet<&str> = HashSet::new();
    for j in &joined {
        let Some(m) = input.mounts.iter().find(|m| m.id == j.record.id) else {
            continue;
        };
        let marker = join::mount_marker(m.id);
        if let Some(at) = html.find(&marker) {
            html.replace_range(at..at + marker.len(), &mount_unit(m));
            placed.insert(m.id);
        }
    }
    let unplaced: Vec<&Mount> = input
        .mounts
        .iter()
        .filter(|m| !placed.contains(m.id))
        .collect();

    let (post, undefined) = post_process(&html, input.title, &unplaced);
    if !undefined.is_empty() {
        issues.push(Issue::Undefined(undefined));
    }
    let ids: BTreeSet<String> = input.mounts.iter().map(|m| m.id.to_string()).collect();
    Ok(Article {
        html: sanitize::sanitize(&post, &ids),
        issues,
    })
}

/// Step 5. Returns the article element's html and the `ltx_ERROR` spans
/// grouped by macro name.
fn post_process(html: &str, title: &str, unplaced: &[&Mount]) -> (String, Vec<UndefinedMacro>) {
    let doc = dom_query::Document::from(html);
    let found = doc.select("article.ltx_document").first();
    let article = match found.nodes().first() {
        Some(n) => *n,
        None => {
            // Not latexml's page: take the body's content as the article.
            let body = doc
                .body()
                .map(|b| b.inner_html().to_string())
                .unwrap_or_default();
            return post_process(
                &format!("<article class=\"ltx_document\">{body}</article>"),
                title,
                unplaced,
            );
        }
    };

    if !doc
        .select("article.ltx_document .ltx_title_document")
        .exists()
    {
        article.prepend_html(format!(
            "<h1 class=\"ltx_title ltx_title_document\">{}</h1>",
            fold::text(title)
        ));
    }

    // The page colour is the reader theme's (`--m-paper`), not latexml's.
    page_background(&article);

    // A widget inside a captioned float: the float's caption is its caption.
    for fig in doc.select("figure[data-widget]").nodes() {
        let float = fig
            .ancestors(None)
            .into_iter()
            .find(|a| a.has_name("figure"));
        let captioned =
            float.is_some_and(|f| f.children().iter().any(|c| c.has_name("figcaption")));
        if captioned {
            let caps: Vec<_> = fig
                .children()
                .into_iter()
                .filter(|c| c.has_name("figcaption"))
                .collect();
            for cap in caps {
                cap.remove_from_parent();
            }
        }
    }

    // A proof's closing mark is amsthm's qed symbol (U+220E), which renders
    // as a solid box in fonts without the glyph: draw the box in CSS
    // instead, so the mark reads in every browser.
    for proof in doc.select("article.ltx_document .ltx_proof").nodes() {
        qed_box(proof);
    }

    // A proof folds: an open `<details>` whose summary is its run-in title,
    // so with scripts off and in print it reads open and can still be
    // folded by hand. The reader script folds proofs on load.
    for proof in doc.select("article.ltx_document div.ltx_proof").nodes() {
        fold_proof(proof);
    }

    // In-page links: a citation or reference to `page.html#x` becomes `#x`
    // when `x` is in the article; a fragment with no target is unlinked.
    let ids: HashSet<String> = article
        .descendants()
        .iter()
        .filter_map(|n| n.id_attr().map(|i| i.to_string()))
        .collect();
    for a in doc.select("article.ltx_document a[href]").nodes() {
        let href = a.attr("href").map(|h| h.to_string()).unwrap_or_default();
        let lower = href.to_ascii_lowercase();
        if lower.starts_with("http://") || lower.starts_with("https://") {
            continue;
        }
        match href.split_once('#') {
            Some((_, frag)) if ids.contains(frag) => a.set_attr("href", &format!("#{frag}")),
            _ => a.remove_attr("href"),
        }
    }

    let undefined = group_undefined(
        doc.select("article.ltx_document .ltx_ERROR")
            .nodes()
            .iter()
            .map(|n| n.text().to_string())
            .collect(),
    );

    let entries = contents(&article, 0);
    if entries.len() > 1 {
        let mut nav = String::from(
            "<nav class=\"m-contents\" aria-label=\"Contents\"><h2 class=\"m-contents-title\">Contents</h2>",
        );
        write_entries(&entries, &mut nav);
        nav.push_str("</nav>");
        let abstract_ = doc.select("article.ltx_document > .ltx_abstract").first();
        let first_section = doc.select("article.ltx_document > section").first();
        if abstract_.exists() {
            abstract_.after_html(nav);
        } else if first_section.exists() {
            first_section.before_html(nav);
        } else {
            article.append_html(nav);
        }
    }

    if !unplaced.is_empty() {
        let mut s = String::from(
            "<section class=\"m-unplaced\"><h2 class=\"ltx_title\">Interactive figures</h2>",
        );
        for m in unplaced {
            s.push_str(&mount_unit(m));
        }
        s.push_str("</section>");
        article.append_html(s);
    }
    (article.html().to_string(), undefined)
}

/// Drop latexml's copies of the page colour from the math. `\pagecolor`
/// is a font background to latexml: text carries it once, as
/// `--ltx-bg-color` on the article root (a style the sanitizer drops), but
/// the MathML writer resolves the inherited background and restates it as
/// `mathbackground` on every token, which would paint the page's light
/// colour behind each formula on a dark reader. The reader takes the page
/// colour from the theme record instead, so a math background inherited from
/// the root goes; one set by a box inside the article (`\colorbox`) stays.
fn page_background(article: &dom_query::NodeRef) {
    let Some(page) = article.attr("style").as_deref().and_then(ltx_background) else {
        return;
    };
    for n in article.descendants() {
        let Some(bg) = n.attr("mathbackground") else {
            continue;
        };
        if !bg.eq_ignore_ascii_case(&page) {
            continue;
        }
        let boxed = n
            .ancestors(None)
            .into_iter()
            .take_while(|a| a.id != article.id)
            .any(|a| {
                a.attr("style")
                    .as_deref()
                    .and_then(ltx_background)
                    .is_some()
            });
        if !boxed {
            n.remove_attr("mathbackground");
        }
    }
}

/// The value of `--ltx-bg-color` in an inline style, latexml's background.
fn ltx_background(style: &str) -> Option<String> {
    style.split(';').find_map(|d| {
        let (k, v) = d.split_once(':')?;
        (k.trim() == "--ltx-bg-color").then(|| v.trim().to_string())
    })
}

/// Replace a proof-final U+220E with a CSS-drawn end mark (`m-qed` in the
/// reader stylesheet). Only a trailing mark is touched: a symbol quoted
/// mid-proof stays text. Latexml leaves the mark bare, doubled (`\qed` plus
/// its own), or wrapped in MathML (`\qedhere` in math is a lone `mo`);
/// equation and item numbers after it are chrome, not content.
fn qed_box(proof: &dom_query::NodeRef) {
    let mut last: Option<dom_query::NodeRef> = None;
    for n in proof.descendants() {
        if n.is_text() && !n.text().trim().is_empty() && !in_chrome(&n) {
            last = Some(n);
        }
    }
    let Some(t) = last else {
        return;
    };
    let text = t.text().to_string();
    let trimmed = text.trim_end().to_string();
    let head = trimmed.trim_end_matches(QED).to_string();
    if head.len() == trimmed.len() {
        return;
    }
    if let Some(math) = math_ancestor(&t) {
        // The mark is all a MathML wrapper holds: drop the wrapper and
        // draw the mark after the formula, where it reads as the proof end.
        let mut qed_node = t;
        let mut at = t.parent();
        while let Some(p) = at {
            if !p.is_element() || p.has_name("math") || !is_qed_only(&p) {
                break;
            }
            qed_node = p;
            at = p.parent();
        }
        math.after_html(QED_MARK);
        qed_node.remove_from_parent();
        return;
    }
    t.after_html(QED_MARK);
    if head.trim().is_empty() {
        t.remove_from_parent();
    } else {
        t.set_text(head);
    }
}

/// Turn latexml's `div.ltx_proof` into `<details class="ltx_proof" open>`
/// with its title (`Proof.`, or the author's `Proof of ...`) as the
/// `<summary>`. A proof latexml gave no title gets the plain one.
fn fold_proof(proof: &dom_query::NodeRef) {
    let title = proof
        .first_element_child()
        .filter(|c| c.has_class("ltx_title"))
        .map(|t| {
            let html = t.html().to_string();
            t.remove_from_parent();
            html
        })
        .unwrap_or_else(|| PROOF_TITLE.to_string());
    proof.rename("details");
    proof.set_attr("open", "");
    proof.prepend_html(format!(
        "<summary class=\"m-proof-summary\">{title}</summary>"
    ));
}

/// The title latexml gives a plain `proof`, for one that arrives without.
const PROOF_TITLE: &str =
    "<h6 class=\"ltx_title ltx_runin ltx_font_italic ltx_title_proof\">Proof.</h6>";

/// The proof-final mark latexml emits, drawn in CSS instead (`U+220E`
/// renders as a solid box in fonts without the glyph).
const QED: char = '\u{220e}';
/// The CSS-drawn end mark replacing it.
const QED_MARK: &str = "<span class=\"m-qed\" role=\"img\" aria-label=\"End of proof\"></span>";

/// The `math` element a node sits inside, if any.
fn math_ancestor<'a>(n: &dom_query::NodeRef<'a>) -> Option<dom_query::NodeRef<'a>> {
    n.ancestors(None).into_iter().find(|a| a.has_name("math"))
}

/// Whether an element holds nothing but the mark (and whitespace).
fn is_qed_only(n: &dom_query::NodeRef) -> bool {
    let s = n.text();
    !s.trim().is_empty() && s.trim().chars().all(|c| c == QED)
}

/// Whether a text node is typeset chrome (an equation or item number),
/// which never counts as content after the mark.
fn in_chrome(n: &dom_query::NodeRef) -> bool {
    n.ancestors(None).into_iter().any(|a| {
        a.attr("class")
            .map(|c| c.to_string())
            .is_some_and(|c| c.split_whitespace().any(|t| t == "ltx_tag"))
    })
}

struct Entry {
    id: String,
    text: String,
    children: Vec<Entry>,
}

/// The sections directly under `node` (latexml nests a subsection directly
/// in its section) that have an id and a heading, two levels deep.
fn contents(node: &dom_query::NodeRef, depth: usize) -> Vec<Entry> {
    let mut out = Vec::new();
    for child in node.element_children() {
        if !child.has_name("section") {
            continue;
        }
        let heading = child.element_children().into_iter().find(|c| {
            ["h1", "h2", "h3", "h4", "h5", "h6"]
                .iter()
                .any(|h| c.has_name(h))
        });
        if let (Some(id), Some(h)) = (child.id_attr(), heading) {
            out.push(Entry {
                id: id.to_string(),
                text: h.text().split_whitespace().collect::<Vec<_>>().join(" "),
                children: if depth == 0 {
                    contents(&child, depth + 1)
                } else {
                    Vec::new()
                },
            });
        }
    }
    out
}

fn write_entries(entries: &[Entry], out: &mut String) {
    out.push_str("<ol>");
    for e in entries {
        out.push_str(&format!(
            "<li><a href=\"#{}\">{}</a>",
            fold::attr(&e.id),
            fold::text(&e.text)
        ));
        if !e.children.is_empty() {
            write_entries(&e.children, out);
        }
        out.push_str("</li>");
    }
    out.push_str("</ol>");
}

/// One heading anchor of a reader page, in document order: the section id
/// the editor-to-article sync scrolls to, with its heading text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Anchor {
    pub id: String,
    pub text: String,
}

/// The heading anchors of a rendered reader page: the same sections the
/// contents list links (top level plus one nesting level), flattened in
/// document order. Reads the article element only, so the JSON islands
/// (whose `<` bytes are escaped) can never contribute one.
pub fn anchors_of(page_html: &str) -> Vec<Anchor> {
    let doc = dom_query::Document::from(page_html);
    let found = doc.select("article.ltx_document").first();
    let Some(article) = found.nodes().first() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for e in contents(article, 0) {
        out.push(Anchor {
            id: e.id,
            text: e.text,
        });
        for c in e.children {
            out.push(Anchor {
                id: c.id,
                text: c.text,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests;
