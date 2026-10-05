//! Document structure of a paper from its conversion, not its PDF: the
//! sections, theorems, equations, citations, figures and widgets of one
//! main file's converted HTML.
//!
//! The converter ([`crate::reflow::convert`]) turns the main file into one
//! HTML document; [`parse`] reads the six categories out of that HTML, and
//! [`document_structure`] runs the two ends (convert, then parse) against a
//! session root. Nothing here parses the PDF: widget geometry and full
//! widget records stay with the [`crate::widgets`] tool, which reads the
//! compiled output. What this reports for widgets is the conversion's own
//! placeholders in document order (each entry's `index` and `kind`), so the
//! call works before any compile.
//!
//! Two honest limits of reading the conversion. Citation keys are latexml's
//! bibliography ids (`bib.bib1`), not the `\cite` keys as written: the
//! source-level keys live with the `citations` tool. Figures that hold a
//! widget placeholder are listed under both figures (their caption) and
//! widgets (their kind).

use crate::Core;

use serde::Serialize;
use std::path::{Path, PathBuf};
use ts_rs::TS;

/// Rows per list; the rest are counted in `truncated`.
const MAX_ROWS: usize = 1000;
/// Longest statement or bibliography entry kept, in chars.
const MAX_TEXT_CHARS: usize = 500;

/// One converted heading: its latexml id, its typeset title and its depth
/// (1 for a top-level section). The bibliography section is not one.
#[derive(Debug, Clone, PartialEq, Serialize, schemars::JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct Section {
    pub id: String,
    pub title: String,
    pub level: u32,
}

/// One `ltx_theorem` block: its latexml id (empty when unlabeled), the
/// environment kind (`theorem`, `lemma`, …), its heading when it has one
/// and its text, capped at [`MAX_TEXT_CHARS`] chars.
#[derive(Debug, Clone, PartialEq, Serialize, schemars::JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct Theorem {
    pub id: String,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub title: Option<String>,
    pub statement: String,
}

/// One `ltx_Math` element: its latexml id, the TeX latexml read
/// (`alttext`), whether it sat inline or as a block, and the equation
/// number beside it when the conversion typeset one.
#[derive(Debug, Clone, PartialEq, Serialize, schemars::JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct Equation {
    pub id: String,
    pub tex: String,
    pub display: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub number: Option<String>,
}

/// One bibliography entry by its converted id: how many `ltx_cite` links
/// point at it and its entry text, capped at [`MAX_TEXT_CHARS`] chars.
/// Cited ids with no entry keep `entry` absent.
#[derive(Debug, Clone, PartialEq, Serialize, schemars::JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct Citation {
    pub key: String,
    pub cites: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub entry: Option<String>,
}

/// One `ltx_figure`: its latexml id, its caption text and the graphic the
/// converter recorded for it (`data-graphic`), absent for widget floats
/// and uncaptioned images.
#[derive(Debug, Clone, PartialEq, Serialize, schemars::JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct Figure {
    pub id: String,
    pub caption: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub graphic: Option<String>,
}

/// One widget placeholder of the conversion in document order: its position
/// and its kind. Ids, pages and rects are the `widgets` tool's job.
#[derive(Debug, Clone, PartialEq, Serialize, schemars::JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct WidgetRef {
    pub index: u32,
    pub kind: crate::widgets::WidgetType,
}

/// The six categories of one paper, in document order within each.
#[derive(Debug, Clone, PartialEq, Serialize, schemars::JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct DocumentStructure {
    pub source: String,
    pub revision: String,
    pub main: String,
    pub sections: Vec<Section>,
    pub theorems: Vec<Theorem>,
    pub equations: Vec<Equation>,
    pub citations: Vec<Citation>,
    pub figures: Vec<Figure>,
    pub widgets: Vec<WidgetRef>,
    pub truncated: usize,
}

/// The six lists before the envelope (source, revision, main) is added.
#[derive(Debug, Default)]
pub struct Parts {
    pub sections: Vec<Section>,
    pub theorems: Vec<Theorem>,
    pub equations: Vec<Equation>,
    pub citations: Vec<Citation>,
    pub figures: Vec<Figure>,
    pub widgets: Vec<WidgetRef>,
}

/// Whitespace-normalized text.
fn words(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Whitespace-normalized text, capped at `max` chars.
fn capped(s: &str, max: usize) -> String {
    let s = words(s);
    if s.chars().count() <= max {
        return s;
    }
    let mut end = max;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &s[..end])
}

fn class_of(n: &dom_query::NodeRef) -> String {
    n.attr("class").map(|c| c.to_string()).unwrap_or_default()
}

fn has_class(n: &dom_query::NodeRef, class: &str) -> bool {
    class_of(n).split_whitespace().any(|c| c == class)
}

fn id_of(n: &dom_query::NodeRef) -> String {
    n.id_attr().map(|i| i.to_string()).unwrap_or_default()
}

/// The first heading child (`h1`..`h6`) of a section or theorem block.
fn heading<'a>(n: &dom_query::NodeRef<'a>) -> Option<dom_query::NodeRef<'a>> {
    n.element_children().into_iter().find(|c| {
        ["h1", "h2", "h3", "h4", "h5", "h6"]
            .iter()
            .any(|h| c.has_name(h))
    })
}

/// The equation number beside a `ltx_Math` element: the text of the
/// `ltx_eqn_eqno` cell of its row, if it has one.
fn equation_number(m: &dom_query::NodeRef) -> Option<String> {
    let row = m
        .ancestors(None)
        .into_iter()
        .find(|a| a.has_name("tr") && has_class(a, "ltx_equation"))?;
    for cell in row.element_children() {
        if (cell.has_name("td") || cell.has_name("th")) && has_class(&cell, "ltx_eqn_eqno") {
            let t = words(cell.text().as_ref());
            if !t.is_empty() {
                return Some(t);
            }
        }
    }
    None
}

/// The six categories of converted HTML, in document order within each.
/// Pure: no fs, no converter, so tests hand it HTML directly.
pub fn parse(html: &str) -> Parts {
    let doc = dom_query::Document::from(html);
    let mut out = Parts::default();

    for s in doc.select("section").nodes() {
        if has_class(s, "ltx_bibliography") {
            continue;
        }
        let id = id_of(s);
        if id.is_empty() {
            continue;
        }
        let level = s
            .ancestors(None)
            .into_iter()
            .filter(|a| a.has_name("section"))
            .count() as u32
            + 1;
        let title = heading(s)
            .map(|h| words(h.text().as_ref()))
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| "(untitled)".to_string());
        out.sections.push(Section { id, title, level });
    }

    for t in doc.select("div.ltx_theorem").nodes() {
        let kind = class_of(t)
            .split_whitespace()
            .find_map(|c| c.strip_prefix("ltx_theorem_"))
            .unwrap_or("theorem")
            .to_string();
        let title = heading(t)
            .map(|h| words(h.text().as_ref()))
            .filter(|t| !t.is_empty());
        out.theorems.push(Theorem {
            id: id_of(t),
            kind,
            title,
            statement: capped(t.text().as_ref(), MAX_TEXT_CHARS),
        });
    }

    for m in doc.select("math.ltx_Math").nodes() {
        let id = id_of(m);
        if id.is_empty() {
            continue;
        }
        out.equations.push(Equation {
            id,
            tex: m.attr("alttext").map(|v| v.to_string()).unwrap_or_default(),
            display: m
                .attr("display")
                .map(|v| v.to_string())
                .unwrap_or_else(|| "inline".to_string()),
            number: equation_number(m),
        });
    }

    let mut uses: Vec<String> = Vec::new();
    for a in doc.select("cite.ltx_cite a[href]").nodes() {
        let href = a.attr("href").map(|h| h.to_string()).unwrap_or_default();
        if let Some((_, frag)) = href.split_once('#') {
            let frag = frag.trim();
            if !frag.is_empty() {
                uses.push(frag.to_string());
            }
        }
    }
    let mut entries: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for li in doc.select("li.ltx_bibitem").nodes() {
        let id = id_of(li);
        if id.is_empty() {
            continue;
        }
        entries
            .entry(id)
            .or_insert_with(|| capped(li.text().as_ref(), MAX_TEXT_CHARS));
    }
    let mut keys: Vec<String> = uses.to_vec();
    for id in entries.keys() {
        if !keys.contains(id) {
            keys.push(id.clone());
        }
    }
    keys.sort();
    keys.dedup();
    for key in keys {
        out.citations.push(Citation {
            cites: uses.iter().filter(|u| *u == &key).count() as u32,
            entry: entries.get(&key).cloned(),
            key,
        });
    }

    let mut captions: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for cap in doc.select("figure.ltx_figure figcaption").nodes() {
        let Some(fig) = cap
            .ancestors(None)
            .into_iter()
            .find(|a| a.has_name("figure"))
        else {
            continue;
        };
        let id = id_of(&fig);
        if id.is_empty() {
            continue;
        }
        captions
            .entry(id)
            .or_insert_with(|| words(cap.text().as_ref()));
    }
    let mut graphics: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for img in doc.select("figure.ltx_figure img.ltx_graphics").nodes() {
        let Some(fig) = img
            .ancestors(None)
            .into_iter()
            .find(|a| a.has_name("figure"))
        else {
            continue;
        };
        let id = id_of(&fig);
        if id.is_empty() {
            continue;
        }
        let g = img
            .attr("data-graphic")
            .map(|v| v.to_string())
            .unwrap_or_default();
        if g.is_empty() {
            continue;
        }
        graphics.entry(id).or_insert(g);
    }
    for fig in doc.select("figure.ltx_figure").nodes() {
        let id = id_of(fig);
        if id.is_empty() {
            continue;
        }
        out.figures.push(Figure {
            caption: captions.remove(&id).unwrap_or_default(),
            graphic: graphics.remove(&id),
            id,
        });
    }

    out
}

/// Root-relative `/`-string of a path already inside `root`.
fn rel_of(root: &Path, abs: &Path) -> String {
    abs.strip_prefix(root)
        .unwrap_or(abs)
        .components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

/// Keep the first `MAX_ROWS`; return how many were dropped.
fn cap<T>(v: &mut Vec<T>) -> usize {
    let over = v.len().saturating_sub(MAX_ROWS);
    v.truncate(MAX_ROWS);
    over
}

/// An app-owned scratch folder for one conversion, removed either way.
struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Result<Scratch, String> {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("maleficium-document-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("convert"))
            .map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
        Ok(Scratch(dir))
    }

    fn convert_dir(&self) -> PathBuf {
        self.0.join("convert")
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The six categories of a main file's conversion. Converts the main file
/// from its saved files (`source` is always `disk`), then parses the
/// result; a conversion that produced no HTML is an error, never an empty
/// document. Needs no compile: widgets are the conversion's placeholders.
pub fn document_structure(
    cx: &Core,
    root_id: &str,
    main_rel: &str,
) -> Result<DocumentStructure, String> {
    let root = super::fs::session_root(cx, root_id)?;
    let abs = super::fs::resolve_in(cx, root_id, main_rel)?;
    if !abs.is_file() {
        return Err(format!("not a file: {main_rel}"));
    }
    let rel = rel_of(&root, &abs);
    let scratch = Scratch::new()?;
    let conversion =
        super::reflow::convert::converter(cx).convert(cx, &abs, &scratch.convert_dir());
    let html = conversion
        .html
        .map_err(|e| format!("{main_rel} could not be converted to HTML: {e}"))?;
    let placeholders =
        super::reflow::join::extract_placeholders(&html).map_err(|e| e.to_string())?;
    let mut p = parse(&html);
    p.widgets = placeholders
        .into_iter()
        .map(|h| WidgetRef {
            index: h.index as u32,
            kind: h.kind,
        })
        .collect();
    let truncated = cap(&mut p.sections)
        + cap(&mut p.theorems)
        + cap(&mut p.equations)
        + cap(&mut p.citations)
        + cap(&mut p.figures)
        + cap(&mut p.widgets);
    Ok(DocumentStructure {
        source: "disk".to_string(),
        revision: maleficium_structure::revision([
            ("document", rel.as_str()),
            ("article", html.as_str()),
        ]),
        main: rel,
        sections: p.sections,
        theorems: p.theorems,
        equations: p.equations,
        citations: p.citations,
        figures: p.figures,
        widgets: p.widgets,
        truncated,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A latexml-shaped conversion of e2e/fixtures/interactive (see
    /// reflow/article): two sections, four figures, one citation, one
    /// inline equation and five widget placeholders.
    const CONVERTED: &str = include_str!("reflow/fixtures/interactive-converted.frag");

    #[test]
    fn sections_come_from_the_converted_headings() {
        let p = parse(CONVERTED);
        let got: Vec<(&str, &str, u32)> = p
            .sections
            .iter()
            .map(|s| (s.id.as_str(), s.title.as_str(), s.level))
            .collect();
        assert_eq!(
            got,
            vec![("S1", "1 Widgets", 1), ("S1.SS1", "1.1 Plain figures", 2),]
        );
    }

    #[test]
    fn theorems_keep_kind_title_and_statement() {
        let html = "<article class=\"ltx_document\"><section id=\"S1\"><h2>1 Results</h2>\
            <div id=\"Thm1\" class=\"ltx_theorem ltx_theorem_theorem\"><h6 class=\"ltx_title ltx_title_theorem\"><span class=\"ltx_tag ltx_tag_theorem\">Theorem 1. </span>Convergence</h6>\
            <div class=\"ltx_para\"><p class=\"ltx_p\">Every bounded sequence converges somewhere.</p></div></div>\
            <div id=\"Lem1\" class=\"ltx_theorem ltx_theorem_lemma\"><div class=\"ltx_para\"><p class=\"ltx_p\">A helper fact.</p></div></div>\
            </section></article>";
        let p = parse(html);
        assert_eq!(p.theorems.len(), 2);
        assert_eq!(p.theorems[0].id, "Thm1");
        assert_eq!(p.theorems[0].kind, "theorem");
        assert_eq!(
            p.theorems[0].title.as_deref(),
            Some("Theorem 1. Convergence")
        );
        assert!(p.theorems[0].statement.contains("bounded sequence"));
        assert_eq!(p.theorems[1].kind, "lemma");
        assert_eq!(p.theorems[1].title, None);
    }

    #[test]
    fn equations_keep_tex_display_and_number() {
        let html = "<article class=\"ltx_document\"><div id=\"S1.p1\" class=\"ltx_para\"><p class=\"ltx_p\">Inline \
            <math id=\"S1.p1.m1\" class=\"ltx_Math\" alttext=\"x^{2}\" display=\"inline\"><msup><mi>x</mi><mn>2</mn></msup></math>.</p></div>\
            <table class=\"ltx_equation ltx_eqn_table\"><tr class=\"ltx_equation ltx_eqn_row\">\
            <td class=\"ltx_eqn_cell ltx_eqn_center_padleft\"><math id=\"S1.E1.m1\" class=\"ltx_Math\" alttext=\"E=mc^{2}\" display=\"block\"><mi>E</mi></math></td>\
            <td class=\"ltx_eqn_cell ltx_eqn_eqno\"><span class=\"ltx_tag ltx_tag_equation\">(1)</span></td>\
            </tr></table></article>";
        let p = parse(html);
        assert_eq!(p.equations.len(), 2);
        assert_eq!(p.equations[0].tex, "x^{2}");
        assert_eq!(p.equations[0].display, "inline");
        assert_eq!(p.equations[0].number, None);
        assert_eq!(p.equations[1].tex, "E=mc^{2}");
        assert_eq!(p.equations[1].display, "block");
        assert_eq!(p.equations[1].number.as_deref(), Some("(1)"));
    }

    #[test]
    fn citations_count_uses_and_keep_entries() {
        let p = parse(CONVERTED);
        assert_eq!(p.citations.len(), 1);
        assert_eq!(p.citations[0].key, "bib.bib1");
        assert_eq!(p.citations[0].cites, 1);
        let entry = p.citations[0].entry.as_deref().unwrap_or_default();
        assert!(entry.contains("Knuth"), "{entry}");
    }

    #[test]
    fn uncited_entries_are_listed_with_no_uses() {
        let html = "<article class=\"ltx_document\"><section id=\"bib\" class=\"ltx_bibliography\"><ul class=\"ltx_biblist\">\
            <li id=\"bib.a\" class=\"ltx_bibitem\">First entry</li>\
            <li id=\"bib.b\" class=\"ltx_bibitem\">Second entry</li></ul></section>\
            <p class=\"ltx_p\"><cite class=\"ltx_cite ltx_citemacro_cite\">[<a href=\"#bib.b\" class=\"ltx_ref\">2</a>]</cite></p></article>";
        let p = parse(html);
        assert!(p.sections.is_empty(), "the bibliography is not a section");
        let got: Vec<(&str, u32)> = p
            .citations
            .iter()
            .map(|c| (c.key.as_str(), c.cites))
            .collect();
        assert_eq!(got, vec![("bib.a", 0), ("bib.b", 1)]);
    }

    #[test]
    fn figures_keep_captions_and_graphics() {
        let p = parse(CONVERTED);
        let got: Vec<(&str, &str, Option<&str>)> = p
            .figures
            .iter()
            .map(|f| (f.id.as_str(), f.caption.as_str(), f.graphic.as_deref()))
            .collect();
        assert_eq!(
            got,
            vec![
                ("S1.F1", "Figure 1: A model.", None),
                ("S1.F2", "Figure 2: A video.", None),
                (
                    "S1.F3",
                    "Figure 3: The mesh as a plain figure.",
                    Some("figures/mesh"),
                ),
                (
                    "S1.F4",
                    "Figure 4: A figure whose file is missing.",
                    Some("figures/absent.png"),
                ),
            ]
        );
    }

    #[test]
    fn widgets_follow_placeholder_order() {
        let p = parse(CONVERTED);
        // Placeholders name kinds only; the widgets tool owns ids and rects.
        assert_eq!(p.widgets.len(), 5);
        let kinds: Vec<String> = p.widgets.iter().map(|w| format!("{:?}", w.kind)).collect();
        assert_eq!(kinds, ["Model", "Video", "Table", "Chart", "Html"]);
        for (i, w) in p.widgets.iter().enumerate() {
            assert_eq!(w.index, i as u32);
        }
    }

    /// A converter that hands back fixed HTML for the orchestration tests.
    struct Fixed {
        html: Result<String, String>,
    }

    impl super::super::reflow::convert::Converter for Fixed {
        fn convert(
            &self,
            _cx: &Core,
            main: &Path,
            work: &Path,
        ) -> super::super::reflow::convert::Conversion {
            assert!(main.is_absolute() && main.is_file(), "{}", main.display());
            assert!(work.is_dir(), "an empty scratch folder");
            super::super::reflow::convert::Conversion {
                html: self.html.clone(),
                errors: Vec::new(),
                log: String::new(),
            }
        }
    }

    fn project(name: &str, html: Result<String, String>) -> (Core, String) {
        let cx = Core::default();
        let dir = crate::test_scratch::dir(&format!("doc-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("main.tex"), "\\section{Hi}\n").unwrap();
        let canon = dunce::canonicalize(&dir).unwrap();
        let id = format!("doc-{name}");
        crate::grant_root(&cx, &id, &canon.to_string_lossy()).unwrap();
        cx.set_converter(std::sync::Arc::new(Fixed { html }));
        (cx, id)
    }

    #[test]
    fn the_call_converts_then_parses() {
        let (cx, id) = project("ok", Ok(CONVERTED.to_string()));
        let d = document_structure(&cx, &id, "main.tex").unwrap();
        assert_eq!(d.source, "disk");
        assert_eq!(d.main, "main.tex");
        assert_eq!(d.sections.len(), 2);
        assert_eq!(d.figures.len(), 4);
        assert_eq!(d.widgets.len(), 5);
        assert_eq!(d.truncated, 0);
        assert_eq!(
            d.revision,
            document_structure(&cx, &id, "main.tex").unwrap().revision
        );
        let json = serde_json::to_string(&d).unwrap();
        assert!(json.contains("\"main\":\"main.tex\""), "{json}");
    }

    #[test]
    fn a_conversion_without_html_is_an_error() {
        let (cx, id) = project("fail", Err("the converter failed".to_string()));
        let err = document_structure(&cx, &id, "main.tex").unwrap_err();
        assert!(err.contains("could not be converted"), "{err}");
    }

    #[test]
    fn roots_and_paths_are_confined() {
        let (cx, id) = project("confined", Ok(CONVERTED.to_string()));
        assert!(document_structure(&cx, "nope", "main.tex").is_err());
        assert!(document_structure(&cx, &id, "../x.tex").is_err());
        assert!(document_structure(&cx, &id, "missing.tex").is_err());
    }
}
