//! Engine outputs of one main file, addressed by session root and
//! root-relative path. The outdir is derived here and never crosses the seam.

use std::path::{Path, PathBuf};

use super::MainOutputs;

/// Resolve a main file inside the root and derive where its outputs live.
pub fn outputs_of(root_id: &str, main_rel: &str) -> Result<MainOutputs, String> {
    let main = super::fs::resolve_in(root_id, main_rel)?;
    if !main.is_file() {
        return Err(format!("not a file: {}", main_rel));
    }
    super::main_outputs(&main)
}

/// Where one main file's engine log lives: the engine's console output
/// (`error:`/`warning: file:line:` records plus the TeX transcript on
/// failure), written by the compile paths.
pub fn log_file(outdir: &Path, main_file: &str) -> PathBuf {
    let stem = main_file.strip_suffix(".tex").unwrap_or(main_file);
    outdir.join(format!("{}.engine.log", stem))
}

/// TeX's own transcript of the last pass (the engine runs with
/// `--keep-logs`): the only record of warnings such as undefined references,
/// which never reach the console of a successful compile.
pub fn tex_log_file(outdir: &Path, main_file: &str) -> PathBuf {
    let stem = main_file.strip_suffix(".tex").unwrap_or(main_file);
    outdir.join(format!("{}.log", stem))
}

fn log_path(o: &MainOutputs) -> PathBuf {
    log_file(&o.outdir, &o.main_file)
}

/// Keep a finished run's console output as the engine log. Best effort: a
/// failed write only costs the log, never the compile result.
pub fn write_engine_log(log: &Path, lines: &[String]) {
    let _ = std::fs::write(log, lines.join("\n"));
}

/// TeX's transcript of the last compile, empty when it left none.
pub fn tex_log(root_id: &str, main_rel: &str) -> Result<String, String> {
    let o = outputs_of(root_id, main_rel)?;
    Ok(std::fs::read_to_string(tex_log_file(&o.outdir, &o.main_file)).unwrap_or_default())
}

/// The full engine log of the last compile.
pub fn engine_log(root_id: &str, main_rel: &str) -> Result<String, String> {
    let o = outputs_of(root_id, main_rel)?;
    std::fs::read_to_string(log_path(&o)).map_err(|e| format!("log unavailable: {}", e))
}

/// The last `max_lines` lines of the engine log.
pub fn log_tail(root_id: &str, main_rel: &str, max_lines: usize) -> Result<String, String> {
    let text = engine_log(root_id, main_rel)?;
    let lines: Vec<&str> = text.lines().collect();
    let start = lines.len().saturating_sub(max_lines.max(1));
    Ok(lines[start..].join("\n"))
}

/// When and how large the main file's pdf last was written: whoever
/// compiled it (this app, an agent over MCP) changes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct OutputStamp {
    pub mtime_ms: u64,
    pub bytes: u64,
}

/// The pdf's stamp, or `None` when no compile has left one.
pub fn output_stamp(root_id: &str, main_rel: &str) -> Result<Option<OutputStamp>, String> {
    let o = outputs_of(root_id, main_rel)?;
    let Ok(meta) = std::fs::metadata(o.outdir.join(&o.pdf_name)) else {
        return Ok(None);
    };
    let mtime_ms = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_millis() as u64);
    Ok(Some(OutputStamp {
        mtime_ms,
        bytes: meta.len(),
    }))
}

/// Where the main file's compiled pdf is, or `None` before any compile left
/// one: what a viewer opens when someone else (an agent over MCP) compiled
/// a document this app has not.
pub fn output_pdf(root_id: &str, main_rel: &str) -> Result<Option<String>, String> {
    let o = outputs_of(root_id, main_rel)?;
    let pdf = o.outdir.join(&o.pdf_name);
    Ok(pdf.is_file().then(|| pdf.to_string_lossy().to_string()))
}

/// Whether a previous compile left a pdf for this main file.
pub fn outputs_fresh(root_id: &str, main_rel: &str) -> Result<bool, String> {
    let o = outputs_of(root_id, main_rel)?;
    Ok(o.outdir.join(&o.pdf_name).is_file())
}

/// Remove the build artifacts of this main file's outdir; sources are never
/// there. Returns how many files went. A missing outdir is already clean.
pub fn clean_outputs(root_id: &str, main_rel: &str) -> Result<usize, String> {
    let o = outputs_of(root_id, main_rel)?;
    let entries = match std::fs::read_dir(&o.outdir) {
        Ok(e) => e,
        Err(_) => return Ok(0),
    };
    let mut removed = 0;
    for entry in entries.flatten() {
        if entry.file_type().is_ok_and(|t| t.is_file())
            && std::fs::remove_file(entry.path()).is_ok()
        {
            removed += 1;
        }
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(name: &str) -> (String, PathBuf) {
        let dir = crate::test_scratch::dir(&format!("outputs-{}", name));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("ch")).unwrap();
        std::fs::write(dir.join("main.tex"), "x").unwrap();
        let canon = dunce::canonicalize(&dir).unwrap();
        let id = format!("out-{}", name);
        crate::core::fs::grant_root(&id, &canon.to_string_lossy()).unwrap();
        (id, canon)
    }

    #[test]
    fn outputs_live_in_the_app_cache_and_round_trip() {
        let (id, root) = project("rt");
        assert!(!outputs_fresh(&id, "main.tex").unwrap());
        assert_eq!(clean_outputs(&id, "main.tex").unwrap(), 0);
        assert!(engine_log(&id, "main.tex").is_err());

        let o = outputs_of(&id, "main.tex").unwrap();
        assert!(!o.outdir.starts_with(&root));
        std::fs::create_dir_all(o.outdir.join("keep")).unwrap();
        std::fs::write(o.outdir.join("main.pdf"), "%PDF").unwrap();
        std::fs::write(log_path(&o), "a\nb\nc").unwrap();
        assert!(outputs_fresh(&id, "main.tex").unwrap());
        assert_eq!(engine_log(&id, "main.tex").unwrap(), "a\nb\nc");
        assert_eq!(log_tail(&id, "main.tex", 2).unwrap(), "b\nc");

        assert_eq!(clean_outputs(&id, "main.tex").unwrap(), 2);
        assert!(!outputs_fresh(&id, "main.tex").unwrap());
        assert!(root.join("main.tex").is_file());
        let _ = std::fs::remove_dir_all(&o.outdir);
    }

    #[test]
    fn outputs_refuse_paths_outside_the_root() {
        let (id, _) = project("esc");
        let mut bads = crate::test_scratch::escapes();
        bads.extend(["ch", "missing.tex", ""]);
        for bad in bads {
            assert!(outputs_fresh(&id, bad).is_err(), "{}", bad);
            assert!(clean_outputs(&id, bad).is_err(), "{}", bad);
            assert!(engine_log(&id, bad).is_err(), "{}", bad);
        }
        assert!(clean_outputs("nope", "main.tex").is_err());
    }

    #[test]
    fn output_stamp_tracks_the_pdf() {
        let (id, _dir) = project("stamp");
        assert_eq!(output_stamp(&id, "main.tex").unwrap(), None);
        let o = outputs_of(&id, "main.tex").unwrap();
        std::fs::create_dir_all(&o.outdir).unwrap();
        std::fs::write(o.outdir.join(&o.pdf_name), "%PDF-1").unwrap();
        let a = output_stamp(&id, "main.tex").unwrap().unwrap();
        assert_eq!(a.bytes, 6);
        std::fs::write(o.outdir.join(&o.pdf_name), "%PDF-1.7").unwrap();
        let b = output_stamp(&id, "main.tex").unwrap().unwrap();
        assert_ne!(a, b);
        assert!(output_stamp(&id, "../escape.tex").is_err());
        let _ = std::fs::remove_dir_all(&o.outdir);
    }

    #[test]
    fn output_pdf_appears_with_the_first_compile() {
        let (id, _dir) = project("pdfpath");
        assert_eq!(output_pdf(&id, "main.tex").unwrap(), None);
        let o = outputs_of(&id, "main.tex").unwrap();
        std::fs::create_dir_all(&o.outdir).unwrap();
        std::fs::write(o.outdir.join(&o.pdf_name), "%PDF-1").unwrap();
        let want = o.outdir.join(&o.pdf_name).to_string_lossy().to_string();
        assert_eq!(output_pdf(&id, "main.tex").unwrap(), Some(want));
        assert!(output_pdf(&id, "../escape.tex").is_err());
        let _ = std::fs::remove_dir_all(&o.outdir);
    }
}
