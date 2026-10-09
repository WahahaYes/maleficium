//! `convert <input.tex> --out <file.html> --bundle URL [--cache D] [--dumps D]
//! [--log F] [-C]`: latexml turns the paper into one HTML5 document.
//!
//! No TeX distribution is needed: `session::prepare` makes the engine answer
//! latexml's `kpsewhich` calls from the pinned bundle. The process runs in the
//! input's directory so `\input` and relative figure paths resolve. Beside the
//! source nothing is written: latexml's post-processing copies its stylesheets
//! (`LaTeXML.css`, `ltx-article.css`, ...) into the destination's directory,
//! so with the destination set to `--out` they land next to it (the old
//! destination-less call used the working directory, which is the input's).
//! Errors in the paper (an undefined macro) still produce the article and are
//! counted in the log; a fatal status, or a result that is not an HTML
//! document, exits 1. The conversion log goes to `--log`; without it a short
//! summary goes to stderr.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use latexml::converter::Converter;
use latexml::post::{self, PostOptions};
use latexml_core::common::{Config, OutputFormat};

use crate::graphics;
use crate::session::{self, Options};

pub fn run(args: Vec<OsString>) -> i32 {
    match execute(args) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("error: {e}");
            1
        }
    }
}

struct Args {
    input: PathBuf,
    out: PathBuf,
    log: Option<PathBuf>,
    dumps: Option<PathBuf>,
    session: Options,
}

fn parse(args: Vec<OsString>) -> Result<Args, String> {
    let mut input: Option<PathBuf> = None;
    let mut out: Option<PathBuf> = None;
    let mut log: Option<PathBuf> = None;
    let mut dumps: Option<PathBuf> = None;
    let mut session = Options::default();
    let mut it = args.into_iter();
    let value = |it: &mut std::vec::IntoIter<OsString>, flag: &str| {
        it.next().ok_or_else(|| format!("{flag} needs a value"))
    };
    while let Some(a) = it.next() {
        match a.to_str() {
            Some("--out") => out = Some(PathBuf::from(value(&mut it, "--out")?)),
            Some("--log") => log = Some(PathBuf::from(value(&mut it, "--log")?)),
            Some("--dumps") => dumps = Some(PathBuf::from(value(&mut it, "--dumps")?)),
            Some("--cache") => session.cache = Some(PathBuf::from(value(&mut it, "--cache")?)),
            Some("-b") | Some("--bundle") => {
                session.bundle = Some(value(&mut it, "--bundle")?.to_string_lossy().into_owned())
            }
            Some("-C") | Some("--only-cached") => session.cached_only = true,
            Some(flag) if flag.starts_with('-') => return Err(format!("unknown option {flag}")),
            _ if input.is_none() => input = Some(PathBuf::from(a)),
            _ => return Err(String::from("exactly one input file")),
        }
    }
    Ok(Args {
        input: input.ok_or("no input file")?,
        out: out.ok_or("no --out file")?,
        log,
        dumps,
        session,
    })
}

/// A finished conversion.
struct Converted {
    html: String,
    log: String,
    status: String,
    status_code: usize,
}

/// latexml's status codes: 0 clean, 1 warnings, 2 errors, 3 fatal.
const FATAL: usize = 3;

fn is_html_document(text: &str) -> bool {
    let head: String = text
        .trim_start()
        .chars()
        .take(32)
        .collect::<String>()
        .to_ascii_lowercase();
    head.starts_with("<!doctype html") || head.starts_with("<html")
}

fn execute(args: Vec<OsString>) -> Result<(), String> {
    let a = parse(args)?;
    let source = std::fs::read_to_string(&a.input)
        .map_err(|e| format!("cannot read {}: {e}", a.input.display()))?;
    let out = std::path::absolute(&a.out).map_err(|e| format!("bad --out: {e}"))?;
    let log_path = a
        .log
        .as_ref()
        .map(std::path::absolute)
        .transpose()
        .map_err(|e| format!("bad --log: {e}"))?;
    let dumps = session::dump_dir(a.dumps.clone())?;
    let source_dir = std::path::absolute(&a.input)
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .ok_or("bad input path")?;
    for file in std::iter::once(&out).chain(log_path.as_ref()) {
        if let Some(dir) = file.parent() {
            std::fs::create_dir_all(dir)
                .map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
        }
    }

    let _session = session::prepare(&a.session)?;
    std::env::set_var("LATEXML_DUMP_DIR", &dumps);
    std::env::set_current_dir(&source_dir)
        .map_err(|e| format!("cannot enter {}: {e}", source_dir.display()))?;

    let done = convert(source, &out, &source_dir);
    let log_text = format!("{}\n{}", done.status, done.log);
    if let Some(path) = &log_path {
        std::fs::write(path, &log_text)
            .map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    }
    if done.status_code >= FATAL {
        if log_path.is_none() {
            eprint!("{log_text}");
        }
        return Err(format!(
            "conversion failed ({}){}",
            done.status.trim(),
            log_hint(&log_path)
        ));
    }
    if !is_html_document(&done.html) {
        if log_path.is_none() {
            eprint!("{log_text}");
        }
        return Err(format!(
            "conversion did not produce an HTML document ({}){}",
            done.status.trim(),
            log_hint(&log_path)
        ));
    }
    std::fs::write(&out, &done.html).map_err(|e| format!("cannot write {}: {e}", out.display()))?;
    if log_path.is_none() {
        eprintln!("{}", summary(&done));
    }
    Ok(())
}

fn log_hint(log: &Option<PathBuf>) -> String {
    log.as_ref()
        .map(|p| format!("; log: {}", p.display()))
        .unwrap_or_default()
}

/// The status line and the errors in the log (not the warnings).
fn summary(done: &Converted) -> String {
    let mut lines = vec![done.status.trim().to_string()];
    lines.extend(
        done.log
            .lines()
            .filter(|l| l.starts_with("Error:") || l.starts_with("Fatal:"))
            .map(str::to_string),
    );
    lines.join("\n")
}

/// Runs the whole pipeline on latexml's 256 MiB-stack worker thread (the
/// engine is a thread-local singleton; deeply nested math overflows the
/// default stack) and frees the thread's engine before it exits.
fn convert(source: String, out: &Path, source_dir: &Path) -> Converted {
    let out = out.to_string_lossy().into_owned();
    let source_dir = source_dir.to_string_lossy().into_owned();
    let work = move || {
        let opts = Config {
            verbosity: -1,
            format: OutputFormat::HTML5,
            bindings_dispatch: Some(Rc::new(latexml_package::dispatch)),
            extra_bindings_dispatch: Some(Rc::new(latexml_contrib::dispatch)),
            // Raw-load a package that has no binding (an author's own .sty).
            // An author .rhai beside their .sty needs no wiring here: the
            // converter's binding chain (latexml's `runtime-bindings`
            // default feature, active in this build) probes `<pkg>.sty.rhai`
            // in the source directory before any compiled binding, so it
            // overrides the raw-loaded .sty (verified: `X.sty.rhai` wins over
            // `X.sty` in the input's folder; without it the .sty raw-loads).
            // Revisit only if a real paper needs more (a .rhai distributed
            // on the TeX tree outside the project resolves through the
            // chain's last tier already).
            include_styles: Some(true),
            ..Config::default()
        };
        let mut converter = Converter::from_config(opts.clone());
        let failed = |why: String| Converted {
            html: String::new(),
            log: why,
            status: String::from("Status:conversion:3"),
            status_code: FATAL,
        };
        if let Err(e) = converter.prepare_session(&opts) {
            return failed(format!("Fatal:session could not prepare: {e}"));
        }
        let core = converter.convert(format!("literal:{source}"));
        let Some(xml) = core.result else {
            return Converted {
                html: String::new(),
                log: core.log,
                status: core.status,
                status_code: core.status_code.max(FATAL),
            };
        };
        let post_opts = PostOptions {
            pmml: true,
            cmml: false,
            keep_xmath: false,
            stylesheet: post::default_stylesheet(Some("html5")),
            destination: Some(&out),
            source_directory: Some(&source_dir),
            site_directory: None,
            search_paths: &[],
            nodefaultresources: false,
            css_files: &[],
            js_files: &[],
            noinvisibletimes: false,
            plane1: true,
            hackplane1: false,
            mathtex: false,
            url_style: latexml_post::crossref::UrlStyle::File,
            navigationtoc: None,
            schemadocs: false,
            split: false,
            split_xpath: None,
            split_naming: None,
            xslt_parameters: &[],
            graphics_svg_threshold_kb: 0,
            // No image tools are assumed: figures keep their references.
            graphicimages: false,
            timestamp: None,
            icon: None,
            whatsout: latexml_post::extract::Whatsout::Document,
        };
        let post = post::run_post_processing_logged(&xml, &post_opts);
        let status_code = core.status_code.max(post.status_code);
        let mut log = format!("{}\n{}", core.log, post.log);
        let html = match graphics::inject(&xml, &post.html) {
            Ok(html) => html,
            Err(why) => {
                log.push_str(&format!("\nWarning:graphics:data-graphic {why}\n"));
                post.html
            }
        };
        Converted {
            html,
            log,
            status: if post.status_code > core.status_code {
                format!("Status:conversion:{}", post.status_code)
            } else {
                core.status
            },
            status_code,
        }
    };
    let joined = std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || {
            let done = work();
            // The engine's thread-local roots do not drop with the thread.
            latexml_core::reset_thread_engine();
            done
        })
        .map(|h| h.join());
    match joined {
        Ok(Ok(done)) => done,
        _ => Converted {
            html: String::new(),
            log: String::from("Fatal:engine the conversion thread died"),
            status: String::from("Status:conversion:3"),
            status_code: FATAL,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_documents_are_told_from_latexml_xml() {
        assert!(is_html_document("<!DOCTYPE html>\n<html>"));
        assert!(is_html_document("  <html lang=\"en\">"));
        assert!(!is_html_document(
            "<?xml version=\"1.0\"?>\n<?latexml class=\"article\"?>\n<document>"
        ));
        assert!(!is_html_document(""));
    }

    #[test]
    fn options_parse() {
        let args = [
            "p.tex", "--out", "o.html", "-b", "u", "--cache", "c", "-C", "--log", "l",
        ]
        .map(OsString::from)
        .to_vec();
        let a = parse(args).unwrap();
        assert_eq!(a.input, PathBuf::from("p.tex"));
        assert_eq!(a.session.bundle.as_deref(), Some("u"));
        assert!(a.session.cached_only);
        assert_eq!(a.log, Some(PathBuf::from("l")));
        assert!(parse(vec!["a".into(), "b".into(), "--out".into(), "o".into()]).is_err());
        assert!(parse(vec!["a".into()]).is_err());
    }
}
