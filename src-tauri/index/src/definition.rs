//! Go-to-definition over the index: what a position on a line refers to
//! (a label key, a citation key, a macro, an input target), and where that
//! is defined, with a one-line summary for hover.

use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::graph::{dir_of, resolve_input};
use crate::ProjectIndex;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, TS)]
#[serde(rename_all = "kebab-case")]
pub enum RefKind {
    /// A `\ref`-family or `\label` key.
    Label,
    /// A `\cite`-family key.
    Citation,
    /// A control sequence (`\name`).
    Macro,
    /// An `\input` / `\include` / `\subfile` target.
    Input,
}

/// What sits under a position: its kind and key as written (a macro keeps
/// its backslash; an input keeps its command for name resolution).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, TS)]
pub struct RefAt {
    pub kind: RefKind,
    pub key: String,
    /// `input`, `include` or `subfile` for an input target.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub command: Option<String>,
}

/// One place a reference is defined.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema, TS)]
pub struct Definition {
    pub rel: String,
    pub line: u32,
    /// For hover: the defining line, a bib entry summary, the macro
    /// definition, or the input file's first line.
    pub summary: String,
}

/// A lookup's answer: what was looked up, and every definition found
/// (several for a duplicate key; none for an undefined one).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema, TS)]
pub struct Lookup {
    #[serde(rename = "ref")]
    #[ts(rename = "ref")]
    pub reference: RefAt,
    pub definitions: Vec<Definition>,
}

static KEYED_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"\\(label|ref|eqref|pageref|autoref|nameref|vref|cref|Cref|cpageref|Cpageref|[a-zA-Z]*cite[a-zA-Z]*|input|include|subfile)\*?(?:\s*\[[^\]]*\]){0,2}\s*\{([^}]*)\}",
    )
    .unwrap()
});
static MACRO_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\\[A-Za-z@]+").unwrap());

/// Byte offset of UTF-16 column `col` in `line` (clamped to the end).
fn byte_at(line: &str, col: u32) -> usize {
    let mut units = 0u32;
    for (i, c) in line.char_indices() {
        if units >= col {
            return i;
        }
        units += c.len_utf16() as u32;
    }
    line.len()
}

/// What the text at UTF-16 column `col` of `line` refers to, if anything.
/// Inside a key list (`\cite{a,b}`) the key under the column is taken.
pub fn ref_at(line: &str, col: u32) -> Option<RefAt> {
    let at = byte_at(line, col);
    for m in KEYED_RE.captures_iter(line) {
        let whole = m.get(0).unwrap();
        if at < whole.start() || at > whole.end() {
            continue;
        }
        let cmd = &m[1];
        let list = m.get(2).unwrap();
        let (kind, command) = match cmd {
            "input" | "include" | "subfile" => (RefKind::Input, Some(cmd.to_string())),
            "label" => (RefKind::Label, None),
            c if c.contains("cite") => (RefKind::Citation, None),
            _ => (RefKind::Label, None),
        };
        if kind == RefKind::Input {
            let key = list.as_str().trim();
            return (!key.is_empty()).then(|| RefAt {
                kind,
                key: key.into(),
                command,
            });
        }
        // The key under the column, else the first key of the list.
        let mut offset = list.start();
        let mut first = None;
        for part in list.as_str().split(',') {
            let key = part.trim();
            let (s, e) = (offset, offset + part.len());
            offset = e + 1;
            if key.is_empty() {
                continue;
            }
            first.get_or_insert(key);
            if at >= s && at <= e {
                return Some(RefAt {
                    kind,
                    key: key.into(),
                    command: None,
                });
            }
        }
        return first.map(|k| RefAt {
            kind,
            key: k.into(),
            command: None,
        });
    }
    MACRO_RE
        .find_iter(line)
        .find(|m| at >= m.start() && at <= m.end())
        .map(|m| RefAt {
            kind: RefKind::Macro,
            key: m.as_str().into(),
            command: None,
        })
}

fn line_of(text: &str, line: u32) -> &str {
    text.lines()
        .nth(line.saturating_sub(1) as usize)
        .unwrap_or("")
        .trim()
}

static FIELD_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)\b(author|title|year)\s*=\s*(?:\{((?:[^{}]|\{[^{}]*\})*)\}|"([^"]*)"|(\d+))"#)
        .unwrap()
});

/// `Author — Title (year)` from the entry starting at `line` of a bib text.
pub fn bib_summary(text: &str, line: u32) -> String {
    let start = text
        .split_inclusive('\n')
        .take(line.saturating_sub(1) as usize)
        .map(str::len)
        .sum::<usize>();
    let rest = &text[start.min(text.len())..];
    let end = rest[1.min(rest.len())..]
        .find("\n@")
        .map(|i| i + 1)
        .unwrap_or(rest.len());
    let entry = &rest[..end];
    let (mut author, mut title, mut year) = (None, None, None);
    for c in FIELD_RE.captures_iter(entry) {
        let v = c
            .get(2)
            .or(c.get(3))
            .or(c.get(4))
            .map(|m| m.as_str().split_whitespace().collect::<Vec<_>>().join(" "));
        match c[1].to_ascii_lowercase().as_str() {
            "author" => author = author.or(v),
            "title" => title = title.or(v),
            _ => year = year.or(v),
        }
    }
    let mut out = String::new();
    if let Some(a) = author {
        out.push_str(&a);
        out.push_str(" — ");
    }
    out.push_str(&title.unwrap_or_else(|| line_of(text, line).to_string()));
    if let Some(y) = year {
        out.push_str(&format!(" ({y})"));
    }
    out
}

/// Where `r` is defined. `main` scopes input resolution to its directory
/// (the engine's); without one, inputs resolve from the root.
pub fn lookup(index: &ProjectIndex, r: &RefAt, main: Option<&str>) -> Lookup {
    let maps = index.maps();
    let text_of = |rel: &str| index.get(rel).and_then(|f| f.text).unwrap_or("");
    let definitions = match r.kind {
        RefKind::Label => maps
            .labels
            .get(&r.key)
            .map(|v| {
                v.iter()
                    .map(|l| Definition {
                        rel: l.rel.clone(),
                        line: l.line,
                        summary: line_of(text_of(&l.rel), l.line).to_string(),
                    })
                    .collect()
            })
            .unwrap_or_default(),
        RefKind::Citation => maps
            .bib_entries
            .get(&r.key)
            .map(|v| {
                v.iter()
                    .map(|l| Definition {
                        rel: l.rel.clone(),
                        line: l.line,
                        summary: bib_summary(text_of(&l.rel), l.line),
                    })
                    .collect()
            })
            .unwrap_or_default(),
        RefKind::Macro => maps
            .macros
            .get(&r.key)
            .map(|v| {
                v.iter()
                    .map(|d| Definition {
                        rel: d.rel.clone(),
                        line: d.line,
                        summary: match d.params {
                            Some(n) if n > 0 => {
                                format!("\\{}{{{}}}[{}]{{{}}}", d.command, r.key, n, d.body)
                            }
                            _ => format!("\\{}{{{}}}{{{}}}", d.command, r.key, d.body),
                        },
                    })
                    .collect()
            })
            .unwrap_or_default(),
        RefKind::Input => {
            let dir = main.map(dir_of).unwrap_or("");
            let cmd = r.command.as_deref().unwrap_or("input");
            resolve_input(dir, cmd, &r.key)
                .filter(|rel| index.get(rel).is_some())
                .map(|rel| {
                    let summary = text_of(&rel)
                        .lines()
                        .find(|l| !l.trim().is_empty())
                        .unwrap_or("")
                        .trim()
                        .to_string();
                    vec![Definition {
                        rel,
                        line: 1,
                        summary,
                    }]
                })
                .unwrap_or_default()
        }
    };
    Lookup {
        reference: r.clone(),
        definitions,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Disk;

    fn r(kind: RefKind, key: &str) -> Option<RefAt> {
        Some(RefAt {
            kind,
            key: key.into(),
            command: None,
        })
    }

    #[test]
    fn finds_what_sits_under_a_column() {
        let line = r"See \cref{sec:a, fig:b} and \citep[p.~3]{knuth, lamport} via \R and \input{ch/intro}.";
        let col = |needle: &str| line.find(needle).unwrap() as u32;
        assert_eq!(ref_at(line, col("sec:a")), r(RefKind::Label, "sec:a"));
        assert_eq!(ref_at(line, col("fig:b") + 2), r(RefKind::Label, "fig:b"));
        assert_eq!(ref_at(line, col(r"\cref")), r(RefKind::Label, "sec:a"));
        assert_eq!(
            ref_at(line, col("lamport")),
            r(RefKind::Citation, "lamport")
        );
        assert_eq!(ref_at(line, col(r"\R") + 1), r(RefKind::Macro, r"\R"));
        assert_eq!(
            ref_at(line, col("ch/intro")),
            Some(RefAt {
                kind: RefKind::Input,
                key: "ch/intro".into(),
                command: Some("input".into())
            })
        );
        assert_eq!(ref_at(line, 1), None);
    }

    #[test]
    fn columns_are_utf16() {
        let line = "é😀 \\ref{x}";
        assert_eq!(ref_at(line, 4), r(RefKind::Label, "x"));
    }

    #[test]
    fn looks_up_every_kind_across_the_project() {
        let mut i = ProjectIndex::new();
        let put = |i: &mut ProjectIndex, p: &str, t: &str| i.set_disk(p, Disk::Text(t.into()));
        put(
            &mut i,
            "p/main.tex",
            "\\newcommand{\\R}{\\mathbb{R}}\n\\section{A}\\label{sec:a}\n\\input{ch/intro}",
        );
        put(&mut i, "p/ch/intro.tex", "\n\\chapter{Intro}\n");
        put(&mut i, "p/ch/dup.tex", "\\label{sec:a}");
        put(&mut i, "p/refs.bib", "@book{knuth,\n  author = {Donald E. Knuth},\n  title = {The {\\TeX}book},\n  year = 1984\n}\n@misc{other,\n title={X}}\n");
        let main = Some("p/main.tex");
        let l = lookup(
            &i,
            &RefAt {
                kind: RefKind::Label,
                key: "sec:a".into(),
                command: None,
            },
            main,
        );
        assert_eq!(l.definitions.len(), 2, "a duplicate label lists both");
        assert_eq!(
            (l.definitions[0].rel.as_str(), l.definitions[0].line),
            ("p/ch/dup.tex", 1)
        );
        assert_eq!(l.definitions[1].summary, "\\section{A}\\label{sec:a}");
        let c = lookup(
            &i,
            &RefAt {
                kind: RefKind::Citation,
                key: "knuth".into(),
                command: None,
            },
            main,
        );
        assert_eq!(
            c.definitions[0].summary,
            "Donald E. Knuth — The {\\TeX}book (1984)"
        );
        let m = lookup(
            &i,
            &RefAt {
                kind: RefKind::Macro,
                key: "\\R".into(),
                command: None,
            },
            main,
        );
        assert_eq!(m.definitions[0].summary, "\\newcommand{\\R}{\\mathbb{R}}");
        let inp = RefAt {
            kind: RefKind::Input,
            key: "ch/intro".into(),
            command: Some("input".into()),
        };
        let d = lookup(&i, &inp, main);
        assert_eq!(
            (
                d.definitions[0].rel.as_str(),
                d.definitions[0].summary.as_str()
            ),
            ("p/ch/intro.tex", "\\chapter{Intro}")
        );
        assert!(
            lookup(&i, &inp, None).definitions.is_empty(),
            "inputs resolve from the main file's directory"
        );
        let none = lookup(
            &i,
            &RefAt {
                kind: RefKind::Label,
                key: "nope".into(),
                command: None,
            },
            main,
        );
        assert!(none.definitions.is_empty());
    }

    #[test]
    fn buffers_define_too() {
        let mut i = ProjectIndex::new();
        i.set_disk("a.tex", Disk::Text("x".into()));
        i.set_overlay("a.tex", Some("\\label{fresh}".into()));
        let l = lookup(
            &i,
            &RefAt {
                kind: RefKind::Label,
                key: "fresh".into(),
                command: None,
            },
            None,
        );
        assert_eq!(l.definitions[0].rel, "a.tex");
    }
}
