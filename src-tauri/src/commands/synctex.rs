use std::process::Command;

#[tauri::command]
pub fn forward_sync(pdf: String, tex: String, line: u32) -> Result<String, String> {
    let output = Command::new("synctex")
        .args(["view", "-i", &format!("{}:1:{}", line, tex), "-o", &pdf])
        .output()
        .map_err(|_| "synctex not installed".to_string())?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout.lines().rev().take(500).collect::<Vec<_>>().join("\n"))
}

#[tauri::command]
pub fn inverse_sync(pdf: String, page: u32, x: f32, y: f32) -> Result<String, String> {
    let output = Command::new("synctex")
        .args(["edit", "-o", &format!("{}:{}:{}:{}", page, x, y, pdf)])
        .output()
        .map_err(|_| "synctex not installed".to_string())?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout.lines().rev().take(500).collect::<Vec<_>>().join("\n"))
}