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

/// Where a running compile is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, TS)]
#[serde(rename_all = "kebab-case")]
pub enum CompilePhase {
    /// Trying the network for files the cache lacks.
    Connect,
    /// Nothing is cached yet: this compile downloads the TeX support files.
    FirstCompile,
    /// Building the LaTeX format from the bundle.
    Format,
    /// A TeX pass; `detail` says why a rerun happened.
    Tex,
    /// BibTeX or an external bibliography tool (`detail` names it).
    Bibliography,
    /// Converting to PDF.
    Xdvipdfmx,
    /// Writing an output file (`detail` names it).
    Writing,
}

/// What one console line says, when it says something typed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, TS)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum LineSignal {
    Fetch {
        file: String,
        outcome: FetchOutcome,
    },
    Phase {
        phase: CompilePhase,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        detail: Option<String>,
    },
}

static DOWNLOADING: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^note: downloading (\S.*)$").unwrap());
static FETCH_FAILED: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"failed to download "([^"]+)"; please check your network connection"#).unwrap()
});
static FILE_NOT_FOUND: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"LaTeX Error: File `([^']+)' not found").unwrap());
static INPUT_UNOPENED: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"^error: failed to open input file "([^"]+)""#).unwrap());
static FONT_MISSING: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"^error: .*Package fontspec Error: The font "([^"]+)" cannot be found"#).unwrap()
});
static SHELL_ESCAPE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^error: .*Package (\S+) Error: You must invoke LaTeX with the -shell-escape flag")
        .unwrap()
});
static ABSOLUTE_PATH: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^warning: accessing absolute path `([^`]+)`").unwrap());
static EXTERNAL_TOOL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^note: Running external tool (\S+)").unwrap());

static RERUN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^note: Rerunning TeX because (.+?)(?: \.\.\.)?$").unwrap());
static WRITING: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^note: Writing `(?:[^`]*/)?([^`/]+)`").unwrap());

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
    let phase = |phase, detail: Option<&str>| {
        Some(LineSignal::Phase {
            phase,
            detail: detail.map(str::to_string),
        })
    };
    if text.starts_with("note: generating format") {
        return phase(CompilePhase::Format, None);
    }
    if text.starts_with("note: Running TeX") {
        return phase(CompilePhase::Tex, None);
    }
    if let Some(m) = RERUN.captures(text) {
        return phase(CompilePhase::Tex, Some(&m[1]));
    }
    if text.starts_with("note: Running BibTeX") {
        return phase(CompilePhase::Bibliography, Some("bibtex"));
    }
    if let Some(m) = EXTERNAL_TOOL.captures(text) {
        return phase(CompilePhase::Bibliography, Some(&m[1]));
    }
    if text.starts_with("note: Running xdvipdfmx") {
        return phase(CompilePhase::Xdvipdfmx, None);
    }
    if let Some(m) = WRITING.captures(text) {
        return phase(CompilePhase::Writing, Some(&m[1]));
    }
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

/// What a run used from outside the bundle: programs it ran and files it
/// read by absolute path (system fonts), each once, in first-use order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExternalNeeds {
    pub tools: Vec<String>,
    pub files: Vec<String>,
}

pub fn external_needs<S: AsRef<str>>(lines: &[S]) -> ExternalNeeds {
    let mut needs = ExternalNeeds::default();
    for l in console_lines(lines) {
        let (re, list) = if l.starts_with("note:") {
            (&*EXTERNAL_TOOL, &mut needs.tools)
        } else {
            (&*ABSOLUTE_PATH, &mut needs.files)
        };
        if let Some(m) = re.captures(l) {
            if !list.iter().any(|x| x == &m[1]) {
                list.push(m[1].to_string());
            }
        }
    }
    needs
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
    // TeX's own miss, or the engine's miss of a file it reads directly (a
    // format input, when the cache was left partly fetched).
    if let Some(file) = first(&lines, &FILE_NOT_FOUND).or_else(|| first(&lines, &INPUT_UNOPENED)) {
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

/// The offline reading of a failed online attempt: a bundle the engine
/// could not reach means nothing was cached; a file it could not fetch
/// means that file is not cached. Anything else passes through unchanged.
pub fn offline_reading(missing: MissingDependency) -> MissingDependency {
    let file = missing.file.clone();
    match missing.reason {
        MissingReason::BundleUnreachable => MissingDependency {
            file: None,
            reason: MissingReason::CacheEmpty,
        },
        MissingReason::FetchFailed => MissingDependency {
            file,
            reason: MissingReason::NotCached,
        },
        _ => missing,
    }
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
    fn a_partly_fetched_cache_is_not_cached() {
        let c = "note: \"version 2\" Tectonic command-line interface activated
note: using only cached resource files
note: generating format \"latex\"
error: failed to open input file \"hyph-el-monoton.tex\"";
        let lines: Vec<&str> = c.lines().collect();
        let in_index = |f: &str| f == "hyph-el-monoton.tex";
        assert_eq!(
            missing_dependency(&lines, false, &in_index),
            Some(MissingDependency {
                file: Some("hyph-el-monoton.tex".into()),
                reason: MissingReason::NotCached
            })
        );
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
    fn external_needs_list_tools_and_absolute_files_once() {
        let c = "note: Running TeX ...
warning: accessing absolute path `/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf`; build may not be reproducible in other environments
note: Running external tool biber ...
warning: accessing absolute path `/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf`; build may not be reproducible in other environments
note: Running external tool biber ...
note: Running xdvipdfmx ...";
        let lines: Vec<&str> = c.lines().collect();
        assert_eq!(
            external_needs(&lines),
            ExternalNeeds {
                tools: vec!["biber".into()],
                files: vec!["/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf".into()],
            }
        );
        assert_eq!(
            external_needs(&["note: Running TeX ..."]),
            ExternalNeeds::default()
        );
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
    }

    #[test]
    fn a_failed_online_attempt_reads_as_offline() {
        let failed = |file: Option<&str>, reason| MissingDependency {
            file: file.map(str::to_string),
            reason,
        };
        assert_eq!(
            offline_reading(failed(None, MissingReason::BundleUnreachable)),
            failed(None, MissingReason::CacheEmpty)
        );
        assert_eq!(
            offline_reading(failed(Some("booktabs.sty"), MissingReason::FetchFailed)),
            failed(Some("booktabs.sty"), MissingReason::NotCached)
        );
        assert_eq!(
            offline_reading(failed(Some("nope.sty"), MissingReason::NotInBundle)),
            failed(Some("nope.sty"), MissingReason::NotInBundle)
        );
    }

    #[test]
    fn route_without_transit_is_a_fetch_failure() {
        let c = "note: Running TeX ...
warning: failure fetching \"booktabs.sty\" from network (1/3)
caused by: error sending request for url (https://data1b.fullyjustified.net/tlextras-2022.0r0.tar)
caused by: dns error: failed to lookup address information
warning: failure fetching \"booktabs.sty\" from network (2/3)
caused by: failed to download \"booktabs.sty\"; please check your network connection.
error: main.tex:3: ! LaTeX Error: File `booktabs.sty' not found.";
        assert_eq!(
            verdict(c, false),
            file("booktabs.sty", MissingReason::FetchFailed)
        );
    }

    #[test]
    fn line_signals_mark_every_phase_of_a_real_run() {
        let phase = |p, d: Option<&str>| {
            Some(LineSignal::Phase {
                phase: p,
                detail: d.map(str::to_string),
            })
        };
        let run = [
            ("note: generating format \"latex\"", phase(CompilePhase::Format, None)),
            ("note: Running TeX ...", phase(CompilePhase::Tex, None)),
            ("note: Running BibTeX on main.aux ...", phase(CompilePhase::Bibliography, Some("bibtex"))),
            ("note: Rerunning TeX because bibtex was run ...", phase(CompilePhase::Tex, Some("bibtex was run"))),
            ("note: Rerunning TeX because \"main.aux\" changed ...", phase(CompilePhase::Tex, Some("\"main.aux\" changed"))),
            ("note: Running external tool biber ...", phase(CompilePhase::Bibliography, Some("biber"))),
            ("note: Running xdvipdfmx ...", phase(CompilePhase::Xdvipdfmx, None)),
            ("note: Writing `/tmp/x/out/main.pdf` (32.79 KiB)", phase(CompilePhase::Writing, Some("main.pdf"))),
            ("note: Skipped writing 3 intermediate files (use --keep-intermediates to keep them)", None),
            ("note: \"version 2\" Tectonic command-line interface activated", None),
        ];
        for (text, want) in run {
            assert_eq!(line_signal(text), want, "{text}");
        }
    }
}
