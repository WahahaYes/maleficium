use std::process::Command;

use super::guard::{require_allowed, require_bare_filename};
use crate::core;

#[tauri::command]
pub fn forward_sync(
    app: tauri::AppHandle,
    pdf: String,
    tex: String,
    line: u32,
) -> Result<String, String> {
    // `pdf` is the absolute outdir path (engine output lives outside any
    // project root), so it validates as an existing pdf file, never against
    // the project grant. `tex` is the absolute visible source and must sit
    // inside the live fs scope. `line` is `u32`: type-checked already.
    let pdf_canon = core::canonical_out_pdf(&pdf)?;
    let tex_canon = require_allowed(&app, &tex)?;
    // Forward (editor → PDF): run inside the pdf's out dir with the bare pdf
    // name. An absolute `-o` path resolves the `-i` tag against the wrong
    // file table, so CWD=outdir + bare name. `tex` stays absolute — the gz
    // stores absolute Input paths, so absolute matches exactly.
    // Bundled sidecar — no PATH fallback.
    let bin = core::sidecar_path_for("synctex")
        .ok_or_else(|| String::from("bundled synctex sidecar missing (src-tauri/binaries/)"))?;
    let pdf_path: &std::path::Path = &pdf_canon;
    let (dir, name) = match (pdf_path.parent(), pdf_path.file_name()) {
        // Validated absolute pdf always has a parent + file name; the `_`
        // arm is unreachable (the match must be exhaustive — fail closed,
        // never CWD-relative).
        (Some(d), Some(n)) if !d.as_os_str().is_empty() => {
            (d.to_path_buf(), n.to_string_lossy().to_string())
        }
        _ => return Err("forbidden path (pdf has no parent)".to_string()),
    };
    let output = Command::new(&bin)
        .current_dir(&dir)
        .args([
            "view",
            "-i",
            &format!("{}:1:{}", line, tex_canon.to_string_lossy()),
            "-o",
            &name,
        ])
        .output()
        .map_err(|e| format!("synctex sidecar failed: {}", e))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout
        .lines()
        .rev()
        .take(500)
        .collect::<Vec<_>>()
        .join("\n"))
}

#[tauri::command]
pub fn inverse_sync(
    _app: tauri::AppHandle,
    synctex_dir: String,
    pdf_name: String,
    page: u32,
    x: f32,
    y: f32,
) -> Result<String, String> {
    // `synctex_dir` is the absolute outdir (engine output outside any
    // project root), so it validates as an existing outdir, never against
    // the project grant. `pdf_name` is interpolated into the `page:x:y:name`
    // tag, so it must be a bare filename — never a path.
    // `page`/`x`/`y` are typed already.
    let dir_canon = core::canonical_out_dir(&synctex_dir)?;
    let name = require_bare_filename(&pdf_name)?.to_string();
    // Inverse (PDF → editor): the tool resolves `<pdf>.synctex.gz` relative
    // to CWD, so run inside the out dir and pass the bare pdf name. An
    // absolute `-o` path fails: the `.synctex.gz` is looked up next to CWD,
    // not next to the pdf arg.
    // Bundled sidecar — no PATH fallback.
    let bin = core::sidecar_path_for("synctex")
        .ok_or_else(|| String::from("bundled synctex sidecar missing (src-tauri/binaries/)"))?;
    let output = Command::new(&bin)
        .current_dir(&dir_canon)
        .args(["edit", "-o", &format!("{}:{}:{}:{}", page, x, y, name)])
        .output()
        .map_err(|e| format!("synctex sidecar failed: {}", e))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout
        .lines()
        .rev()
        .take(500)
        .collect::<Vec<_>>()
        .join("\n"))
}
