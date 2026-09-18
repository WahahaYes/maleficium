use std::process::Command;

use super::guard::{require_allowed, require_repo_path};

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
pub fn git_status(app: tauri::AppHandle, root: String) -> Result<String, String> {
    // Trust boundary (design §5): `root` becomes `git -C <root>`, so it must
    // sit inside the live fs scope (project grant). Verdict on these commands
    // belongs to 10-history — the guard applies regardless while they exist.
    let root_canon = require_allowed(&app, &root)?;
    let root_str = root_canon.to_string_lossy().to_string();
    let status = Command::new("git")
        .args(["-C", &root_str, "status", "--porcelain=v1", "-b"])
        .output()
        .map_err(|e| format!("git status failed: {}", e))?;
    if !status.status.success() {
        return Err("not a git repository".to_string());
    }
    let log = Command::new("git")
        .args(["-C", &root_str, "log", "--oneline", "-5"])
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
pub fn git_show_head(app: tauri::AppHandle, root: String, file: String) -> Result<String, String> {
    // Trust boundary (design §5): `root` as above; `file` becomes
    // `HEAD:<file>`, so it must be repo-relative with no `..`/absolute
    // escape (lexical check — HEAD-only paths need not exist on disk).
    let root_canon = require_allowed(&app, &root)?;
    let rel = require_repo_path(&file)?.to_string();
    let root_str = root_canon.to_string_lossy().to_string();
    let head = Command::new("git")
        .args(["-C", &root_str, "show", &format!("HEAD:{}", rel)])
        .output()
        .map_err(|e| format!("git show failed: {}", e))?;
    if !head.status.success() {
        let branch = git_branch(&root_str);
        let _ = branch;
        return Err("not a git repository or file not in HEAD".to_string());
    }
    Ok(String::from_utf8_lossy(&head.stdout).to_string())
}
