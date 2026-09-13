use std::process::Command;

fn git_branch(root: &str) -> Option<String> {
    let out = Command::new("git")
        .args(["-C", root, "rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let b = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if b.is_empty() || b == "HEAD" {
        None
    } else {
        Some(b)
    }
}

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

/// Working-copy vs HEAD diff for one file (honest empty on non-repo/binary).
#[tauri::command]
pub fn git_show_head(root: String, file: String) -> Result<String, String> {
    let head = Command::new("git")
        .args(["-C", &root, "show", &format!("HEAD:{}", file)])
        .output()
        .map_err(|e| format!("git show failed: {}", e))?;
    if !head.status.success() {
        let branch = git_branch(&root);
        let _ = branch;
        return Err("not a git repository or file not in HEAD".to_string());
    }
    Ok(String::from_utf8_lossy(&head.stdout).to_string())
}