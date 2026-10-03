//! `compile <input> [--outdir D] [--synctex] [--keep-logs] [-b URL] [-C]`:
//! Tectonic 0.17's `-X compile` for the options the app passes, built from
//! the same library calls the CLI makes so the status lines match.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use tectonic::config::PersistentConfig;
use tectonic::driver::{OutputFormat, PassSetting, ProcessingSessionBuilder};
use tectonic::errors::EngineError;
use tectonic::status::plain::PlainStatusBackend;
use tectonic::status::{ChatterLevel, StatusBackend};
use tectonic::unstable_opts::UnstableOptions;
use tectonic::{tt_error, tt_note};
use tectonic_bridge_core::{SecuritySettings, SecurityStance};
use tectonic_bundles::detect_bundle;
use tectonic_errors::{Error, Result};

struct Options {
    input: String,
    outdir: Option<PathBuf>,
    bundle: Option<String>,
    only_cached: bool,
    synctex: bool,
    keep_logs: bool,
}

fn parse(args: Vec<OsString>) -> Result<Options> {
    let mut o = Options {
        input: String::new(),
        outdir: None,
        bundle: None,
        only_cached: false,
        synctex: false,
        keep_logs: false,
    };
    let mut it = args.into_iter();
    let value = |it: &mut std::vec::IntoIter<OsString>, flag: &str| {
        it.next()
            .ok_or_else(|| Error::msg(format!("{flag} needs a value")))
    };
    while let Some(a) = it.next() {
        match a.to_str() {
            Some("--outdir") => o.outdir = Some(PathBuf::from(value(&mut it, "--outdir")?)),
            Some("-b") | Some("--bundle") => {
                o.bundle = Some(value(&mut it, "--bundle")?.to_string_lossy().into_owned())
            }
            Some("-C") | Some("--only-cached") => o.only_cached = true,
            Some("--synctex") => o.synctex = true,
            Some("--keep-logs") => o.keep_logs = true,
            Some(flag) if flag.starts_with('-') && flag != "-" => {
                return Err(Error::msg(format!("unknown option {flag}")))
            }
            _ if o.input.is_empty() => o.input = a.to_string_lossy().into_owned(),
            _ => return Err(Error::msg("exactly one input file")),
        }
    }
    if o.input.is_empty() {
        return Err(Error::msg("no input file"));
    }
    Ok(o)
}

pub fn run(args: Vec<OsString>) -> i32 {
    let mut status = PlainStatusBackend::new(ChatterLevel::Normal);
    match execute(args, &mut status) {
        Ok(()) => 0,
        Err(e) => {
            status.report_error(&e);
            1
        }
    }
}

fn execute(args: Vec<OsString>, status: &mut dyn StatusBackend) -> Result<()> {
    let o = parse(args)?;
    // The first line the app has always seen from the engine (core's fixtures
    // carry it), kept so the transcript is unchanged.
    tt_note!(
        status,
        "\"version 2\" Tectonic command-line interface activated"
    );
    let config = PersistentConfig::open(false)?;
    let mut sb = ProcessingSessionBuilder::new_with_security(SecuritySettings::new(
        SecurityStance::MaybeAllowInsecures,
    ));
    sb.unstables(UnstableOptions::default())
        .format_name("latex")
        .keep_logs(o.keep_logs)
        .keep_intermediates(false)
        .format_cache_path(config.format_cache_path()?)
        .synctex(o.synctex)
        .output_format(OutputFormat::Pdf)
        .pass(PassSetting::Default);

    let input = Path::new(&o.input);
    sb.primary_input_path(input);
    let name = input
        .file_name()
        .ok_or_else(|| Error::msg(format!("no basename for input path \"{}\"", o.input)))?;
    sb.tex_input_name(&name.to_string_lossy());
    sb.output_dir(
        input
            .parent()
            .ok_or_else(|| Error::msg(format!("no parent directory for \"{}\"", o.input)))?,
    );
    if let Some(dir) = o.outdir {
        if !dir.is_dir() {
            return Err(Error::msg(format!(
                "output directory \"{}\" does not exist",
                dir.display()
            )));
        }
        sb.output_dir(dir);
    }
    sb.print_stdout(false);
    if o.only_cached {
        tt_note!(status, "using only cached resource files");
    }
    match o.bundle {
        Some(url) => match detect_bundle(url.clone(), o.only_cached, None)? {
            Some(b) => sb.bundle(b),
            None => {
                return Err(Error::msg(format!(
                    "`{url}` doesn't specify a valid bundle."
                )))
            }
        },
        None => sb.bundle(config.default_bundle(o.only_cached)?),
    };
    sb.build_date_from_env(false);

    let mut sess = sb.create(status)?;
    let result = sess.run(status);
    if let Err(e) = &result {
        if let Some(err) = e.downcast_ref::<EngineError>() {
            let output = sess.get_stdout_content();
            if output.is_empty() {
                tt_error!(
                    status,
                    "something bad happened inside {}, but no output was logged",
                    err.engine()
                );
            } else {
                tt_error!(
                    status,
                    "something bad happened inside {}; its output follows:\n",
                    err.engine()
                );
                status.dump_error_logs(&output);
            }
        }
    }
    result
}
