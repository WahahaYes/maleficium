//! Document outline of one text: O(text) scan.
//!
//! Sections carry the hierarchy; labels, floats, and \input boundaries ride
//! along as non-hierarchical marker rows at the current section level (the
//! indent column stays a pure section tree).

use std::sync::LazyLock;

use regex::Regex;
use serde::Serialize;
use ts_rs::TS;

use crate::text::{base_name, strip_comments, Lines};

/// Rows past this cap are counted in `Outline::truncated`, not returned.
pub const MAX_OUTLINE_ENTRIES: usize = 1000;

/// What a row IS: a sectioning command, or a marker riding the tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, schemars::JsonSchema, TS)]
#[serde(rename_all = "lowercase")]
pub enum OutlineKind {
    Section,
    Label,
    Figure,
    Table,
    Input,
}

#[derive(Debug, Clone, PartialEq, Serialize, schemars::JsonSchema, TS)]
pub struct OutlineEntry {
    pub level: u8,
    pub title: String,
    pub line: u32,
    pub kind: OutlineKind,
    /// Marker detail: label key, float graphics file, or input path.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, schemars::JsonSchema, TS)]
pub struct Outline {
    pub entries: Vec<OutlineEntry>,
    /// Rows dropped by the cap.
    pub truncated: usize,
}

// Bodies match `[^}]*` and are length-checked after: equivalent to a bounded
// repetition (the class cannot cross the closing brace) without the automaton
// blow-up a Unicode `{1,200}` costs.
static SECTION_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\\(chapter|section|subsection|subsubsection|paragraph)\*?\{([^}]*)\}").unwrap()
});
static LABEL_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\\label\{([^}]*)\}").unwrap());
static FLOAT_BEGIN_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\\begin\{(figure|table)\*?\}").unwrap());
static CAPTION_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\\caption(?:\[[^\]]*\])?\{([^}]*)\}").unwrap());
static GRAPHICS_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\\includegraphics(?:\[[^\]]*\])?\{([^}]*)\}").unwrap());
static INPUT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\\(?:input|include)\{([^}]*)\}").unwrap());
static FORMAT_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\\(?:textbf|textit|textsc|textsf|texttt|emph|verb)\{([^}]*)\}").unwrap()
});
static MATH_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\$[^$]*\$").unwrap());
static COMMAND_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\\[a-zA-Z]+\s?").unwrap());
static SPACE_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+").unwrap());

/// How far past `\begin{...}` the matching `\end` may start.
const FLOAT_BODY_MAX: usize = 2000;

fn within(s: &str, min: usize, max: usize) -> bool {
    let n = s.chars().count();
    n >= min && n <= max
}

/// Titles are display text: keep formatted text, drop short math spans and
/// bare commands, collapse whitespace.
fn clean_title(raw: &str) -> String {
    let s = FORMAT_RE.replace_all(raw, "$1");
    let s = MATH_RE.replace_all(&s, |c: &regex::Captures| {
        if c[0].chars().count() <= 82 {
            String::new()
        } else {
            c[0].to_string()
        }
    });
    let s = COMMAND_RE.replace_all(&s, "");
    let s = s.replace(['{', '}'], "");
    SPACE_RE.replace_all(&s, " ").trim().to_string()
}

struct Raw {
    offset: usize,
    kind: OutlineKind,
    level: Option<u8>,
    title: String,
    detail: Option<String>,
}

/// Each float: its span in `stripped` and the env name.
fn floats(stripped: &str) -> Vec<(usize, usize, &'static str)> {
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(m) = FLOAT_BEGIN_RE.captures_at(stripped, from) {
        let whole = m.get(0).unwrap();
        let env: &'static str = if &m[1] == "figure" { "figure" } else { "table" };
        let body_start = whole.end();
        let needle = format!("\\end{{{}", env);
        let end = stripped[body_start..]
            .match_indices(&needle)
            .take_while(|(i, _)| *i <= FLOAT_BODY_MAX)
            .find_map(|(i, _)| {
                let after = body_start + i + needle.len();
                let rest = &stripped[after..];
                if rest.starts_with('}') {
                    Some(after + 1)
                } else if rest.starts_with("*}") {
                    Some(after + 2)
                } else {
                    None
                }
            });
        match end {
            Some(e) => {
                out.push((whole.start(), e, env));
                from = e;
            }
            None => from = whole.end(),
        }
    }
    out
}

pub fn outline(text: &str) -> Outline {
    let stripped = strip_comments(text);
    let lines = Lines::new(&stripped);
    let mut raws: Vec<Raw> = Vec::new();

    for m in SECTION_RE.captures_iter(&stripped) {
        let body = &m[2];
        if !within(body, 1, 200) {
            continue;
        }
        let level = match &m[1] {
            "chapter" => 0,
            "section" => 1,
            "subsection" => 2,
            "subsubsection" => 3,
            _ => 4,
        };
        let title = clean_title(body);
        raws.push(Raw {
            offset: m.get(0).unwrap().start(),
            kind: OutlineKind::Section,
            level: Some(level),
            title: if title.is_empty() {
                "(untitled)".into()
            } else {
                title
            },
            detail: None,
        });
    }

    let spans = floats(&stripped);
    // Float-body labels are claimed by their float: listing them again would
    // double the same key.
    let in_float = |off: usize| spans.iter().any(|&(a, b, _)| off > a && off < b);

    for m in LABEL_RE.captures_iter(&stripped) {
        let key = m[1].trim();
        let off = m.get(0).unwrap().start();
        if !within(&m[1], 1, 120) || in_float(off) {
            continue;
        }
        raws.push(Raw {
            offset: off,
            kind: OutlineKind::Label,
            level: None,
            title: if key.is_empty() {
                "(unlabeled)".into()
            } else {
                key.into()
            },
            detail: Some(key.into()),
        });
    }

    for &(a, b, env) in &spans {
        let body = &stripped[a..b];
        let cap = CAPTION_RE
            .captures(body)
            .map(|c| c[1].to_string())
            .filter(|c| within(c, 0, 160))
            .unwrap_or_default();
        let gfx = GRAPHICS_RE
            .captures(body)
            .map(|c| c[1].trim().to_string())
            .filter(|g| within(g, 1, 160))
            .unwrap_or_default();
        let mut title = clean_title(cap.trim());
        if title.is_empty() {
            title = if gfx.is_empty() {
                format!("({})", env)
            } else {
                base_name(&gfx).to_string()
            };
        }
        raws.push(Raw {
            offset: a,
            kind: if env == "figure" {
                OutlineKind::Figure
            } else {
                OutlineKind::Table
            },
            level: None,
            title,
            detail: if gfx.is_empty() { None } else { Some(gfx) },
        });
    }

    for m in INPUT_RE.captures_iter(&stripped) {
        if !within(&m[1], 1, 160) {
            continue;
        }
        let rel = m[1].trim();
        raws.push(Raw {
            offset: m.get(0).unwrap().start(),
            kind: OutlineKind::Input,
            level: None,
            title: base_name(rel).to_string(),
            detail: Some(rel.to_string()),
        });
    }

    raws.sort_by_key(|r| r.offset);
    let total = raws.len();
    let mut entries = Vec::with_capacity(total.min(MAX_OUTLINE_ENTRIES));
    let mut current = 1u8;
    for r in raws {
        if let Some(l) = r.level {
            current = l;
        }
        if entries.len() == MAX_OUTLINE_ENTRIES {
            break;
        }
        entries.push(OutlineEntry {
            level: r.level.unwrap_or(current),
            title: r.title,
            line: lines.line_of(r.offset),
            kind: r.kind,
            detail: r.detail,
        });
    }
    Outline {
        truncated: total - entries.len(),
        entries,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use OutlineKind::*;

    fn e(
        level: u8,
        title: &str,
        line: u32,
        kind: OutlineKind,
        detail: Option<&str>,
    ) -> OutlineEntry {
        OutlineEntry {
            level,
            title: title.into(),
            line,
            kind,
            detail: detail.map(Into::into),
        }
    }

    fn kinds(o: &Outline) -> Vec<OutlineKind> {
        o.entries.iter().map(|e| e.kind).collect()
    }

    #[test]
    fn section_hierarchy_with_line_numbers() {
        let text = "\\documentclass{article}\n\\begin{document}\n\\section{Intro}\nHi\n\\subsection{Bits}\nX\n\\end{document}\n";
        assert_eq!(
            outline(text).entries,
            vec![
                e(1, "Intro", 3, Section, None),
                e(2, "Bits", 5, Section, None)
            ]
        );
    }

    #[test]
    fn commented_sections_are_ignored() {
        assert_eq!(
            outline("% \\section{Fake}\n\\section{Real}\n").entries,
            vec![e(1, "Real", 2, Section, None)]
        );
        // Inline comments too, not only whole-line ones.
        assert_eq!(
            outline("x % \\section{Fake}\n\\section{Real} % \\label{no}\n").entries,
            vec![e(1, "Real", 2, Section, None)]
        );
    }

    #[test]
    fn titles_are_cleaned() {
        assert_eq!(
            outline("\\section{Intro \\textbf{bold}}\n").entries[0].title,
            "Intro bold"
        );
        assert_eq!(
            outline("\\section{Energy $E=mc^2$ law}\n").entries[0].title,
            "Energy law"
        );
        assert_eq!(outline("\\section{}\n").entries.len(), 0);
        assert_eq!(
            outline("\\section{\\LaTeX}\n").entries[0].title,
            "(untitled)"
        );
    }

    #[test]
    fn labels_in_document_order() {
        let text = "\\section{Intro}\n\\label{sec:intro}\nText \\ref{fig:a}.\n";
        assert_eq!(
            outline(text).entries,
            vec![
                e(1, "Intro", 1, Section, None),
                e(1, "sec:intro", 2, Label, Some("sec:intro")),
            ]
        );
    }

    #[test]
    fn floats_prefer_captions_then_file_name() {
        let text = "\\begin{figure}[h]\n\\centering\n\\includegraphics[width=0.5\\textwidth]{figs/diagram}\n\\caption{A diagram.}\n\\label{fig:diagram}\n\\end{figure}\n";
        assert_eq!(
            outline(text).entries,
            vec![e(1, "A diagram.", 1, Figure, Some("figs/diagram"))]
        );
        let no_cap = "\\begin{figure}\n\\includegraphics{figs/photo}\n\\end{figure}\n";
        assert_eq!(outline(no_cap).entries[0].title, "photo");
        let bare = "\\begin{table*}\nx\n\\end{table*}\n";
        assert_eq!(outline(bare).entries, vec![e(1, "(table)", 1, Table, None)]);
    }

    #[test]
    fn unterminated_float_is_not_a_row() {
        assert!(outline("\\begin{figure}\n\\caption{C}\n")
            .entries
            .is_empty());
        let far = format!(
            "\\begin{{figure}}{}\\end{{figure}}",
            "x".repeat(FLOAT_BODY_MAX + 1)
        );
        assert!(outline(&far).entries.is_empty());
    }

    #[test]
    fn input_boundaries_with_paths() {
        let text = "\\section{A}\n\\input{chapters/background}\n\\include{ch/method}\n";
        assert_eq!(
            outline(text).entries,
            vec![
                e(1, "A", 1, Section, None),
                e(1, "background", 2, Input, Some("chapters/background")),
                e(1, "method", 3, Input, Some("ch/method")),
            ]
        );
    }

    #[test]
    fn markers_take_the_current_level() {
        let text = "\\section{A}\n\\label{a}\n\\subsection{B}\n\\label{b}\n";
        let levels: Vec<(OutlineKind, u8)> = outline(text)
            .entries
            .iter()
            .map(|e| (e.kind, e.level))
            .collect();
        assert_eq!(
            levels,
            vec![(Section, 1), (Label, 1), (Section, 2), (Label, 2)]
        );
    }

    #[test]
    fn float_internal_and_commented_labels_do_not_double_list() {
        let text =
            "% \\label{fake}\n\\begin{figure}\n\\caption{C}\n\\label{fig:c}\n\\end{figure}\n";
        assert_eq!(kinds(&outline(text)), vec![Figure]);
    }

    #[test]
    fn all_kinds_interleave_in_document_order() {
        let text = "\\section{A}\n\\label{a}\n\\begin{figure}\n\\caption{C}\n\\end{figure}\n\\input{ch/b}\n";
        assert_eq!(kinds(&outline(text)), vec![Section, Label, Figure, Input]);
    }

    #[test]
    fn caps_at_max_and_counts_the_rest() {
        let mut text = String::from("\\documentclass{article}\n\\begin{document}\n");
        for i in 0..3000 {
            text.push_str(&format!("\\section{{S{i}}} \\label{{s:{i}}}\n"));
        }
        let o = outline(&text);
        assert_eq!(o.entries.len(), MAX_OUTLINE_ENTRIES);
        assert_eq!(o.truncated, 6000 - MAX_OUTLINE_ENTRIES);
    }

    #[test]
    fn serializes_without_empty_detail() {
        let json = serde_json::to_string(&outline("\\section{A}\n")).unwrap();
        assert!(!json.contains("detail"));
        assert!(json.contains("\"kind\":\"section\""));
    }
}
