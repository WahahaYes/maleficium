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
    /// The widget's rect on its pdf page, in points.
    pub width: f64,
    pub height: f64,
    /// An `<img src>`: a bundled path or a `data:` url.
    pub poster: String,
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
    /// raw TeX), counted.
    Errors(usize),
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

/// The mount unit markup `reader.js` mounts a widget into.
pub fn mount_unit(m: &Mount) -> String {
    let ar = if m.width > 0.0 && m.height > 0.0 {
        format!("{:.2} / {:.2}", m.width, m.height)
    } else {
        "4 / 3".to_string()
    };
    let name = m
        .figure
        .filter(|f| !f.is_empty())
        .or(m.label.filter(|l| !l.is_empty()));
    let cap = match name {
        Some(n) => format!("<strong>{}</strong> {}", fold::text(n), fold::text(m.alt)),
        None => fold::text(m.alt),
    };
    format!(
        "<figure id=\"{id}\" data-widget=\"{id}\" data-type=\"{kind}\" style=\"--ar:{ar}\"><div class=\"frame\"><img class=\"poster\" src=\"{poster}\" alt=\"{alt}\"></div><figcaption>{cap}</figcaption></figure>",
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

    let (post, errors) = post_process(&html, input.title, &unplaced);
    if errors > 0 {
        issues.push(Issue::Errors(errors));
    }
    let ids: BTreeSet<String> = input.mounts.iter().map(|m| m.id.to_string()).collect();
    Ok(Article {
        html: sanitize::sanitize(&post, &ids),
        issues,
    })
}

/// Step 5. Returns the article element's html and the `ltx_ERROR` count.
fn post_process(html: &str, title: &str, unplaced: &[&Mount]) -> (String, usize) {
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

    let errors = doc.select("article.ltx_document .ltx_ERROR").length();

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
    (article.html().to_string(), errors)
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

#[cfg(test)]
mod tests;
