use std::process::Command;

use super::{synctex_path};

#[tauri::command]
pub fn forward_sync(pdf: String, tex: String, line: u32) -> Result<String, String> {
    // Forward (editor → PDF): run INSIDE the pdf's out dir with the bare pdf
    // name, mirroring inverse_sync. Rationale (verified against the bundled
    // sidecar 2026-09-14): with an absolute `-o` path the tool finds the
    // `.synctex.gz` but resolves the `-i` tag against the WRONG file table
    // (`No tag for <pdf>`); with CWD=outdir + bare name both the gz lookup
    // AND the tag resolution succeed. `tex` stays absolute — the gz stores
    // absolute Input paths, so absolute matches exactly.
    // Bundled sidecar (externalBin `binaries/synctex`) — no PATH fallback.
    let bin = synctex_path().ok_or_else(|| String::from("bundled synctex sidecar missing (src-tauri/binaries/)"))?;
    let pdf_path = std::path::Path::new(&pdf);
    let (dir, name) = match (pdf_path.parent(), pdf_path.file_name()) {
        (Some(d), Some(n)) if !d.as_os_str().is_empty() => (d.to_path_buf(), n.to_string_lossy().to_string()),
        _ => (std::path::PathBuf::from("."), pdf),
    };
    let output = Command::new(&bin)
        .current_dir(&dir)
        .args(["view", "-i", &format!("{}:1:{}", line, tex), "-o", &name])
        .output()
        .map_err(|e| format!("synctex sidecar failed: {}", e))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout.lines().rev().take(500).collect::<Vec<_>>().join("\n"))
}

#[tauri::command]
pub fn inverse_sync(synctex_dir: String, pdf_name: String, page: u32, x: f32, y: f32) -> Result<String, String> {
    // Inverse (PDF → editor): synctex resolves `<pdf>.synctex.gz` relative to CWD,
    // so run INSIDE the out dir and pass the bare pdf name. Finds
    // `<tmp>/maleficium-out/<hash>/hello.synctex.gz` next to the pdf (Tectonic
    // `--synctex` writes both there). Passing an absolute `-o` path fails: the
    // tool looks for the `.synctex.gz` next to CWD, not next to the pdf arg.
    // Bundled sidecar (externalBin `binaries/synctex`) — no PATH fallback.
    let bin = synctex_path().ok_or_else(|| String::from("bundled synctex sidecar missing (src-tauri/binaries/)"))?;
    let output = Command::new(&bin)
        .current_dir(&synctex_dir)
        .args(["edit", "-o", &format!("{}:{}:{}:{}", page, x, y, pdf_name)])
        .output()
        .map_err(|e| format!("synctex sidecar failed: {}", e))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout.lines().rev().take(500).collect::<Vec<_>>().join("\n"))
}
