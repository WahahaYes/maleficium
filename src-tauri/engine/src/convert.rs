//! `convert <input.tex> --out <file.html>`: latexml turns the paper into one
//! HTML5 document. The process runs in the input's directory so `\input` and
//! relative figure paths resolve, and writes nothing but `--out` (no
//! `.latexml.log` beside the source). On failure the conversion log goes to
//! stderr and the exit code is 1.

use std::ffi::OsString;
use std::path::PathBuf;

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
    let mut input: Option<PathBuf> = None;
    let mut out: Option<PathBuf> = None;
    let mut it = args.into_iter();
    while let Some(a) = it.next() {
        match a.to_str() {
            Some("--out") => {
                out = Some(PathBuf::from(it.next().ok_or("--out needs a value")?));
            }
            Some(flag) if flag.starts_with('-') => return Err(format!("unknown option {flag}")),
            _ if input.is_none() => input = Some(PathBuf::from(a)),
            _ => return Err(String::from("exactly one input file")),
        }
    }
    let input = input.ok_or("no input file")?;
    let out = out.ok_or("no --out file")?;
    let source = std::fs::read_to_string(&input)
        .map_err(|e| format!("cannot read {}: {e}", input.display()))?;
    let out = std::path::absolute(&out).map_err(|e| format!("bad --out: {e}"))?;
    if let Some(dir) = input.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::env::set_current_dir(dir)
            .map_err(|e| format!("cannot enter {}: {e}", dir.display()))?;
    }
    let html = latexml::api::convert_to_html(&source)?;
    std::fs::write(&out, html).map_err(|e| format!("cannot write {}: {e}", out.display()))
}
