//! Cross-reference symbols of one text, as written: label definitions, ref
//! and cite uses, input edges, bibliography resources. Resolving them across
//! files is the caller's job (it owns the file graph).

use std::sync::LazyLock;

use regex::Regex;
use serde::Serialize;

use crate::text::{strip_comments, Lines};

/// A key (or path, as written) and the 1-based line it appears on.
#[derive(Debug, Clone, PartialEq, Serialize, schemars::JsonSchema)]
pub struct KeyAt {
    pub key: String,
    pub line: u32,
}

/// One `\input` / `\include` / `\subfile` edge, target as written.
#[derive(Debug, Clone, PartialEq, Serialize, schemars::JsonSchema)]
pub struct InputAt {
    pub command: String,
    pub target: String,
    pub line: u32,
}

/// A package (`\usepackage`, `\RequirePackage`) or class (`\documentclass`,
/// `\LoadClass`) the text loads, with its options as written.
#[derive(Debug, Clone, PartialEq, Serialize, schemars::JsonSchema)]
pub struct PackageAt {
    pub name: String,
    pub class: bool,
    pub options: Option<String>,
    pub line: u32,
}

/// A macro definition (`\newcommand`, `\def`, `\DeclareMathOperator`,
/// `\NewDocumentCommand`, …): name with its backslash, the defining command,
/// the parameter count where the form states one, and the body as written
/// (capped at `MACRO_BODY_MAX` bytes).
#[derive(Debug, Clone, PartialEq, Serialize, schemars::JsonSchema)]
pub struct MacroAt {
    pub name: String,
    pub command: String,
    pub params: Option<u8>,
    pub body: String,
    pub line: u32,
}

/// Longest macro body kept, in bytes.
pub const MACRO_BODY_MAX: usize = 400;

#[derive(Debug, Clone, Default, PartialEq, Serialize, schemars::JsonSchema)]
pub struct Symbols {
    pub labels: Vec<KeyAt>,
    pub refs: Vec<KeyAt>,
    pub cites: Vec<KeyAt>,
    pub inputs: Vec<InputAt>,
    /// Bibliography resources as written (`\bibliography` entries get `.bib`).
    pub bibliographies: Vec<KeyAt>,
    /// Packages and classes, one row per name.
    pub packages: Vec<PackageAt>,
    /// Font names given to fontspec (`\setmainfont`, `\newfontfamily`, …).
    pub fonts: Vec<KeyAt>,
    /// `\write18` shell escapes.
    pub shell_escapes: Vec<KeyAt>,
    /// Macro definitions.
    pub macros: Vec<MacroAt>,
}

static LABEL_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\\label\{([^}]*)\}").unwrap());
static REF_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"\\(?:ref|eqref|pageref|autoref|nameref|vref|cref|Cref|cpageref|Cpageref)\*?\{([^}]*)\}",
    )
    .unwrap()
});
// Any `\...cite...` command (natbib, biblatex, plain) with up to two optional
// arguments before the key list.
static CITE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\\[a-zA-Z]*cite[a-zA-Z]*\*?(?:\s*\[[^\]]*\]){0,2}\s*\{([^}]*)\}").unwrap()
});
static INPUT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\\(input|include|subfile)\{([^}]*)\}").unwrap());
static BIBLIOGRAPHY_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\\bibliography\{([^}]*)\}").unwrap());
static ADDBIB_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\\addbibresource(?:\[[^\]]*\])?\{([^}]*)\}").unwrap());
static PACKAGE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"\\(usepackage|RequirePackage|documentclass|LoadClass)\s*(?:\[([^\]]*)\])?\s*\{([^}]*)\}",
    )
    .unwrap()
});
static FONT_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"\\(?:setmainfont|setsansfont|setmonofont|setmathfont|fontspec|(?:newfontfamily|newfontface)\s*\\[A-Za-z@]+)\s*(?:\[[^\]]*\])?\s*\{([^}]*)\}",
    )
    .unwrap()
});
static WRITE18_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\\write18\b").unwrap());
// The head of a macro definition: the defining command, then the name
// braced or bare. The rest (parameters, body) is read by brace matching.
static MACRO_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"\\(newcommand|renewcommand|providecommand|DeclareRobustCommand|DeclareMathOperator|NewDocumentCommand|RenewDocumentCommand|ProvideDocumentCommand|DeclareDocumentCommand|def|gdef|edef|xdef)\b\*?\s*(?:\{\s*(\\[A-Za-z@]+|\\.)\s*\}|(\\[A-Za-z@]+|\\.))",
    )
    .unwrap()
});
static BIB_ENTRY_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"@([a-zA-Z]+)\s*[{(]\s*([^,\s{}()]+)\s*,").unwrap());

/// Comma-separated keys of one match, trimmed, empties and `*` dropped.
fn keys(list: &str) -> impl Iterator<Item = &str> {
    list.split(',')
        .map(str::trim)
        .filter(|k| !k.is_empty() && *k != "*")
}

pub fn symbols(text: &str) -> Symbols {
    let stripped = strip_comments(text);
    let lines = Lines::new(&stripped);
    let at = |m: &regex::Captures| lines.line_of(m.get(0).unwrap().start());
    let mut s = Symbols::default();
    for m in LABEL_RE.captures_iter(&stripped) {
        let key = m[1].trim();
        if !key.is_empty() {
            s.labels.push(KeyAt {
                key: key.into(),
                line: at(&m),
            });
        }
    }
    for m in REF_RE.captures_iter(&stripped) {
        let line = at(&m);
        s.refs.extend(keys(&m[1]).map(|k| KeyAt {
            key: k.into(),
            line,
        }));
    }
    for m in CITE_RE.captures_iter(&stripped) {
        let line = at(&m);
        s.cites.extend(keys(&m[1]).map(|k| KeyAt {
            key: k.into(),
            line,
        }));
    }
    for m in INPUT_RE.captures_iter(&stripped) {
        let target = m[2].trim();
        if !target.is_empty() {
            s.inputs.push(InputAt {
                command: m[1].into(),
                target: target.into(),
                line: at(&m),
            });
        }
    }
    for m in BIBLIOGRAPHY_RE.captures_iter(&stripped) {
        let line = at(&m);
        s.bibliographies.extend(keys(&m[1]).map(|k| KeyAt {
            key: if k.ends_with(".bib") {
                k.into()
            } else {
                format!("{k}.bib")
            },
            line,
        }));
    }
    for m in ADDBIB_RE.captures_iter(&stripped) {
        let key = m[1].trim();
        if !key.is_empty() {
            s.bibliographies.push(KeyAt {
                key: key.into(),
                line: at(&m),
            });
        }
    }
    s.bibliographies.sort_by_key(|b| b.line);
    for m in PACKAGE_RE.captures_iter(&stripped) {
        let line = at(&m);
        let class = matches!(&m[1], "documentclass" | "LoadClass");
        let options = m.get(2).map(|o| o.as_str().trim().to_string());
        s.packages.extend(keys(&m[3]).map(|name| PackageAt {
            name: name.into(),
            class,
            options: options.clone(),
            line,
        }));
    }
    for m in FONT_RE.captures_iter(&stripped) {
        let key = m[1].trim();
        if !key.is_empty() {
            s.fonts.push(KeyAt {
                key: key.into(),
                line: at(&m),
            });
        }
    }
    for m in WRITE18_RE.find_iter(&stripped) {
        s.shell_escapes.push(KeyAt {
            key: String::from("\\write18"),
            line: lines.line_of(m.start()),
        });
    }
    for m in MACRO_RE.captures_iter(&stripped) {
        let name = m.get(2).or(m.get(3)).unwrap().as_str();
        let command = &m[1];
        if let Some((params, body)) = macro_rest(&stripped, command, m.get(0).unwrap().end()) {
            s.macros.push(MacroAt {
                name: name.into(),
                command: command.into(),
                params,
                body,
                line: at(&m),
            });
        }
    }
    s
}

/// Skip ASCII whitespace from byte `i`.
fn skip_ws(b: &[u8], mut i: usize) -> usize {
    while i < b.len() && b[i].is_ascii_whitespace() {
        i += 1;
    }
    i
}

/// A balanced `{…}` group starting at byte `i` (after whitespace): its inner
/// text and the byte after the closing brace.
fn brace_group(text: &str, i: usize) -> Option<(&str, usize)> {
    let b = text.as_bytes();
    let start = skip_ws(b, i);
    if b.get(start) != Some(&b'{') {
        return None;
    }
    let mut depth = 0usize;
    let mut j = start;
    while j < b.len() {
        match b[j] {
            b'\\' => j += 1,
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some((&text[start + 1..j], j + 1));
                }
            }
            _ => {}
        }
        j += 1;
    }
    None
}

/// A `[…]` group at byte `i` (after whitespace), unnested.
fn bracket_group(text: &str, i: usize) -> Option<(&str, usize)> {
    let b = text.as_bytes();
    let start = skip_ws(b, i);
    if b.get(start) != Some(&b'[') {
        return None;
    }
    let end = start + text[start..].find(']')?;
    Some((&text[start + 1..end], end + 1))
}

fn capped(body: &str) -> String {
    let body = body.trim();
    if body.len() <= MACRO_BODY_MAX {
        return body.to_string();
    }
    let mut end = MACRO_BODY_MAX;
    while !body.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &body[..end])
}

/// The parameters and body after a macro head ending at byte `i`.
fn macro_rest(text: &str, command: &str, i: usize) -> Option<(Option<u8>, String)> {
    match command {
        "def" | "gdef" | "edef" | "xdef" => {
            let open = i + text[i..].find('{')?;
            let params = text[i..open].matches('#').count() as u8;
            let (body, _) = brace_group(text, open)?;
            Some((Some(params), capped(body)))
        }
        "DeclareMathOperator" => brace_group(text, i).map(|(b, _)| (Some(0), capped(b))),
        "NewDocumentCommand"
        | "RenewDocumentCommand"
        | "ProvideDocumentCommand"
        | "DeclareDocumentCommand" => {
            let (_, after_spec) = brace_group(text, i)?;
            brace_group(text, after_spec).map(|(b, _)| (None, capped(b)))
        }
        _ => {
            let (params, i) = match bracket_group(text, i) {
                Some((n, j)) => (n.trim().parse::<u8>().ok(), j),
                None => (Some(0), i),
            };
            let i = bracket_group(text, i).map(|(_, j)| j).unwrap_or(i);
            brace_group(text, i).map(|(b, _)| (params, capped(b)))
        }
    }
}

/// Entry keys of a `.bib` text (`@comment`, `@string`, `@preamble` skipped).
pub fn bib_keys(text: &str) -> Vec<KeyAt> {
    let lines = Lines::new(text);
    BIB_ENTRY_RE
        .captures_iter(text)
        .filter(|m| {
            !matches!(
                m[1].to_ascii_lowercase().as_str(),
                "comment" | "string" | "preamble"
            )
        })
        .map(|m| KeyAt {
            key: m[2].into(),
            line: lines.line_of(m.get(0).unwrap().start()),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn k(key: &str, line: u32) -> KeyAt {
        KeyAt {
            key: key.into(),
            line,
        }
    }

    #[test]
    fn labels_refs_and_cites_with_lines() {
        let text = "\\section{A}\\label{sec:a}\nSee \\ref{sec:a} and \\cref{fig:x, tab:y}.\n\\citep[p.~3]{knuth, lamport} \\textcite{mitt}\n% \\cite{hidden}\n\\nocite{*}\n";
        let s = symbols(text);
        assert_eq!(s.labels, vec![k("sec:a", 1)]);
        assert_eq!(s.refs, vec![k("sec:a", 2), k("fig:x", 2), k("tab:y", 2)]);
        assert_eq!(s.cites, vec![k("knuth", 3), k("lamport", 3), k("mitt", 3)]);
    }

    #[test]
    fn float_labels_count_as_definitions() {
        let s = symbols("\\begin{figure}\n\\caption{C}\\label{fig:c}\n\\end{figure}\n");
        assert_eq!(s.labels, vec![k("fig:c", 2)]);
    }

    #[test]
    fn inputs_keep_command_and_target() {
        let s = symbols("\\input{chapters/a}\n\\include{b}\n\\subfile{c.tex}\n\\input{}\n");
        let got: Vec<(&str, &str, u32)> = s
            .inputs
            .iter()
            .map(|i| (i.command.as_str(), i.target.as_str(), i.line))
            .collect();
        assert_eq!(
            got,
            vec![
                ("input", "chapters/a", 1),
                ("include", "b", 2),
                ("subfile", "c.tex", 3)
            ]
        );
    }

    #[test]
    fn bibliographies_normalize_extension() {
        let s = symbols(
            "\\addbibresource[datatype=bibtex]{extra.bib}\n\\bibliography{refs,more.bib}\n",
        );
        assert_eq!(
            s.bibliographies,
            vec![k("extra.bib", 1), k("refs.bib", 2), k("more.bib", 2)]
        );
    }

    #[test]
    fn packages_classes_fonts_and_shell_escapes() {
        let s = symbols(
            "\\documentclass[11pt]{article}\n\\usepackage[backend=biber, style=alpha]{biblatex}\n\\usepackage{amsmath,  amssymb}\n% \\usepackage{hidden}\n\\RequirePackage{xcolor}\n\\setmainfont{DejaVu Sans}\n\\newfontfamily\\mono[Scale=0.9]{Fira Mono}\n\\immediate\\write18{ls}\n",
        );
        let got: Vec<(&str, bool, Option<&str>, u32)> = s
            .packages
            .iter()
            .map(|p| (p.name.as_str(), p.class, p.options.as_deref(), p.line))
            .collect();
        assert_eq!(
            got,
            vec![
                ("article", true, Some("11pt"), 1),
                ("biblatex", false, Some("backend=biber, style=alpha"), 2),
                ("amsmath", false, None, 3),
                ("amssymb", false, None, 3),
                ("xcolor", false, None, 5),
            ]
        );
        assert_eq!(s.fonts, vec![k("DejaVu Sans", 6), k("Fira Mono", 7)]);
        assert_eq!(s.shell_escapes, vec![k("\\write18", 8)]);
    }

    #[test]
    fn macro_definitions_in_every_form() {
        let s = symbols(concat!(
            "\\newcommand{\\R}{\\mathbb{R}}\n",
            "\\renewcommand*\\vec[1]{\\mathbf{#1}}\n",
            "\\newcommand{\\opt}[2][x]{#1+#2}\n",
            "\\DeclareMathOperator*{\\argmax}{arg\\,max}\n",
            "\\def\\pair#1#2{(#1, #2)}\n",
            "\\NewDocumentCommand{\\note}{m o}{\\textbf{#1}}\n",
            "% \\newcommand{\\hidden}{x}\n",
            "\\newcommand{\\nested}{a{b}c}\n",
        ));
        let got: Vec<(&str, &str, Option<u8>, &str, u32)> = s
            .macros
            .iter()
            .map(|m| {
                (
                    m.name.as_str(),
                    m.command.as_str(),
                    m.params,
                    m.body.as_str(),
                    m.line,
                )
            })
            .collect();
        assert_eq!(
            got,
            vec![
                ("\\R", "newcommand", Some(0), "\\mathbb{R}", 1),
                ("\\vec", "renewcommand", Some(1), "\\mathbf{#1}", 2),
                ("\\opt", "newcommand", Some(2), "#1+#2", 3),
                ("\\argmax", "DeclareMathOperator", Some(0), "arg\\,max", 4),
                ("\\pair", "def", Some(2), "(#1, #2)", 5),
                ("\\note", "NewDocumentCommand", None, "\\textbf{#1}", 6),
                ("\\nested", "newcommand", Some(0), "a{b}c", 8),
            ]
        );
    }

    #[test]
    fn long_macro_bodies_are_capped() {
        let body = "x".repeat(MACRO_BODY_MAX + 10);
        let s = symbols(&format!("\\newcommand{{\\big}}{{{body}}}"));
        assert!(s.macros[0].body.ends_with('…'));
    }

    #[test]
    fn bib_keys_skip_non_entries() {
        let bib = "@string{x = \"y\"}\n@Book{knuth1984,\n title={T}}\n@comment{no,}\n@article ( lamport ,\n}\n";
        assert_eq!(bib_keys(bib), vec![k("knuth1984", 2), k("lamport", 5)]);
    }
}
