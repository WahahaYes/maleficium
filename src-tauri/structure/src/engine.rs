//! Engine console → typed signals: the files a run fetched or failed to
//! fetch, and which dependency a failed run lacked and why.
//!
//! Reads the engine's own `note:`/`warning:`/`error:` lines and never the
//! TeX transcript it embeds between `=====` rules. Whether a file is in the
//! bundle is the caller's lookup against the bundle index.

use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, TS)]
#[serde(rename_all = "kebab-case")]
pub enum FetchOutcome {
    Fetched,
    Failed,
}

/// Why a compile could not get a dependency.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, TS)]
#[serde(rename_all = "kebab-case")]
pub enum MissingReason {
    /// In the bundle but not cached, and the run could not fetch.
    NotCached,
    /// A fetch was attempted and failed (offline or server down).
    FetchFailed,
    /// The bundle does not carry it: fetching cannot help.
    NotInBundle,
    /// The bundle was never cached and cannot be reached.
    BundleUnreachable,
    /// The bundle location is not a bundle.
    BundleInvalid,
    /// Nothing is cached yet and there is no network to fetch with.
    CacheEmpty,
    /// The pinned bundle URL now resolves to a different digest.
    BundleChanged,
    /// A font the document names is not installed on this machine.
    SystemFont,
    /// A program the engine runs (e.g. biber) is not installed.
    ExternalTool,
    /// A package needs shell escape, which compiles never enable.
    ShellEscapeRequired,
}

/// The dependency a run lacked: a file, font, tool or package name where
/// the engine names one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, TS)]
pub struct MissingDependency {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub file: Option<String>,
    pub reason: MissingReason,
}

/// What one console line says, when it says something typed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, TS)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum LineSignal {
    Fetch { file: String, outcome: FetchOutcome },
}

static DOWNLOADING: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^note: downloading (\S.*)$").unwrap());
static FETCH_FAILED: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"failed to download "([^"]+)"; please check your network connection"#).unwrap()
});
static FILE_NOT_FOUND: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"LaTeX Error: File `([^']+)' not found").unwrap());
static FONT_MISSING: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"^error: .*Package fontspec Error: The font "([^"]+)" cannot be found"#).unwrap()
});
static SHELL_ESCAPE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^error: .*Package (\S+) Error: You must invoke LaTeX with the -shell-escape flag")
        .unwrap()
});
static EXTERNAL_TOOL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^note: Running external tool (\S+)").unwrap());

const BUNDLE_UNREACHABLE: &str =
    "this bundle isn't cached, and we couldn't get it from the internet";
const BUNDLE_INVALID: &str = "doesn't specify a valid bundle";
const FORMAT_INPUT_MISSING: &str = r#"failed to open input file "tectonic-format-latex.tex""#;
const TOOL_NOT_FOUND: &str = "error: No such file or directory (os error 2)";
const CACHED_ONLY: &str = "note: using only cached resource files";
const TRANSCRIPT_OPENS: &str = "its output follows:";

/// Extensions of TeX support files the bundle carries; a missing file with
/// any other extension is a project file, not a bundle gap.
const SUPPORT_EXTS: &[&str] = &[
    "sty", "cls", "clo", "def", "cfg", "fd", "ldf", "bst", "bbx", "cbx", "lbx", "dbx", "tfm", "vf",
    "pfb", "otf", "ttf", "map", "enc",
];

pub fn line_signal(text: &str) -> Option<LineSignal> {
    if let Some(m) = DOWNLOADING.captures(text) {
        return Some(LineSignal::Fetch {
            file: m[1].trim().to_string(),
            outcome: FetchOutcome::Fetched,
        });
    }
    FETCH_FAILED.captures(text).map(|m| LineSignal::Fetch {
        file: m[1].to_string(),
        outcome: FetchOutcome::Failed,
    })
}

/// The engine's own lines: everything but the TeX transcript it embeds
/// between two `=====` rules after announcing it.
fn console_lines<S: AsRef<str>>(lines: &[S]) -> Vec<&str> {
    let mut out = Vec::new();
    let mut state = 0u8; // 0 console, 1 announced, 2 inside transcript
    for l in lines.iter().map(AsRef::as_ref) {
        let rule = !l.is_empty() && l.bytes().all(|b| b == b'=');
        match state {
            1 if rule => state = 2,
            2 if rule => state = 0,
            2 => {}
            _ => {
                if l.ends_with(TRANSCRIPT_OPENS) {
                    state = 1;
                }
                out.push(l);
            }
        }
    }
    out
}

fn first(lines: &[&str], re: &Regex) -> Option<String> {
    lines
        .iter()
        .find_map(|l| re.captures(l).map(|m| m[1].to_string()))
}

fn support_file(name: &str) -> bool {
    !name.contains('/')
        && name
            .rsplit_once('.')
            .is_some_and(|(_, ext)| SUPPORT_EXTS.contains(&ext))
}

/// Which dependency a finished run lacked, from its console lines (both
/// streams, any order) and whether it exited 0. `None` for a success or an
/// ordinary document error. `in_bundle` answers from the bundle index.
pub fn missing_dependency<S: AsRef<str>>(
    lines: &[S],
    success: bool,
    in_bundle: &dyn Fn(&str) -> bool,
) -> Option<MissingDependency> {
    if success {
        return None;
    }
    let lines = console_lines(lines);
    let has = |needle: &str| lines.iter().any(|l| l.contains(needle));
    let found = |file: Option<String>, reason| Some(MissingDependency { file, reason });
    if has(BUNDLE_UNREACHABLE) {
        return found(None, MissingReason::BundleUnreachable);
    }
    if has(BUNDLE_INVALID) {
        return found(None, MissingReason::BundleInvalid);
    }
    if has(FORMAT_INPUT_MISSING) {
        return found(None, MissingReason::CacheEmpty);
    }
    if let Some(font) = first(&lines, &FONT_MISSING) {
        return found(Some(font), MissingReason::SystemFont);
    }
    if let Some(pkg) = first(&lines, &SHELL_ESCAPE) {
        return found(Some(pkg), MissingReason::ShellEscapeRequired);
    }
    if lines.iter().any(|l| l.trim() == TOOL_NOT_FOUND) {
        if let Some(tool) = lines
            .iter()
            .rev()
            .find_map(|l| EXTERNAL_TOOL.captures(l).map(|m| m[1].to_string()))
        {
            return found(Some(tool), MissingReason::ExternalTool);
        }
    }
    let failed_fetch = first(&lines, &FETCH_FAILED);
    if let Some(file) = first(&lines, &FILE_NOT_FOUND) {
        return if in_bundle(&file) {
            if has(CACHED_ONLY) {
                found(Some(file), MissingReason::NotCached)
            } else if failed_fetch.as_deref() == Some(file.as_str()) {
                found(Some(file), MissingReason::FetchFailed)
            } else {
                None
            }
        } else if support_file(&file) {
            found(Some(file), MissingReason::NotInBundle)
        } else {
            None
        };
    }
    failed_fetch.map(|f| MissingDependency {
        file: Some(f),
        reason: MissingReason::FetchFailed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const BUNDLE: &[&str] = &["amsmath.sty", "booktabs.sty", "minted.sty", "fontspec.sty"];

    fn in_bundle(f: &str) -> bool {
        BUNDLE.contains(&f)
    }

    fn verdict(console: &str, success: bool) -> Option<(Option<String>, MissingReason)> {
        let lines: Vec<&str> = console.lines().collect();
        missing_dependency(&lines, success, &in_bundle).map(|m| (m.file, m.reason))
    }

    fn file(f: &str, r: MissingReason) -> Option<(Option<String>, MissingReason)> {
        Some((Some(f.to_string()), r))
    }

    const TRANSCRIPT: &str = "error: something bad happened inside XeTeX; its output follows:

===============================================================================
(main.tex
! ! LaTeX Error: File `booktabs.sty' not found..
\\@missingfileerror ...or: File `#1.#2' not found.}
No pages of output.
===============================================================================
error: the XeTeX engine had an unrecoverable error
caused by: halted on potentially-recoverable error as specified";

    #[test]
    fn cached_only_miss_of_a_bundle_file_is_not_cached() {
        let c = format!(
            "note: \"version 2\" Tectonic command-line interface activated
note: using only cached resource files
note: Running TeX ...
error: main.tex:3: ! LaTeX Error: File `booktabs.sty' not found.
{TRANSCRIPT}"
        );
        assert_eq!(
            verdict(&c, false),
            file("booktabs.sty", MissingReason::NotCached)
        );
    }

    #[test]
    fn online_miss_that_could_not_fetch_is_fetch_failed() {
        let c = format!(
            "note: Running TeX ...
warning: failure fetching \"booktabs.sty\" from network (1/3)
caused by: error sending request for url (https://data1b.fullyjustified.net/tlextras-2022.0r0.tar)
caused by: dns error
warning: open of input booktabs.sty failed
caused by: failed to download \"booktabs.sty\"; please check your network connection.
error: main.tex:3: ! LaTeX Error: File `booktabs.sty' not found.
{TRANSCRIPT}"
        );
        assert_eq!(
            verdict(&c, false),
            file("booktabs.sty", MissingReason::FetchFailed)
        );
    }

    #[test]
    fn a_support_file_absent_from_the_index_is_not_in_bundle() {
        for mode in ["note: using only cached resource files\n", ""] {
            let c = format!(
                "{mode}note: Running TeX ...
error: main.tex:3: ! LaTeX Error: File `nonexistentpkgxyz.sty' not found.
{TRANSCRIPT}"
            );
            assert_eq!(
                verdict(&c, false),
                file("nonexistentpkgxyz.sty", MissingReason::NotInBundle)
            );
        }
    }

    #[test]
    fn a_missing_project_file_is_a_document_error() {
        for f in ["chapters/gone.tex", "gone.tex", "fig.png"] {
            let c =
                format!("error: main.tex:9: ! LaTeX Error: File `{f}' not found.\n{TRANSCRIPT}");
            assert_eq!(verdict(&c, false), None, "{f}");
        }
    }

    #[test]
    fn unreachable_bundle_with_or_without_a_panic() {
        let pinned = "note: using only cached resource files
error: this bundle isn't cached, and we couldn't get it from the internet. Error: error sending request for url (https://data1b.fullyjustified.net/tlextras-2022.0r0.tar.index.gz)";
        let panic = "thread 'main' (290337) panicked at src/config.rs:154:18:
called `Result::unwrap()` on an `Err` value: this bundle isn't cached, and we couldn't get it from the internet. Error: error sending request for url (https://relay.fullyjustified.net/default_bundle_v33.tar.index.gz)";
        for c in [pinned, panic] {
            assert_eq!(
                verdict(c, false),
                Some((None, MissingReason::BundleUnreachable))
            );
        }
    }

    #[test]
    fn a_path_that_is_not_a_bundle_is_invalid() {
        let c = "error: `/nonexistent/path/bundle.tar` doesn't specify a valid bundle.";
        assert_eq!(
            verdict(c, false),
            Some((None, MissingReason::BundleInvalid))
        );
    }

    #[test]
    fn cached_only_without_format_inputs_is_cache_empty() {
        let c = "note: using only cached resource files
note: generating format \"latex\"
error: failed to open input file \"tectonic-format-latex.tex\"";
        assert_eq!(verdict(c, false), Some((None, MissingReason::CacheEmpty)));
    }

    #[test]
    fn missing_system_font_names_the_font() {
        let c = format!(
            "warning: accessing absolute path `/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf`; build may not be reproducible in other environments
error: main.tex:4: Package fontspec Error: The font \"NoSuchFontXyz\" cannot be found.
For immediate help type H <return>
{TRANSCRIPT}"
        );
        assert_eq!(
            verdict(&c, false),
            file("NoSuchFontXyz", MissingReason::SystemFont)
        );
    }

    #[test]
    fn minted_without_shell_escape_names_the_package() {
        let c = format!(
            "warning: main.tex:3:
runsystem(mkdir -p _minted-main)...disabled.
error: main.tex:3: Package minted Error: You must invoke LaTeX with the -shell-escape flag.
See the minted package documentation for explanation.
{TRANSCRIPT}"
        );
        assert_eq!(
            verdict(&c, false),
            file("minted", MissingReason::ShellEscapeRequired)
        );
    }

    #[test]
    fn a_missing_external_tool_is_named_from_the_run_note() {
        let c = "note: Running TeX ...
note: Running external tool biber ...
error: No such file or directory (os error 2)";
        assert_eq!(
            verdict(c, false),
            file("biber", MissingReason::ExternalTool)
        );
    }

    #[test]
    fn a_fetch_failure_without_a_tex_error_is_fetch_failed() {
        let c = "caused by: failed to download \"cmr10.tfm\"; please check your network connection.
error: the XeTeX engine had an unrecoverable error";
        assert_eq!(
            verdict(c, false),
            file("cmr10.tfm", MissingReason::FetchFailed)
        );
    }

    #[test]
    fn document_errors_and_successes_lack_nothing() {
        let c = format!("error: main.tex:3: Undefined control sequence.\n{TRANSCRIPT}");
        let transcript_only = TRANSCRIPT.replace("booktabs", "amsmath");
        assert_eq!(verdict(&c.replace("booktabs", "amsmath"), false), None);
        assert_eq!(verdict(&transcript_only, false), None);
        let retried = "warning: failure fetching \"x.sty\" from network (1/3)
caused by: failed to download \"x.sty\"; please check your network connection.";
        assert_eq!(verdict(retried, true), None);
    }

    #[test]
    fn line_signals_mark_fetches_and_skip_retries() {
        assert_eq!(
            line_signal("note: downloading tectonic-format-latex.tex"),
            Some(LineSignal::Fetch {
                file: "tectonic-format-latex.tex".into(),
                outcome: FetchOutcome::Fetched
            })
        );
        assert_eq!(
            line_signal(
                "caused by: failed to download \"booktabs.sty\"; please check your network connection."
            ),
            Some(LineSignal::Fetch {
                file: "booktabs.sty".into(),
                outcome: FetchOutcome::Failed
            })
        );
        assert_eq!(
            line_signal("warning: failure fetching \"booktabs.sty\" from network (1/3)"),
            None
        );
        assert_eq!(line_signal("note: Running TeX ..."), None);
    }
}
