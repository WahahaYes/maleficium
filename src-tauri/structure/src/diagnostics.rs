//! Engine log → `file:line` diagnostics, rebased onto the project root.
//!
//! `root` and `base` (the directory the engine ran in) are only used to
//! resolve paths: results carry root-relative paths, never absolute ones.
//! Entries that resolve outside the root are flagged `external` and carry
//! no path at all. Repeats (the engine reruns TeX and re-reports) collapse
//! to their first occurrence.
//!
//! `root` and `base` are native paths of whichever OS ran the engine, and
//! the root's own form picks the path rules (`C:\` or UNC → Windows, both
//! separators; otherwise Unix). That keeps this crate host-independent: the
//! Windows cases are tested on every OS.

use std::collections::HashSet;
use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use typed_path::{Utf8TypedPath, Utf8TypedPathBuf};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema, TS,
)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema, TS)]
pub struct Diagnostic {
    /// Root-relative source path; `None` when `external`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub path: Option<String>,
    pub line: u32,
    pub message: String,
    pub severity: Severity,
    /// The source resolves outside the project root.
    pub external: bool,
}

static LINE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:^|\s)((?:[A-Za-z]:)?[\w\-./\\]+\.tex):(\d+):?\s*(.*)").unwrap()
});

/// `file` (absolute, or relative to `base`) as a root-relative `/`-path, or
/// `None` when it resolves outside `root`. Lexical: `..` never climbs past
/// the filesystem root.
fn root_relative(file: &str, root: &Utf8TypedPathBuf, base: &Utf8TypedPathBuf) -> Option<String> {
    let abs = base.join(file).normalize();
    let rel = abs.strip_prefix(root.as_str()).ok()?;
    Some(
        rel.components()
            .map(|c| c.as_str())
            .collect::<Vec<_>>()
            .join("/"),
    )
}

/// XeTeX font tracing (`\XeTeXtracingfonts=1`, which libertine.sty sets
/// under XeTeX): Tectonic reports every trace line as its own warning, which
/// buries the real diagnostics. A request names the font and size, the next
/// line resolves it; neither is actionable, so both shapes are dropped.
static FONT_TRACE_REQUEST_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"^Requested font ".+" at [0-9.]+pt$"#).unwrap());
static FONT_TRACE_TARGET_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^-> \S+$").unwrap());

fn is_font_trace(message: &str) -> bool {
    FONT_TRACE_REQUEST_RE.is_match(message) || FONT_TRACE_TARGET_RE.is_match(message)
}

pub fn diagnostics(log: &str, root: &str, base: &str) -> Vec<Diagnostic> {
    let root = Utf8TypedPath::derive(root).normalize();
    let base = Utf8TypedPath::derive(base).normalize();
    let mut all: Vec<Diagnostic> = Vec::new();
    for line in log.lines() {
        match LINE_RE.captures(line) {
            Some(m) => {
                let message = m[3].to_string();
                if is_font_trace(&message) {
                    continue;
                }
                let path = root_relative(&m[1], &root, &base);
                let severity = if line.trim_start().starts_with("warning:") {
                    Severity::Warning
                } else {
                    Severity::Error
                };
                let Some(nr): Option<u32> = m[2].parse().ok() else {
                    continue;
                };
                all.push(Diagnostic {
                    external: path.is_none(),
                    path,
                    line: nr,
                    message,
                    severity,
                });
            }
            // A `file:line:` line with no message carries its text below
            // (a disabled shell-escape note, a wrapped miss). Fold those
            // continuations in; anything else (a transcript tail, a blank)
            // stays out so it cannot stick to a finished diagnostic.
            None => {
                if let Some(last) = all.last_mut() {
                    let rest = line.trim();
                    if last.message.is_empty() && !rest.is_empty() {
                        last.message.push_str(rest);
                    }
                }
            }
        }
    }
    let mut seen = HashSet::new();
    all.into_iter()
        .filter(|d| !d.message.trim().is_empty())
        .filter(|d| seen.insert((d.path.clone(), d.line, d.message.clone(), d.severity)))
        .collect()
}

/// A LaTeX warning that only TeX's own transcript carries: the engine's
/// console never shows it on a successful compile. It names a key and, for
/// uses, the input line, but not the file; callers place it through the
/// project's index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TexWarning {
    pub kind: TexWarningKind,
    pub key: String,
    pub line: Option<u32>,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TexWarningKind {
    UndefinedReference,
    UndefinedCitation,
    DuplicateLabel,
}

static TEX_WARNING_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"LaTeX Warning: (?:(Reference|Citation) [`']([^']+)' on page \S+ undefined on input line (\d+)|Label `([^']+)' multiply defined)",
    )
    .unwrap()
});

/// TeX's log width: a longer line continues on the next.
const TEX_LOG_WIDTH: usize = 79;

/// The reference, citation and label warnings in a TeX transcript, first
/// occurrence of each.
pub fn tex_warnings(log: &str) -> Vec<TexWarning> {
    let mut joined: Vec<String> = Vec::new();
    let mut continues = false;
    for line in log.lines() {
        match joined.last_mut() {
            Some(last) if continues => last.push_str(line),
            _ => joined.push(line.to_string()),
        }
        continues = line.chars().count() == TEX_LOG_WIDTH;
    }
    let mut seen = HashSet::new();
    joined
        .iter()
        .filter_map(|l| {
            let m = TEX_WARNING_RE.captures(l)?;
            let (kind, key, line) = match (m.get(1).map(|k| k.as_str()), m.get(4)) {
                (Some("Reference"), _) => {
                    (TexWarningKind::UndefinedReference, &m[2], m[3].parse().ok())
                }
                (Some(_), _) => (TexWarningKind::UndefinedCitation, &m[2], m[3].parse().ok()),
                (None, Some(label)) => (TexWarningKind::DuplicateLabel, label.as_str(), None),
                (None, None) => return None,
            };
            let message = m[0].trim_start_matches("LaTeX Warning: ").to_string();
            Some(TexWarning {
                kind,
                key: key.to_string(),
                line,
                message,
            })
        })
        .filter(|w| seen.insert((w.kind, w.key.clone(), w.line)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tex_warnings_name_the_key_and_line() {
        let log = "(main.tex
LaTeX Warning: Citation 'knuth1985' on page 1 undefined on input line 27.
LaTeX Warning: Reference `sec:methods' on page 1 undefined on input line 27.
LaTeX Warning: Reference `sec:methods' on page 1 undefined on input line 27.
LaTeX Warning: Label `sec:intro' multiply defined.
LaTeX Warning: There were undefined references.
Package hyperref Warning: Rerun to get /PageLabels entry.";
        let got: Vec<_> = tex_warnings(log)
            .into_iter()
            .map(|w| (w.kind, w.key, w.line))
            .collect();
        assert_eq!(
            got,
            vec![
                (
                    TexWarningKind::UndefinedCitation,
                    "knuth1985".into(),
                    Some(27)
                ),
                (
                    TexWarningKind::UndefinedReference,
                    "sec:methods".into(),
                    Some(27)
                ),
                (TexWarningKind::DuplicateLabel, "sec:intro".into(), None),
            ]
        );
    }

    #[test]
    fn a_tex_warning_wrapped_at_79_columns_is_read_whole() {
        let whole = "LaTeX Warning: Reference `sec:a-rather-long-label-name-for-wrapping' on page 12 undefined on input line 345.";
        let (a, b) = whole.split_at(TEX_LOG_WIDTH);
        let w = tex_warnings(&format!("{a}\n{b}\n"));
        assert_eq!(w.len(), 1);
        assert_eq!(w[0].key, "sec:a-rather-long-label-name-for-wrapping");
        assert_eq!(w[0].line, Some(345));
    }

    fn d(path: Option<&str>, line: u32, message: &str) -> Diagnostic {
        Diagnostic {
            external: path.is_none(),
            path: path.map(Into::into),
            line,
            message: message.into(),
            severity: Severity::Error,
        }
    }

    #[test]
    fn bare_error_is_root_relative() {
        assert_eq!(
            diagnostics(
                "error: hello.tex:3: Undefined control sequence",
                "/tmp/x",
                "/tmp/x"
            ),
            vec![d(Some("hello.tex"), 3, "Undefined control sequence")]
        );
    }

    #[test]
    fn dot_relative_resolves_against_base() {
        assert_eq!(
            diagnostics("./parts/body.tex:12: oops", "/proj", "/proj/sub"),
            vec![d(Some("sub/parts/body.tex"), 12, "oops")]
        );
        assert_eq!(
            diagnostics("a/b/../c.tex:7: x", "/tmp/x", "/tmp/x"),
            vec![d(Some("a/c.tex"), 7, "x")]
        );
    }

    #[test]
    fn outside_root_is_external_with_no_path() {
        assert_eq!(
            diagnostics("/other/a.tex:5: msg", "/proj", "/proj"),
            vec![d(None, 5, "msg")]
        );
        // A sibling sharing the root's prefix is still outside.
        assert_eq!(
            diagnostics("/project/a.tex:5: msg", "/proj", "/proj"),
            vec![d(None, 5, "msg")]
        );
        assert_eq!(
            diagnostics("../x.tex:1: up", "/proj", "/proj"),
            vec![d(None, 1, "up")]
        );
        let json = serde_json::to_string(&diagnostics("/o/a.tex:5: m", "/proj", "/proj")).unwrap();
        assert!(!json.contains("/o/"), "{json}");
    }

    /// Windows roots, checked on every host: drive and UNC prefixes, both
    /// separators (TeX echoes `\input{parts/x}` with `/`), subdirectory mains.
    #[test]
    fn windows_paths_rebase_onto_the_root() {
        let root = r"C:\Users\ada\paper";
        assert_eq!(
            diagnostics("error: main.tex:3: bad", root, root),
            vec![d(Some("main.tex"), 3, "bad")]
        );
        assert_eq!(
            diagnostics("./parts/body.tex:12: oops", root, r"C:\Users\ada\paper\sub"),
            vec![d(Some("sub/parts/body.tex"), 12, "oops")]
        );
        assert_eq!(
            diagnostics(r"parts\a\..\c.tex:7: x", root, root),
            vec![d(Some("parts/c.tex"), 7, "x")]
        );
        assert_eq!(
            diagnostics(r"error: C:\Users\ada\paper\ch1.tex:5: m", root, root),
            vec![d(Some("ch1.tex"), 5, "m")]
        );
        let unc = r"\\server\share\paper";
        assert_eq!(
            diagnostics("main.tex:2: m", unc, unc),
            vec![d(Some("main.tex"), 2, "m")]
        );
    }

    #[test]
    fn windows_outside_root_is_external() {
        let root = r"C:\Users\ada\paper";
        for log in [
            r"C:\Users\ada\other\a.tex:5: msg",
            r"C:\Users\ada\paperback\a.tex:5: msg",
            r"D:\paper\a.tex:5: msg",
            r"..\x.tex:5: msg",
            "../x.tex:5: msg",
        ] {
            assert_eq!(
                diagnostics(log, root, root),
                vec![d(None, 5, "msg")],
                "{log}"
            );
        }
    }

    #[test]
    fn rerun_repeats_collapse() {
        let log = "warning: w.tex:4: Overfull \\hbox (63.44pt too wide) detected at line 4\nwarning: w.tex:4: Overfull \\hbox (63.44pt too wide) detected at line 4\nwarning: warnings were issued by the TeX engine; use --print and/or --keep-logs for details.";
        let got = diagnostics(log, "/p", "/p");
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].severity, Severity::Warning);
        assert_eq!(got[0].path.as_deref(), Some("w.tex"));
    }

    #[test]
    fn warnings_are_classified() {
        let got = diagnostics("warning: main.tex:9: Reference `x' undefined", "/p", "/p");
        assert_eq!(got[0].severity, Severity::Warning);
    }

    #[test]
    fn xetex_font_trace_lines_are_dropped() {
        let log = "warning: main.tex:47: Requested font \"nxlmi7\" at 7.3pt\n\
             warning: main.tex:47: -> nxlmi7\n\
             warning: main.tex:47: Requested font \"[LinLibertine_R.otf]/OT:script=latn;language=dflt;+tnum;+lnum;mapping=tex-text;\" at 7.0pt\n\
             warning: main.tex:47: -> MinLibReg-ot1\n\
             warning: main.tex:47: \n\
             warning: main.tex:159: Overfull \\hbox (3.27449pt too wide) in paragraph at lines 159--159";
        let got = diagnostics(log, "/p", "/p");
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].path.as_deref(), Some("main.tex"));
        assert_eq!(got[0].line, 159);
        assert!(got[0].message.contains("Overfull"), "{}", got[0].message);
    }

    #[test]
    fn a_file_line_with_no_message_folds_its_continuation() {
        let log = "warning: main.tex:3:\nrunsystem(mkdir -p _minted-main)...disabled.\n\
             error: main.tex:3: Package minted Error: You must invoke LaTeX with the -shell-escape flag.";
        let got = diagnostics(log, "/p", "/p");
        assert_eq!(got.len(), 2);
        assert_eq!(
            got[0].message,
            "runsystem(mkdir -p _minted-main)...disabled."
        );
    }

    #[test]
    fn trailer_lines_stay_out_of_finished_diagnostics() {
        let log = "error: hello.tex:3: Undefined control sequence\nNo pages of output.";
        let got = diagnostics(log, "/tmp/x", "/tmp/x");
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].message, "Undefined control sequence");
    }

    #[test]
    fn tectonic_snippet_keeps_only_file_lines() {
        let log = "error: hello.tex:3: Undefined control sequence\nl.3 Hello \\badcommand\nNo pages of output.";
        assert_eq!(
            diagnostics(log, "/tmp/x", "/tmp/x"),
            vec![d(Some("hello.tex"), 3, "Undefined control sequence")]
        );
        assert!(diagnostics("l.3 Hello \\badcommand", "/tmp/x", "/tmp/x").is_empty());
    }

    #[test]
    fn twenty_thousand_line_log_parses() {
        let log: Vec<String> = (1..=20000)
            .map(|i| {
                if i % 5 == 0 {
                    format!("error: ch{i}.tex:{i}: msg{i}")
                } else {
                    format!("filler line {i}")
                }
            })
            .collect();
        let t = std::time::Instant::now();
        assert_eq!(
            diagnostics(&log.join("\n"), "/tmp/scale", "/tmp/scale").len(),
            4000
        );
        assert!(t.elapsed().as_secs() < 2, "{:?}", t.elapsed());
    }
}
