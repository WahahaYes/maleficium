//! Shared Rust core: guard validation, outdir derivation, sidecar
//! resolution, fs ops, compile orchestration, synctex arg-building.
//!
//! The Tauri commands (`commands/`) and the MCP tools (`mcp/`) are thin
//! adapters over these functions — identical logic, two transports.

pub mod compile;
pub mod fs;
pub mod synctex;

pub use compile::{cancel as cancel_job, poll as poll_job, run as run_job, JobRecord, JobStatus};
pub use fs::{
    grant_root, list_dir, log_tail, read_text, resolve_in, resolve_read, session_root, trash_file,
    undo_trash,
};
pub use synctex::{forward_query, inverse_query};

use std::path::{Path, PathBuf};
use std::process::Child;
use std::time::{Duration, Instant};

/// How a waited child resolved: exited (with status), killed after the
/// hang-guard timeout, or taken by cancel before the wait.
pub enum JobOutcome {
    Exited(std::process::ExitStatus),
    TimedOut,
    Cancelled,
}

/// Own a child end-to-end: block on exit, kill + reap after `timeout_secs`.
/// Shared by the Tauri command (Cancel races via the state slot) and the MCP
/// job worker below.
pub fn wait_for_child(mut child: Child, timeout_secs: u64) -> JobOutcome {
    let deadline = Instant::now() + Duration::from_secs(timeout_secs);
    loop {
        match child.try_wait() {
            Ok(Some(s)) => return JobOutcome::Exited(s),
            Ok(None) => {}
            Err(_) => return JobOutcome::TimedOut,
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            match child.wait() {
                Ok(s) => {
                    if s.success() {
                        return JobOutcome::Exited(s);
                    }
                    return JobOutcome::TimedOut;
                }
                Err(_) => return JobOutcome::TimedOut,
            }
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// djb2 hex (8 chars) — mirrors `src/lib/paths.ts hashRoot`.
pub fn hash_root(root: &str) -> String {
    let mut h: u32 = 5381;
    for b in root.bytes() {
        h = h.wrapping_mul(33).wrapping_add(b as u32);
    }
    format!("{:08x}", h)
}

/// App-local compile-output home for one project root.
pub fn out_dir_for(tmp: &Path, root: &str) -> PathBuf {
    tmp.join("maleficium-out").join(hash_root(root))
}

/// Triple suffix matching `src-tauri/binaries/<name>-<triple>`.
pub fn sidecar_triple() -> &'static str {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => "x86_64-unknown-linux-gnu",
        ("linux", "aarch64") => "aarch64-unknown-linux-musl",
        ("macos", "aarch64") => "aarch64-apple-darwin",
        ("macos", "x86_64") => "x86_64-apple-darwin",
        ("windows", "x86_64") => "x86_64-pc-windows-msvc",
        _ => "x86_64-unknown-linux-gnu",
    }
}

/// Locate a bundled sidecar binary by name.
pub fn sidecar_path_for(name: &str) -> Option<PathBuf> {
    let triple = sidecar_triple();
    let exe_name = if cfg!(windows) {
        format!("{}-{}.exe", name, triple)
    } else {
        format!("{}-{}", name, triple)
    };
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let p = dir.join(&exe_name);
            if p.exists() {
                return Some(p);
            }
        }
    }
    let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("binaries")
        .join(&exe_name);
    if dev.exists() {
        return Some(dev);
    }
    None
}

/// Entry kind for directory listings.
#[derive(Debug, Clone)]
pub struct FileEntry {
    pub name: String,
    pub entry_type: String,
}

/// Names never shown in listings (build artifacts + trash).
pub fn is_hidden_name(name: &str) -> bool {
    const EXACT: &[&str] = &[".git", ".maleficium-trash", "out"];
    const SUFFIX: &[&str] = &[".aux", ".log", ".fls", ".fdb_latexmk", ".synctex.gz"];
    if EXACT.contains(&name) {
        return true;
    }
    if name.starts_with('.') && name != ".maleficium.json" && name != ".gitignore" {
        return true;
    }
    SUFFIX.iter().any(|s| name.ends_with(s))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_root_is_deterministic_8_hex() {
        assert_eq!(hash_root("/home/u/paper"), hash_root("/home/u/paper"));
        assert_ne!(hash_root("/home/u/other"), hash_root("/home/u/paper"));
        let h = hash_root("/home/u/paper");
        assert_eq!(h.len(), 8);
        assert!(h.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn out_dir_shards_tmp_per_root() {
        let a = out_dir_for(Path::new("/tmp"), "/home/u/paper");
        let b = out_dir_for(Path::new("/tmp"), "/home/u/other");
        assert_ne!(a, b);
        assert!(a.starts_with("/tmp/maleficium-out"));
        assert!(!a.starts_with("/home/u/paper"));
    }

    #[test]
    fn sidecar_triple_returns_non_empty() {
        assert!(!sidecar_triple().is_empty());
    }

    #[test]
    fn hidden_names_skip_artifacts() {
        assert!(is_hidden_name(".git"));
        assert!(is_hidden_name("x.aux"));
        assert!(!is_hidden_name("main.tex"));
    }
}
