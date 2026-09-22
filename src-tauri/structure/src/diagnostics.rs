//! Engine log → `file:line` diagnostics, rebased onto the project root.
//!
//! `root` and `base` (the directory the engine ran in) are only used to
//! resolve paths: results carry root-relative paths, never absolute ones.
//! Entries that resolve outside the root are flagged `external` and carry
//! no path at all. Repeats (the engine reruns TeX and re-reports) collapse
//! to their first occurrence.

use std::collections::HashSet;
use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

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

static LINE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?:^|\s)(?:\./)?([\w\-./]+\.tex):(\d+):?\s*(.*)").unwrap());

/// Lexical normalization of an absolute `/`-path: `.` and empty segments
/// drop, `..` pops (never past `/`).
fn normalize(p: &str) -> String {
    let mut out: Vec<&str> = Vec::new();
    for part in p.split('/') {
        match part {
            ".." => {
                out.pop();
            }
            "." | "" => {}
            s => out.push(s),
        }
    }
    format!("/{}", out.join("/"))
}

pub fn diagnostics(log: &str, root: &str, base: &str) -> Vec<Diagnostic> {
    let root = normalize(root);
    let prefix = if root == "/" {
        root.clone()
    } else {
        format!("{root}/")
    };
    let base = normalize(base);
    let mut seen = HashSet::new();
    log.lines()
        .filter_map(|line| {
            let m = LINE_RE.captures(line)?;
            let file = &m[1];
            let abs = if file.starts_with('/') {
                normalize(file)
            } else {
                normalize(&format!("{base}/{file}"))
            };
            let path = abs.strip_prefix(&prefix).map(str::to_string);
            let severity = if line.trim_start().starts_with("warning:") {
                Severity::Warning
            } else {
                Severity::Error
            };
            Some(Diagnostic {
                external: path.is_none(),
                path,
                line: m[2].parse().ok()?,
                message: m[3].to_string(),
                severity,
            })
        })
        .filter(|d| seen.insert((d.path.clone(), d.line, d.message.clone(), d.severity)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

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
