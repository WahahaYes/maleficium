//! `dump --out <dir> --bundle <url> [--cache <dir>] [-C]`: generates latexml's
//! kernel format dumps from the pinned bundle, exactly what
//! `latexml_oxide --init=plain.tex` and `--init=latex.ltx` write:
//! `plain.<year>.dump.txt`, `latex.<year>.dump.txt` and
//! `texlive.<year>.version` in `<dir>`. The year is the one the `kpsewhich`
//! mode reports (2022 for the pinned bundle), so `convert` picks the dumps up
//! through `LATEXML_DUMP_DIR`. The build runs this once per bundle and latexml
//! version; the dumps are too large to commit.
//!
//! latexml's `dump_format` writes to `resources/dumps/` under the working
//! directory when given no destination, so the run happens in the scratch
//! directory and the results are copied out.

use std::ffi::OsString;
use std::path::PathBuf;
use std::rc::Rc;

use latexml::converter::Converter;
use latexml_core::common::{Config, OutputFormat};

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

fn execute(args: Vec<OsString>) -> Result<(), String> {
    let mut out: Option<PathBuf> = None;
    let mut session = Options::default();
    let mut it = args.into_iter();
    while let Some(a) = it.next() {
        let mut value = |flag: &str| it.next().ok_or_else(|| format!("{flag} needs a value"));
        match a.to_str() {
            Some("--out") => out = Some(PathBuf::from(value("--out")?)),
            Some("--cache") => session.cache = Some(PathBuf::from(value("--cache")?)),
            Some("-b") | Some("--bundle") => {
                session.bundle = Some(value("--bundle")?.to_string_lossy().into_owned())
            }
            Some("-C") | Some("--only-cached") => session.cached_only = true,
            _ => return Err(format!("unknown argument {}", a.to_string_lossy())),
        }
    }
    let out = std::path::absolute(out.ok_or("no --out directory")?)
        .map_err(|e| format!("bad --out: {e}"))?;
    std::fs::create_dir_all(&out).map_err(|e| format!("cannot create {}: {e}", out.display()))?;

    let s = session::prepare(&session)?;
    let work = s.scratch.join("work");
    std::fs::create_dir_all(&work).map_err(|e| format!("cannot create {}: {e}", work.display()))?;
    std::env::set_current_dir(&work)
        .map_err(|e| format!("cannot enter {}: {e}", work.display()))?;
    // Read by the engine's format loading to stop after the bootstrap pool;
    // must be set before the session is prepared.
    std::env::set_var("LATEXML_INI_MODE", "1");

    for init in ["plain.tex", "latex.ltx"] {
        let written = dump_one(init)?;
        eprintln!("dump: {init}: {written} entries");
    }
    let made = work.join("resources").join("dumps");
    let mut copied = 0;
    for entry in std::fs::read_dir(&made).map_err(|e| format!("no dumps written: {e}"))? {
        let path = entry.map_err(|e| e.to_string())?.path();
        let name = path.file_name().ok_or("bad dump name")?;
        std::fs::copy(&path, out.join(name))
            .map_err(|e| format!("cannot copy {}: {e}", path.display()))?;
        copied += 1;
    }
    if copied != 3 {
        return Err(format!(
            "expected 3 dump files, found {copied} in {}",
            made.display()
        ));
    }
    eprintln!("dump: wrote {copied} files to {}", out.display());
    Ok(())
}

/// One init file on its own 256 MiB-stack worker thread (the engine is a
/// thread-local singleton, freed before the thread exits), so each dump
/// starts from a fresh engine as it does in the `latexml_oxide` process.
fn dump_one(init: &'static str) -> Result<usize, String> {
    let work = move || -> Result<usize, String> {
        let opts = Config {
            verbosity: -1,
            format: OutputFormat::HTML5,
            bindings_dispatch: Some(Rc::new(latexml_package::dispatch)),
            extra_bindings_dispatch: Some(Rc::new(latexml_contrib::dispatch)),
            ..Config::default()
        };
        let mut converter = Converter::from_config(opts.clone());
        converter
            .prepare_session(&opts)
            .map_err(|e| format!("could not prepare session: {e}"))?;
        let n = latexml::ini_tex::dump_format(&mut converter, init, None);
        latexml_core::reset_thread_engine();
        n
    };
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(work)
        .map_err(|e| format!("cannot start the dump thread: {e}"))?
        .join()
        .map_err(|_| format!("the {init} dump thread died"))?
}
