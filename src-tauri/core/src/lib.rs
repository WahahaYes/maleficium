//! Shared core behind every adapter (desktop commands, MCP server): session
//! roots and fs ops, compile and engine outputs, SyncTeX, the project index,
//! search and replace, structure, history, templates, and export. No Tauri
//! dependency.

pub mod api;
pub mod compile;
pub mod engine;
pub mod eventlog;
pub mod export;
pub mod fs;
pub mod guard;
pub mod history;
pub mod index;
pub mod mainfile;
pub mod outputs;
pub mod readiness;
pub mod replace;
pub mod search;
pub mod snippet;
pub mod structure;
pub mod synctex;
pub mod templates;
#[cfg(any(test, feature = "test-support"))]
pub mod test_scratch;

pub use compile::{cancel as cancel_job, poll as poll_job, run as run_job, JobRecord, JobStatus};
pub use fs::{
    grant_project, grant_root, grant_untitled, list_dir, make_dir, read_bytes, read_text,
    remove_path, rename_path, resolve_in, resolve_read, resolve_write, save, session_root,
    stat_path, trash_file, undo_trash, write_bytes, FileStat,
};
pub use outputs::{
    clean_outputs, engine_log, log_file, log_tail, output_pdf, output_stamp, outputs_fresh,
    outputs_of, tex_log, tex_log_file, write_engine_log, OutputStamp,
};
pub use synctex::{forward, inverse, ForwardHit, InverseHit};

/// Session state one adapter owns: granted roots, compile jobs, live
/// indexes, and held replace plans. Clones share the same state, so a
/// worker thread can carry one. Each test builds its own.
#[derive(Clone, Default)]
pub struct Core(Arc<State>);

#[derive(Default)]
struct State {
    sessions: fs::Sessions,
    jobs: compile::Jobs,
    indexes: index::Indexes,
    plans: replace::Plans,
}

impl Core {
    pub(crate) fn sessions(&self) -> &fs::Sessions {
        &self.0.sessions
    }
    pub(crate) fn jobs(&self) -> &compile::Jobs {
        &self.0.jobs
    }
    pub(crate) fn indexes(&self) -> &index::Indexes {
        &self.0.indexes
    }
    pub(crate) fn plans(&self) -> &replace::Plans {
        &self.0.plans
    }
}

use std::path::{Path, PathBuf};
use std::process::Child;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

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

const APP_ID: &str = "io.github.wahahayes.maleficium";

/// `<base>/<app id>`, else the OS tmp tree when the OS reports no base.
/// `dirs` is what Tauri's `appDataDir()`/`appCacheDir()` resolve through,
/// so core and the webview agree on every OS: XDG dirs (else `~/.local/share`,
/// `~/.cache`) on Linux, `~/Library/...` on macOS, `%APPDATA%` and
/// `%LOCALAPPDATA%` on Windows.
fn app_dir(base: Option<PathBuf>) -> PathBuf {
    base.unwrap_or_else(std::env::temp_dir).join(APP_ID)
}

/// Base dir for all engine outputs: the OS app-cache dir.
pub fn out_base_dir() -> PathBuf {
    app_dir(dirs::cache_dir())
}

/// The OS app-data dir: state the app keeps about projects.
pub fn data_base_dir() -> PathBuf {
    app_dir(dirs::data_dir())
}

/// Scratch project for untitled documents: under the OS app-data dir.
pub fn untitled_dir() -> PathBuf {
    data_base_dir().join("maleficium-untitled")
}

/// Triple suffix of the bundled `<name>-<triple>` binaries for this host;
/// `None` where no engine is bundled.
pub fn sidecar_triple() -> Option<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => Some("x86_64-unknown-linux-gnu"),
        ("linux", "aarch64") => Some("aarch64-unknown-linux-musl"),
        ("macos", "aarch64") => Some("aarch64-apple-darwin"),
        ("macos", "x86_64") => Some("x86_64-apple-darwin"),
        ("windows", "x86_64") => Some("x86_64-pc-windows-msvc"),
        _ => None,
    }
}

/// A child process that opens no console window on Windows: the release app
/// has no console of its own, so each spawn would otherwise flash one.
pub fn quiet_command(program: impl AsRef<std::ffi::OsStr>) -> std::process::Command {
    #[allow(unused_mut)]
    let mut cmd = std::process::Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

/// Locate a bundled sidecar binary by name: next to the app, where the
/// bundler installs it without the triple suffix, else as
/// `binaries/<name>-<triple>` in the dev tree. Errors say whether this
/// platform has no engine at all or the binary was never fetched.
pub fn sidecar_path_for(name: &str) -> Result<PathBuf, String> {
    let triple = sidecar_triple().ok_or_else(|| {
        format!(
            "no bundled {} for {}/{}",
            name,
            std::env::consts::OS,
            std::env::consts::ARCH
        )
    })?;
    let exe = std::env::current_exe().ok();
    let dev = Path::new(env!("CARGO_MANIFEST_DIR")).join("../binaries");
    find_sidecar(exe.as_deref().and_then(Path::parent), &dev, name, triple).ok_or_else(|| {
        format!(
            "bundled {} sidecar missing ({}-{}): run scripts/fetch-sidecars.sh",
            name, name, triple
        )
    })
}

fn find_sidecar(
    app_dir: Option<&Path>,
    dev_dir: &Path,
    name: &str,
    triple: &str,
) -> Option<PathBuf> {
    let ext = if cfg!(windows) { ".exe" } else { "" };
    let bundled = app_dir.map(|d| d.join(format!("{name}{ext}")));
    let dev = dev_dir.join(format!("{name}-{triple}{ext}"));
    bundled.into_iter().chain([dev]).find(|p| p.is_file())
}

/// Entry kind for directory listings.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
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
    fn sidecar_triple_names_this_host() {
        // CI and dev hosts are all bundled platforms.
        assert!(sidecar_triple().is_some_and(|t| !t.is_empty()));
    }

    #[test]
    fn sidecar_found_next_to_app_without_triple() {
        let app = crate::test_scratch::dir("sidecar-app");
        std::fs::create_dir_all(&app).unwrap();
        let dev = crate::test_scratch::dir("sidecar-dev");
        std::fs::create_dir_all(&dev).unwrap();
        let ext = if cfg!(windows) { ".exe" } else { "" };
        let installed = app.join(format!("maleficium-tectonic{ext}"));
        std::fs::write(&installed, b"").unwrap();
        std::fs::write(dev.join(format!("maleficium-tectonic-t{ext}")), b"").unwrap();
        let found = find_sidecar(Some(&app), &dev, "maleficium-tectonic", "t");
        assert_eq!(found, Some(installed));
    }

    #[test]
    fn sidecar_falls_back_to_dev_tree() {
        let app = crate::test_scratch::dir("sidecar-app-empty");
        std::fs::create_dir_all(&app).unwrap();
        let dev = crate::test_scratch::dir("sidecar-dev-only");
        std::fs::create_dir_all(&dev).unwrap();
        let ext = if cfg!(windows) { ".exe" } else { "" };
        let fetched = dev.join(format!("maleficium-synctex-t{ext}"));
        std::fs::write(&fetched, b"").unwrap();
        assert_eq!(
            find_sidecar(Some(&app), &dev, "maleficium-synctex", "t"),
            Some(fetched)
        );
        assert_eq!(
            find_sidecar(Some(&app), &dev, "maleficium-tectonic", "t"),
            None
        );
    }

    #[test]
    fn missing_sidecar_names_the_fix() {
        let err = sidecar_path_for("no-such-sidecar").unwrap_err();
        assert!(err.contains("fetch-sidecars.sh"), "{err}");
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
        assert!(base.ends_with("io.github.wahahayes.maleficium"));
    }

    #[test]
    fn untitled_scratch_is_an_app_data_session_root() {
        let cx = &Core::default();
        assert!(untitled_dir().ends_with("io.github.wahahayes.maleficium/maleficium-untitled"));
        let (canon, id) = grant_untitled(cx).unwrap();
        assert!(canon.is_dir());
        assert_eq!(session_root(cx, &id).unwrap(), canon);
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
