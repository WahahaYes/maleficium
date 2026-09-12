use std::process::Command;

#[tauri::command]
pub fn compile_tex(input: String, workdir: String) -> Result<String, String> {
    let bin = "tectonic";
    let out = Command::new(bin)
        .arg(&input)
        .current_dir(&workdir)
        .output()
        .map_err(|e| format!("spawn failed {}: {}", bin, e))?;
    if out.status.success() {
        Ok(format!("compiled {} ({} bytes out)", input, out.stdout.len()))
    } else {
        let stderr = String::from_utf8_lossy(&out.stderr);
        let truncated = &stderr[..500.min(stderr.len())];
        Err(format!("tectonic failed: {}", truncated))
    }
}