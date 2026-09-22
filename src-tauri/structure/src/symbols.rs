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

#[derive(Debug, Clone, Default, PartialEq, Serialize, schemars::JsonSchema)]
pub struct Symbols {
    pub labels: Vec<KeyAt>,
    pub refs: Vec<KeyAt>,
    pub cites: Vec<KeyAt>,
    pub inputs: Vec<InputAt>,
    /// Bibliography resources as written (`\bibliography` entries get `.bib`).
    pub bibliographies: Vec<KeyAt>,
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
    s
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
    fn bib_keys_skip_non_entries() {
        let bib = "@string{x = \"y\"}\n@Book{knuth1984,\n title={T}}\n@comment{no,}\n@article ( lamport ,\n}\n";
        assert_eq!(bib_keys(bib), vec![k("knuth1984", 2), k("lamport", 5)]);
    }
}
