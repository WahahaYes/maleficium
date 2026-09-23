//! Replace across the project: the plan a replace would carry out, computed
//! from the index. Every match of the query is replaced (the search cap does
//! not apply); a regex replacement expands `$1`/`${name}` groups, a literal
//! one is inserted as written. Applying the plan is the caller's job.

use serde::Serialize;
use ts_rs::TS;

use crate::search::{compile, hit_parts, line_starts, matches, ordered, Query};
use crate::{ProjectIndex, Source};

/// One replacement as the preview shows it: the line before and after.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct ReplaceHunk {
    pub line: u32,
    /// UTF-16 column and length of the match on the line before.
    pub col: u32,
    pub len: u32,
    pub before: String,
    pub after: String,
}

/// What replacing would do to one file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct ReplaceFile {
    pub rel: String,
    pub source: Source,
    /// Revision of the text the plan was made from.
    pub revision: String,
    pub replacements: u32,
    /// The first hunks of this file (see `ReplacePreview.hunksTruncated`).
    pub hunks: Vec<ReplaceHunk>,
}

/// A replace plan as the caller sees it before applying: every file it
/// would change, the first hunks, and the token that applies it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct ReplacePreview {
    /// Pass back to apply; refused once any listed file has changed.
    pub token: String,
    pub files: Vec<ReplaceFile>,
    pub replacements: u32,
    /// Hunks not carried in `files` (the replacements still happen).
    pub hunks_truncated: u32,
}

/// Text a caller applies itself (an open editor buffer).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema, TS)]
pub struct BufferEdit {
    pub rel: String,
    pub text: String,
}

/// What applying a plan did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct ReplaceApplied {
    /// History batch holding every file's prior content: undo restores it.
    pub batch: String,
    /// Files written on disk.
    pub written: Vec<String>,
    /// New text for files the caller keeps open, not written here.
    pub edits: Vec<BufferEdit>,
    pub replacements: u32,
}

/// A planned file with the text it would hold.
#[derive(Debug, Clone)]
pub struct Planned {
    pub file: ReplaceFile,
    pub before: String,
    pub after: String,
}

/// Hunks a preview carries across all files; the rest are counted.
pub const PREVIEW_HUNKS_MAX: usize = 2000;

/// The replacement text for one match.
fn expanded(
    re: &regex::Regex,
    q: &Query,
    text: &str,
    (s, e): (usize, usize),
    repl: &str,
) -> String {
    if !q.regex {
        return repl.to_string();
    }
    let mut out = String::new();
    if let Some(c) = re
        .captures_at(text, s)
        .filter(|c| c.get(0).map(|m| (m.start(), m.end())) == Some((s, e)))
    {
        c.expand(repl, &mut out);
    } else {
        out.push_str(repl);
    }
    out
}

/// Plan replacing every match of `q` with `replacement`, in search order.
/// Also returns how many hunks were left out of the preview.
pub fn plan(
    index: &ProjectIndex,
    q: &Query,
    replacement: &str,
    main: Option<&str>,
) -> Result<(Vec<Planned>, u32), String> {
    let re = compile(q)?;
    let mut out = Vec::new();
    let mut shown = 0usize;
    let mut left_out = 0u32;
    for rel in ordered(index, main) {
        let f = index.get(rel).expect("ordered paths are indexed");
        let Some(text) = f.text else { continue };
        let found = matches(&re, text, q.whole_word);
        if found.is_empty() {
            continue;
        }
        let starts = line_starts(text);
        let mut after = String::with_capacity(text.len());
        let mut last = 0;
        let mut hunks = Vec::new();
        for &(s, e) in &found {
            let r = expanded(&re, q, text, (s, e), replacement);
            after.push_str(&text[last..s]);
            after.push_str(&r);
            last = e;
            if shown < PREVIEW_HUNKS_MAX {
                let (line, col, len, line_text, ms, me) = hit_parts(text, &starts, (s, e));
                let mut changed = String::with_capacity(line_text.len() + r.len());
                changed.push_str(&line_text[..ms]);
                changed.push_str(&r);
                changed.push_str(&line_text[me..]);
                hunks.push(ReplaceHunk {
                    line,
                    col,
                    len,
                    before: line_text.to_string(),
                    after: changed,
                });
                shown += 1;
            } else {
                left_out += 1;
            }
        }
        after.push_str(&text[last..]);
        out.push(Planned {
            file: ReplaceFile {
                rel: rel.to_string(),
                source: f.source,
                revision: f.revision.to_string(),
                replacements: found.len() as u32,
                hunks,
            },
            before: text.to_string(),
            after,
        });
    }
    Ok((out, left_out))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Disk;

    fn index(files: &[(&str, &str)]) -> ProjectIndex {
        let mut i = ProjectIndex::new();
        for (r, t) in files {
            i.set_disk(r, Disk::Text(t.to_string()));
        }
        i
    }

    fn q(p: &str) -> Query {
        Query {
            pattern: p.into(),
            ..Default::default()
        }
    }

    #[test]
    fn replaces_every_match_and_previews_lines() {
        let i = index(&[
            ("a.tex", "Foo foo\nfood\n"),
            ("b.tex", "none"),
            ("c.tex", "foo"),
        ]);
        let (p, left) = plan(
            &i,
            &Query {
                whole_word: true,
                ..q("foo")
            },
            "bar",
            None,
        )
        .unwrap();
        assert_eq!(left, 0);
        let got: Vec<(&str, &str, u32)> = p
            .iter()
            .map(|f| (f.file.rel.as_str(), f.after.as_str(), f.file.replacements))
            .collect();
        assert_eq!(got, [("a.tex", "bar bar\nfood\n", 2), ("c.tex", "bar", 1)]);
        let h = &p[0].file.hunks[1];
        assert_eq!(
            (h.line, h.col, h.before.as_str(), h.after.as_str()),
            (1, 4, "Foo foo", "Foo bar")
        );
        assert_eq!(p[0].before, "Foo foo\nfood\n");
    }

    #[test]
    fn regex_replacements_expand_groups_and_literals_do_not() {
        let i = index(&[("a.tex", "\\ref{sec:a} \\ref{sec:b}")]);
        let re = Query {
            regex: true,
            case_sensitive: true,
            ..q(r"\\ref\{(sec:\w+)\}")
        };
        let (p, _) = plan(&i, &re, r"\cref{$1}", None).unwrap();
        assert_eq!(p[0].after, "\\cref{sec:a} \\cref{sec:b}");
        let (p, _) = plan(&i, &q("sec:a"), "$1", None).unwrap();
        assert_eq!(p[0].after, "\\ref{$1} \\ref{sec:b}");
    }

    #[test]
    fn plans_from_buffer_text_when_laid_over() {
        let mut i = index(&[("a.tex", "disk x")]);
        i.set_overlay("a.tex", Some("buffer x".into()));
        let (p, _) = plan(&i, &q("x"), "y", None).unwrap();
        assert_eq!(
            (p[0].file.source, p[0].after.as_str()),
            (Source::Buffer, "buffer y")
        );
    }

    #[test]
    fn caps_preview_hunks_but_not_replacements() {
        let text = "x ".repeat(PREVIEW_HUNKS_MAX + 5);
        let i = index(&[("a.tex", text.as_str())]);
        let (p, left) = plan(&i, &q("x"), "y", None).unwrap();
        assert_eq!(left, 5);
        assert_eq!(p[0].file.hunks.len(), PREVIEW_HUNKS_MAX);
        assert!(!p[0].after.contains('x'));
    }
}
