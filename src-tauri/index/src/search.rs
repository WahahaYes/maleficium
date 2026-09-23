//! Text search over the index, and the fuzzy file-name scorer behind the
//! file finder.
//!
//! Matches are found over each file's whole text (a pattern may span lines)
//! and reported by start line. Files reachable from the main file come
//! first, in graph order; the rest follow by path; hits within a file by
//! position. Columns are UTF-16 code units, the editor's offsets.

use regex::{Regex, RegexBuilder};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::graph::Document;
use crate::{ProjectIndex, Source};

/// A search as the user states it.
#[derive(
    Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, TS,
)]
#[serde(rename_all = "camelCase")]
pub struct Query {
    pub pattern: String,
    /// Treat `pattern` as a regular expression (else literal text).
    #[serde(default)]
    pub regex: bool,
    #[serde(default)]
    pub case_sensitive: bool,
    /// Only matches with no word character on either side.
    #[serde(default)]
    pub whole_word: bool,
}

/// One match: where it starts, and its line as context.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct Hit {
    /// 1-based line the match starts on.
    pub line: u32,
    /// UTF-16 column of the match start within `preview`'s line.
    pub col: u32,
    /// UTF-16 length of the match on that line (clipped at the line end).
    pub len: u32,
    /// The line the match starts on, without its newline, capped at
    /// `PREVIEW_MAX` bytes around the match.
    pub preview: String,
    /// UTF-16 offset of `preview` within its line (non-zero when clipped).
    pub preview_col: u32,
}

/// The hits in one file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct FileHits {
    pub rel: String,
    pub source: Source,
    pub revision: String,
    pub hits: Vec<Hit>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    pub files: Vec<FileHits>,
    /// Hits returned across `files`.
    pub hits: u32,
    /// Hits past the cap, not returned.
    pub truncated: u32,
    /// Files whose text was searched.
    pub searched: u32,
    /// Listed files with no text to search (binary, too large, …).
    pub unsearched: u32,
}

/// Longest preview line kept, in bytes.
pub const PREVIEW_MAX: usize = 240;

/// Compile a query into a regex.
pub fn compile(q: &Query) -> Result<Regex, String> {
    if q.pattern.is_empty() {
        return Err("empty search pattern".into());
    }
    let src = if q.regex {
        q.pattern.clone()
    } else {
        regex::escape(&q.pattern)
    };
    RegexBuilder::new(&src)
        .case_insensitive(!q.case_sensitive)
        .multi_line(true)
        .size_limit(1 << 22)
        .build()
        .map_err(|e| format!("invalid pattern: {}", e))
}

fn is_word(c: Option<char>) -> bool {
    c.is_some_and(|c| c.is_alphanumeric() || c == '_')
}

/// Byte ranges of every non-empty match of `re` in `text`, filtered to
/// whole words when asked.
pub fn matches(re: &Regex, text: &str, whole_word: bool) -> Vec<(usize, usize)> {
    re.find_iter(text)
        .filter(|m| m.start() < m.end())
        .filter(|m| {
            !whole_word
                || (!is_word(text[..m.start()].chars().next_back())
                    && !is_word(text[m.end()..].chars().next()))
        })
        .map(|m| (m.start(), m.end()))
        .collect()
}

fn utf16_len(s: &str) -> u32 {
    s.encode_utf16().count() as u32
}

fn floor_boundary(s: &str, mut i: usize) -> usize {
    while !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

/// Line starts (byte offsets) of `text`.
pub fn line_starts(text: &str) -> Vec<usize> {
    std::iter::once(0)
        .chain(text.match_indices('\n').map(|(i, _)| i + 1))
        .collect()
}

/// Where a match sits on its start line: 1-based line, UTF-16 column and
/// length, the line (without newline or CR), and the match's byte range
/// within that line (clipped at the line end).
pub fn hit_parts<'a>(
    text: &'a str,
    starts: &[usize],
    (s, e): (usize, usize),
) -> (u32, u32, u32, &'a str, usize, usize) {
    let li = starts.partition_point(|&x| x <= s) - 1;
    let ls = starts[li];
    let le = text[ls..].find('\n').map(|n| ls + n).unwrap_or(text.len());
    let le_trim = if le > ls && text.as_bytes()[le - 1] == b'\r' {
        le - 1
    } else {
        le
    };
    let line = &text[ls..le_trim];
    let ms = (s - ls).min(line.len());
    let me = (e.min(le_trim)).saturating_sub(ls).max(ms);
    (
        li as u32 + 1,
        utf16_len(&line[..ms]),
        utf16_len(&line[ms..me]),
        line,
        ms,
        me,
    )
}

fn hit_at(text: &str, starts: &[usize], m: (usize, usize)) -> Hit {
    let (line_no, col, len, line, ms, _) = hit_parts(text, starts, m);
    // Clip long lines to a window around the match.
    let (from, to) = if line.len() <= PREVIEW_MAX {
        (0, line.len())
    } else {
        let from = floor_boundary(line, ms.saturating_sub(PREVIEW_MAX / 3));
        let to = floor_boundary(line, (from + PREVIEW_MAX).min(line.len()));
        (from, to.max(from))
    };
    Hit {
        line: line_no,
        col,
        len,
        preview: line[from..to].to_string(),
        preview_col: utf16_len(&line[..from]),
    }
}

/// Paths in search order: the main file's document first, then the rest.
pub fn ordered<'a>(index: &'a ProjectIndex, main: Option<&str>) -> Vec<&'a str> {
    let mut out: Vec<&str> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    if let Some(main) = main {
        for rel in Document::walk(index, main).files {
            if let Some(f) = index.get(&rel) {
                if seen.insert(f.rel) {
                    out.push(f.rel);
                }
            }
        }
    }
    for f in index.iter() {
        if seen.insert(f.rel) {
            out.push(f.rel);
        }
    }
    out
}

/// Search every indexed text, returning at most `max` hits.
pub fn search(
    index: &ProjectIndex,
    q: &Query,
    main: Option<&str>,
    max: usize,
) -> Result<SearchResult, String> {
    let re = compile(q)?;
    let mut out = SearchResult {
        files: Vec::new(),
        hits: 0,
        truncated: 0,
        searched: 0,
        unsearched: 0,
    };
    for rel in ordered(index, main) {
        let f = index.get(rel).expect("ordered paths are indexed");
        let Some(text) = f.text else {
            out.unsearched += 1;
            continue;
        };
        out.searched += 1;
        let found = matches(&re, text, q.whole_word);
        if found.is_empty() {
            continue;
        }
        let room = max.saturating_sub(out.hits as usize);
        out.truncated += found.len().saturating_sub(room) as u32;
        if room == 0 {
            continue;
        }
        let starts = line_starts(text);
        let hits: Vec<Hit> = found
            .into_iter()
            .take(room)
            .map(|m| hit_at(text, &starts, m))
            .collect();
        out.hits += hits.len() as u32;
        out.files.push(FileHits {
            rel: rel.to_string(),
            source: f.source,
            revision: f.revision.to_string(),
            hits,
        });
    }
    Ok(out)
}

/// A file the finder offers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct FileMatch {
    pub rel: String,
    pub score: i32,
    /// UTF-16 positions in `rel` of the query's characters, for highlighting.
    pub positions: Vec<u32>,
}

/// Fuzzy score of `query` against a path: every query character must appear
/// in order (case-insensitive). Consecutive runs, starts of path segments
/// and words, and matches in the file name score higher; longer paths score
/// slightly lower. `None` when the query does not match.
pub fn fuzzy_score(query: &str, rel: &str) -> Option<(i32, Vec<u32>)> {
    let q: Vec<char> = query
        .chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect();
    if q.is_empty() {
        return Some((0, Vec::new()));
    }
    let name_start = rel.rfind('/').map(|i| i + 1).unwrap_or(0);
    let chars: Vec<(usize, char)> = rel.char_indices().collect();
    let mut qi = 0;
    let mut score = 0i32;
    let mut positions = Vec::new();
    let mut prev: Option<usize> = None;
    for (ci, &(bi, c)) in chars.iter().enumerate() {
        if qi == q.len() {
            break;
        }
        if c.to_lowercase().eq(std::iter::once(q[qi])) {
            let mut s = 1;
            if prev == Some(ci.wrapping_sub(1)) {
                s += 12;
            }
            let before = if ci == 0 { None } else { Some(chars[ci - 1].1) };
            if matches!(before, None | Some('/')) {
                s += 8;
            } else if matches!(before, Some('_' | '-' | '.' | ' '))
                || (before.is_some_and(char::is_lowercase) && c.is_uppercase())
            {
                s += 4;
            }
            if bi >= name_start {
                s += 2;
            }
            score += s;
            positions.push(utf16_len(&rel[..bi]));
            prev = Some(ci);
            qi += 1;
        }
    }
    if qi < q.len() {
        return None;
    }
    Some((score * 16 - rel.len() as i32, positions))
}

/// The best `max` files for a finder query, best first (ties by path).
pub fn find_files(index: &ProjectIndex, query: &str, max: usize) -> Vec<FileMatch> {
    let mut out: Vec<FileMatch> = index
        .iter()
        .filter_map(|f| {
            fuzzy_score(query, f.rel).map(|(score, positions)| FileMatch {
                rel: f.rel.to_string(),
                score,
                positions,
            })
        })
        .collect();
    out.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.rel.cmp(&b.rel)));
    out.truncate(max);
    out
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

    fn flat(r: &SearchResult) -> Vec<(String, u32, u32, u32)> {
        r.files
            .iter()
            .flat_map(|f| {
                f.hits
                    .iter()
                    .map(move |h| (f.rel.clone(), h.line, h.col, h.len))
            })
            .collect()
    }

    #[test]
    fn literal_search_is_case_insensitive_by_default_and_escapes_regex() {
        let i = index(&[("a.tex", "Foo foo\nf.o fxo\n")]);
        let r = search(&i, &q("foo"), None, 100).unwrap();
        assert_eq!(
            flat(&r),
            [("a.tex".into(), 1, 0, 3), ("a.tex".into(), 1, 4, 3)]
        );
        let r = search(&i, &q("f.o"), None, 100).unwrap();
        assert_eq!(flat(&r), [("a.tex".into(), 2, 0, 3)]);
        let cs = Query {
            case_sensitive: true,
            ..q("Foo")
        };
        assert_eq!(search(&i, &cs, None, 100).unwrap().hits, 1);
    }

    #[test]
    fn regex_and_whole_word_flags() {
        let i = index(&[(
            "a.tex",
            "cat catalog concat cat_x\n\\ref{sec:1} \\ref{sec:22}\n",
        )]);
        let ww = Query {
            whole_word: true,
            ..q("cat")
        };
        assert_eq!(
            flat(&search(&i, &ww, None, 100).unwrap()),
            [("a.tex".into(), 1, 0, 3)]
        );
        let re = Query {
            regex: true,
            ..q(r"\\ref\{sec:\d+\}")
        };
        let r = search(&i, &re, None, 100).unwrap();
        assert_eq!(
            flat(&r),
            [("a.tex".into(), 2, 0, 11), ("a.tex".into(), 2, 12, 12)]
        );
        let bad = Query {
            regex: true,
            ..q("(")
        };
        assert!(search(&i, &bad, None, 100)
            .unwrap_err()
            .starts_with("invalid pattern"));
        assert!(search(&i, &q(""), None, 100).is_err());
    }

    #[test]
    fn main_document_ranks_first_then_paths() {
        let i = index(&[
            ("aaa.tex", "x"),
            ("paper/main.tex", "x\n\\input{ch/z}\n\\input{ch/b}"),
            ("paper/ch/b.tex", "x"),
            ("paper/ch/z.tex", "x"),
            ("zzz.tex", "x"),
        ]);
        let r = search(&i, &q("x"), Some("paper/main.tex"), 100).unwrap();
        let files: Vec<&str> = r.files.iter().map(|f| f.rel.as_str()).collect();
        assert_eq!(
            files,
            [
                "paper/main.tex",
                "paper/ch/z.tex",
                "paper/ch/b.tex",
                "aaa.tex",
                "zzz.tex"
            ]
        );
        let r = search(&i, &q("x"), None, 100).unwrap();
        assert_eq!(r.files[0].rel, "aaa.tex");
    }

    #[test]
    fn caps_hits_and_counts_the_rest() {
        let i = index(&[("a.tex", "x x x"), ("b.tex", "x x"), ("fig.png", "")]);
        let mut i = i;
        i.set_disk(
            "fig.png",
            Disk::Listed {
                bytes: 3,
                reason: crate::Unindexed::NotText,
            },
        );
        let r = search(&i, &q("x"), None, 4).unwrap();
        assert_eq!(
            (r.hits, r.truncated, r.searched, r.unsearched),
            (4, 1, 2, 1)
        );
        assert_eq!(r.files[1].hits.len(), 1);
    }

    #[test]
    fn columns_are_utf16_and_buffers_are_searched() {
        let mut i = index(&[("a.tex", "é😀 needle\r\nnext")]);
        let r = search(&i, &q("needle"), None, 10).unwrap();
        let h = &r.files[0].hits[0];
        assert_eq!((h.col, h.len, h.preview.as_str()), (4, 6, "é😀 needle"));
        assert_eq!(r.files[0].source, Source::Disk);
        i.set_overlay("a.tex", Some("unsaved needle".into()));
        let r = search(&i, &q("needle"), None, 10).unwrap();
        assert_eq!(
            (r.files[0].source, r.files[0].hits[0].col),
            (Source::Buffer, 8)
        );
    }

    #[test]
    fn multi_line_matches_report_their_start() {
        let i = index(&[("a.tex", "one\ntwo end\nthree")]);
        let re = Query {
            regex: true,
            ..q(r"end\nthr")
        };
        let h = &search(&i, &re, None, 10).unwrap().files[0].hits[0];
        assert_eq!((h.line, h.col, h.len), (2, 4, 3));
    }

    #[test]
    fn long_lines_preview_a_window_around_the_match() {
        let line = format!("{}needle{}", "a".repeat(1000), "b".repeat(1000));
        let i = index(&[("a.tex", line.as_str())]);
        let h = &search(&i, &q("needle"), None, 10).unwrap().files[0].hits[0];
        assert!(h.preview.len() <= PREVIEW_MAX);
        assert!(h.preview.contains("needle"));
        assert_eq!(
            h.preview_col + utf16_len(&h.preview[..h.preview.find("needle").unwrap()]),
            h.col
        );
    }

    #[test]
    fn fuzzy_prefers_names_segment_starts_and_runs() {
        let i = index(&[
            ("chapters/intro.tex", ""),
            ("main.tex", ""),
            ("figures/main-plot.pdf", ""),
            ("misc/a/i/n.tex", ""),
        ]);
        let got: Vec<String> = find_files(&i, "main", 10)
            .into_iter()
            .map(|m| m.rel)
            .collect();
        assert_eq!(got, ["main.tex", "figures/main-plot.pdf", "misc/a/i/n.tex"]);
        let intro = find_files(&i, "ch intro", 10);
        assert_eq!(intro[0].rel, "chapters/intro.tex");
        assert_eq!(fuzzy_score("MT", "main.tex").unwrap().1, [0, 5]);
        assert!(fuzzy_score("xyz", "main.tex").is_none());
        assert_eq!(find_files(&i, "", 2).len(), 2);
    }
}
