use std::process::Command;

#[tauri::command]
pub fn git_status(root: String) -> Result<String, String> {
    let status = Command::new("git")
        .args(["-C", &root, "status", "--porcelain=v1", "-b"])
        .output()
        .map_err(|e| format!("git status failed: {}", e))?;
    if !status.status.success() {
        return Err("not a git repository".to_string());
    }
    let log = Command::new("git")
        .args(["-C", &root, "log", "--oneline", "-5"])
        .output()
        .ok();
    let status_str = String::from_utf8_lossy(&status.stdout).to_string();
    let log_str = log
        .filter(|l| l.status.success())
        .map(|l| String::from_utf8_lossy(&l.stdout).into_owned())
        .unwrap_or_default();
    let result = if log_str.is_empty() {
        status_str
    } else {
        format!("{}\n{}", status_str, log_str)
    };
    Ok(result)
}