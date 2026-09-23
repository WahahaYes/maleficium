//! Pre-compile dependency checks over a document's symbols: every package
//! or class neither the project nor the bundle provides (all at once, which
//! a compile cannot do: it stops at the first), biblatex needing biber,
//! shell escape, and fontspec fonts. Pure: the caller answers what the
//! bundle carries and which fonts and tools this machine has.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::symbols::Symbols;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, TS)]
#[serde(rename_all = "kebab-case")]
pub enum CheckKind {
    /// Neither the project nor the TeX bundle has this package or class.
    NotInBundle,
    /// The document needs a program outside the bundle.
    ExternalTool,
    /// The document needs shell escape, which compiles never enable.
    ShellEscape,
    /// A fontspec font this machine does not have.
    SystemFont,
}

/// One pre-compile finding, where the document asks for it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, TS)]
pub struct Finding {
    pub kind: CheckKind,
    /// The package, class, tool or font the document asks for.
    pub name: String,
    /// Root-relative file and 1-based line of the request.
    pub path: String,
    pub line: u32,
    /// A change that removes the need, when one exists.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub suggestion: Option<String>,
}

/// What the checks may ask of the caller. `None` means that check cannot run
/// here (no cached bundle index, no font database) and is skipped.
pub struct CheckEnv<'a> {
    /// Whether the TeX bundle carries a file name (e.g. `amsmath.sty`).
    pub in_bundle: Option<&'a dyn Fn(&str) -> bool>,
    /// Whether the project provides a file name the engine would find.
    pub in_project: &'a dyn Fn(&str) -> bool,
    /// Whether a font family (or font file) is installed.
    pub font_installed: Option<&'a dyn Fn(&str) -> bool>,
}

/// Packages that only work with shell escape.
const SHELL_ESCAPE_PACKAGES: &[&str] = &[
    "minted",
    "pythontex",
    "svg",
    "gnuplottex",
    "auto-pst-pdf",
    "bashful",
    "shellesc",
];

/// `backend=<x>` from a biblatex option list; biblatex's default is biber.
fn biblatex_backend(options: Option<&str>) -> &str {
    options
        .into_iter()
        .flat_map(|o| o.split(','))
        .find_map(|kv| {
            let (k, v) = kv.split_once('=')?;
            (k.trim() == "backend").then(|| v.trim())
        })
        .unwrap_or("biber")
}

/// Findings for each file's symbols, in file order; a name is reported once,
/// where it is first asked for.
pub fn precompile_checks(files: &[(String, &Symbols)], env: &CheckEnv) -> Vec<Finding> {
    let mut out: Vec<Finding> = Vec::new();
    let mut push = |f: Finding| {
        if !out.iter().any(|o| o.kind == f.kind && o.name == f.name) {
            out.push(f);
        }
    };
    let at = |kind, name: &str, path: &str, line, suggestion: Option<&str>| Finding {
        kind,
        name: name.to_string(),
        path: path.to_string(),
        line,
        suggestion: suggestion.map(str::to_string),
    };
    for (path, s) in files {
        for p in &s.packages {
            let file = format!("{}.{}", p.name, if p.class { "cls" } else { "sty" });
            if let Some(in_bundle) = env.in_bundle {
                if !(env.in_project)(&file) && !in_bundle(&file) {
                    push(at(CheckKind::NotInBundle, &file, path, p.line, None));
                }
            }
            if !p.class && p.name == "biblatex" {
                let backend = biblatex_backend(p.options.as_deref());
                if backend == "biber" {
                    push(at(
                        CheckKind::ExternalTool,
                        "biber",
                        path,
                        p.line,
                        Some(
                            "\\usepackage[backend=bibtex]{biblatex} compiles from the bundle alone",
                        ),
                    ));
                }
            }
            if !p.class && SHELL_ESCAPE_PACKAGES.contains(&p.name.as_str()) {
                push(at(CheckKind::ShellEscape, &p.name, path, p.line, None));
            }
        }
        for w in &s.shell_escapes {
            push(at(CheckKind::ShellEscape, &w.key, path, w.line, None));
        }
        if let Some(installed) = env.font_installed {
            for f in &s.fonts {
                let file_like = f.key.contains('.');
                let found = if file_like {
                    (env.in_project)(&f.key) || env.in_bundle.is_some_and(|b| b(&f.key))
                } else {
                    installed(&f.key)
                };
                if !found {
                    push(at(CheckKind::SystemFont, &f.key, path, f.line, None));
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::symbols::symbols;

    fn run(text: &str, env: &CheckEnv) -> Vec<(CheckKind, String, u32)> {
        let s = symbols(text);
        precompile_checks(&[("main.tex".into(), &s)], env)
            .into_iter()
            .map(|f| (f.kind, f.name, f.line))
            .collect()
    }

    const BUNDLE: &[&str] = &[
        "article.cls",
        "amsmath.sty",
        "biblatex.sty",
        "minted.sty",
        "fontspec.sty",
        "texgyretermes-regular.otf",
    ];

    fn env<'a>(
        in_bundle: &'a dyn Fn(&str) -> bool,
        in_project: &'a dyn Fn(&str) -> bool,
        fonts: &'a dyn Fn(&str) -> bool,
    ) -> CheckEnv<'a> {
        CheckEnv {
            in_bundle: Some(in_bundle),
            in_project,
            font_installed: Some(fonts),
        }
    }

    #[test]
    fn every_missing_package_is_listed_at_once() {
        let b = |f: &str| BUNDLE.contains(&f);
        let local = |f: &str| f == "mystyle.sty";
        let fonts = |_: &str| true;
        let got = run(
            "\\documentclass{article}\n\\usepackage{amsmath,nopkga}\n\\usepackage{mystyle}\n\\usepackage{nopkgb}\n\\RequirePackage{nopkgc}\n\\usepackage{nopkga}\n",
            &env(&b, &local, &fonts),
        );
        assert_eq!(
            got,
            vec![
                (CheckKind::NotInBundle, "nopkga.sty".into(), 2),
                (CheckKind::NotInBundle, "nopkgb.sty".into(), 4),
                (CheckKind::NotInBundle, "nopkgc.sty".into(), 5),
            ]
        );
    }

    #[test]
    fn a_missing_class_is_a_cls() {
        let b = |f: &str| BUNDLE.contains(&f);
        let none = |_: &str| false;
        let fonts = |_: &str| true;
        let got = run("\\documentclass{nosuchclass}\n", &env(&b, &none, &fonts));
        assert_eq!(
            got,
            vec![(CheckKind::NotInBundle, "nosuchclass.cls".into(), 1)]
        );
    }

    #[test]
    fn biblatex_needs_biber_unless_the_backend_is_bibtex() {
        let b = |f: &str| BUNDLE.contains(&f);
        let none = |_: &str| false;
        let fonts = |_: &str| true;
        let e = env(&b, &none, &fonts);
        for (opts, flagged) in [
            ("", true),
            ("[backend=biber]", true),
            ("[style=alpha, backend = bibtex]", false),
            ("[backend=bibtex8]", false),
        ] {
            let got = run(&format!("\\usepackage{opts}{{biblatex}}\n"), &e);
            assert_eq!(!got.is_empty(), flagged, "{opts}");
            if flagged {
                let s = symbols(&format!("\\usepackage{opts}{{biblatex}}\n"));
                let f = &precompile_checks(&[("main.tex".into(), &s)], &e)[0];
                assert_eq!(
                    (f.kind, f.name.as_str()),
                    (CheckKind::ExternalTool, "biber")
                );
                assert!(f.suggestion.as_deref().unwrap().contains("backend=bibtex"));
            }
        }
    }

    #[test]
    fn shell_escape_packages_and_write18_are_flagged() {
        let b = |f: &str| BUNDLE.contains(&f);
        let none = |_: &str| false;
        let fonts = |_: &str| true;
        let got = run(
            "\\usepackage{minted}\n\\immediate\\write18{date > d.tex}\n",
            &env(&b, &none, &fonts),
        );
        assert_eq!(
            got,
            vec![
                (CheckKind::ShellEscape, "minted".into(), 1),
                (CheckKind::ShellEscape, "\\write18".into(), 2),
            ]
        );
    }

    #[test]
    fn fonts_resolve_by_family_or_as_bundle_files() {
        let b = |f: &str| BUNDLE.contains(&f);
        let none = |_: &str| false;
        let fonts = |f: &str| f == "DejaVu Sans";
        let got = run(
            "\\usepackage{fontspec}\n\\setmainfont{DejaVu Sans}\n\\setsansfont{NoSuchFontXyz}\n\\setmonofont{texgyretermes-regular.otf}\n",
            &env(&b, &none, &fonts),
        );
        assert_eq!(
            got,
            vec![(CheckKind::SystemFont, "NoSuchFontXyz".into(), 3)]
        );
    }

    #[test]
    fn checks_without_an_index_or_font_database_are_skipped() {
        let none = |_: &str| false;
        let e = CheckEnv {
            in_bundle: None,
            in_project: &none,
            font_installed: None,
        };
        let got = run("\\usepackage{nopkga}\n\\setmainfont{NoSuchFontXyz}\n", &e);
        assert!(got.is_empty());
    }

    #[test]
    fn findings_carry_the_requesting_file() {
        let b = |f: &str| BUNDLE.contains(&f);
        let none = |_: &str| false;
        let fonts = |_: &str| true;
        let main = symbols("\\documentclass{article}\n");
        let part = symbols("\n\n\\usepackage{nopkga}\n");
        let got = precompile_checks(
            &[("main.tex".into(), &main), ("chapters/a.tex".into(), &part)],
            &env(&b, &none, &fonts),
        );
        assert_eq!(got.len(), 1);
        assert_eq!((got[0].path.as_str(), got[0].line), ("chapters/a.tex", 3));
    }
}
