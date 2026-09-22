//! Shared core: guard validation, outdir derivation, sidecar
//! resolution, fs ops, compile orchestration, synctex arg-building,
//! document structure over the file graph.

pub mod compile;
pub mod fs;
pub mod outputs;
pub mod structure;
pub mod synctex;

pub use compile::{cancel as cancel_job, poll as poll_job, run as run_job, JobRecord, JobStatus};
pub use fs::{
    grant_project, grant_root, grant_untitled, list_dir, read_text, resolve_in, resolve_read,
    session_root, trash_file, undo_trash,
};
pub use outputs::{
    clean_outputs, engine_log, log_file, log_tail, outputs_fresh, outputs_of, write_engine_log,
};
pub use synctex::{forward, inverse, ForwardHit, InverseHit};

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

/// djb2 hex (8 chars) for per-project dir sharding.
pub fn hash_root(root: &str) -> String {
    let mut h: u32 = 5381;
    for b in root.bytes() {
        h = h.wrapping_mul(33).wrapping_add(b as u32);
    }
    format!("{:08x}", h)
}

/// App-local compile-output home for one project root.
pub fn out_dir_for(base: &Path, root: &str) -> PathBuf {
    base.join("maleficium-out").join(hash_root(root))
}

const APP_ID: &str = "com.ethan.tauri-app";

/// `$<xdg_var>/<app id>`, else `$HOME/<home_rel>/<app id>`, else the OS tmp
/// tree when neither resolves.
fn xdg_app_dir(xdg_var: &str, home_rel: &str) -> PathBuf {
    if let Ok(xdg) = std::env::var(xdg_var) {
        if !xdg.is_empty() {
            return PathBuf::from(xdg).join(APP_ID);
        }
    }
    if let Ok(home) = std::env::var("HOME") {
        if !home.is_empty() {
            return PathBuf::from(home).join(home_rel).join(APP_ID);
        }
    }
    std::env::temp_dir()
}

/// Base dir for all engine outputs: the OS app-cache dir.
pub fn out_base_dir() -> PathBuf {
    xdg_app_dir("XDG_CACHE_HOME", ".cache")
}

/// Scratch project for untitled documents: under the OS app-data dir.
pub fn untitled_dir() -> PathBuf {
    xdg_app_dir("XDG_DATA_HOME", ".local/share").join("maleficium-untitled")
}

/// Triple suffix matching the bundled `<name>-<triple>` binaries.
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

/// Where the engine writes for one main file: it compiles inside the file's
/// own directory, into an app-cache outdir keyed by that directory.
pub struct MainOutputs {
    pub dir: PathBuf,
    pub main_file: String,
    pub outdir: PathBuf,
    pub pdf_name: String,
}

/// Derive the outputs of a canonical main-file path.
pub fn main_outputs(main_canon: &Path) -> Result<MainOutputs, String> {
    let dir = main_canon
        .parent()
        .filter(|d| !d.as_os_str().is_empty())
        .ok_or_else(|| "no parent directory".to_string())?
        .to_path_buf();
    let main_file = main_canon
        .file_name()
        .map(|f| f.to_string_lossy().to_string())
        .ok_or_else(|| "no file name".to_string())?;
    let stem = main_file.strip_suffix(".tex").unwrap_or(&main_file);
    Ok(MainOutputs {
        outdir: out_dir_for(&out_base_dir(), &dir.to_string_lossy()),
        pdf_name: format!("{}.pdf", stem),
        dir,
        main_file,
    })
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

    #[test]
    fn out_base_dir_resolves_app_cache() {
        let base = out_base_dir();
        assert!(base.ends_with("com.ethan.tauri-app"));
    }

    #[test]
    fn untitled_scratch_is_an_app_data_session_root() {
        assert!(untitled_dir().ends_with("com.ethan.tauri-app/maleficium-untitled"));
        let (canon, id) = grant_untitled().unwrap();
        assert!(canon.is_dir());
        assert_eq!(session_root(&id).unwrap(), canon);
        assert!(!canon.starts_with(std::env::temp_dir()) || std::env::var("HOME").is_err());
    }

    #[test]
    fn main_outputs_key_by_directory() {
        let o = main_outputs(Path::new("/home/u/paper/sub/main.tex")).unwrap();
        assert_eq!(o.dir, PathBuf::from("/home/u/paper/sub"));
        assert_eq!(o.main_file, "main.tex");
        assert_eq!(o.pdf_name, "main.pdf");
        assert_eq!(o.outdir, out_dir_for(&out_base_dir(), "/home/u/paper/sub"));
        assert!(main_outputs(Path::new("main.tex")).is_err());
    }
}
