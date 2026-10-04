//! The process environment latexml needs, shared by `convert` and `dump`.
//!
//! `prepare` makes a private scratch directory and points the process at it:
//! a `kpsewhich` link to this executable first on `PATH` (and in `KPSEWHICH`),
//! so latexml's subprocess kpathsea backend resolves every file through
//! `kpsewhich.rs`; `TMPDIR`/`TEMP`/`TMP` at a writable directory (without one
//! `latexml::api::convert_to_html` silently skips post-processing and returns
//! raw LaTeXML XML); and the settings the `kpsewhich` mode reads. The scratch
//! directory is removed when the `Session` drops.

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// Tauri's `productName`: the directory the Linux packages install resources
/// under. A unit test keeps it equal to `tauri.conf.json`.
const PRODUCT_NAME: &str = "Maleficium";

/// Scratch directories older than this are leftovers of a killed run.
const STALE_AFTER: Duration = Duration::from_secs(24 * 60 * 60);

#[derive(Default)]
pub struct Options {
    /// The bundle every file resolves against (`--bundle`, else the
    /// `MALEFICIUM_BUNDLE_URL` variable).
    pub bundle: Option<String>,
    /// Tectonic's cache directory (`--cache`, else `TECTONIC_CACHE_DIR`, else
    /// Tectonic's per-user default).
    pub cache: Option<PathBuf>,
    /// Never fetch (`-C`, or `MALEFICIUM_CACHED_ONLY=1`).
    pub cached_only: bool,
}

pub struct Session {
    pub scratch: PathBuf,
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.scratch);
    }
}

/// Where the packaged app keeps its resources, relative to the engine
/// executable (Tauri's `resource_dir`; the engine sits beside the app).
/// Verified on Linux only; the macOS and Windows layouts are read from
/// `tauri-utils` and unverified.
fn resource_dir(exe: &Path) -> Option<PathBuf> {
    let exe_dir = exe.parent()?;
    if cfg!(target_os = "linux") {
        Some(exe_dir.join("..").join("lib").join(PRODUCT_NAME))
    } else if cfg!(target_os = "macos") {
        Some(exe_dir.join("..").join("Resources"))
    } else {
        Some(exe_dir.to_path_buf())
    }
}

/// The directory of latexml format dumps: `--dumps`, else
/// `MALEFICIUM_DUMP_DIR`, else `resources/dumps` in the app's resource
/// directory.
pub fn dump_dir(arg: Option<PathBuf>) -> Result<PathBuf, String> {
    let chosen = arg
        .or_else(|| {
            std::env::var_os("MALEFICIUM_DUMP_DIR")
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
        })
        .or_else(|| {
            let exe = std::env::current_exe().ok()?;
            Some(resource_dir(&exe)?.join("resources").join("dumps"))
        });
    match chosen {
        Some(d) if d.is_dir() => {
            // Absolute: convert changes into the input's directory.
            std::path::absolute(&d).map_err(|e| format!("bad dumps path: {e}"))
        }
        Some(d) => Err(format!(
            "no format dumps at {}; pass --dumps or set MALEFICIUM_DUMP_DIR",
            d.display()
        )),
        None => Err("no format dumps; pass --dumps or set MALEFICIUM_DUMP_DIR".into()),
    }
}

pub fn prepare(o: &Options) -> Result<Session, String> {
    let bundle = o
        .bundle
        .clone()
        .or_else(|| std::env::var("MALEFICIUM_BUNDLE_URL").ok())
        .filter(|b| !b.is_empty())
        .ok_or("no bundle: pass --bundle URL")?;
    if let Some(cache) = &o.cache {
        let cache = std::path::absolute(cache).map_err(|e| format!("bad --cache: {e}"))?;
        fs::create_dir_all(&cache)
            .map_err(|e| format!("cannot create {}: {e}", cache.display()))?;
        std::env::set_var("TECTONIC_CACHE_DIR", cache);
    }
    let cached_only =
        o.cached_only || std::env::var("MALEFICIUM_CACHED_ONLY").as_deref() == Ok("1");

    // Under the cache directory when there is one (app-owned and certainly
    // writable), else the system temp dir.
    let root = match std::env::var_os("TECTONIC_CACHE_DIR").filter(|v| !v.is_empty()) {
        Some(cache) => PathBuf::from(cache).join("scratch"),
        None => std::env::temp_dir().join("maleficium-scratch"),
    };
    fs::create_dir_all(&root).map_err(|e| format!("cannot create {}: {e}", root.display()))?;
    prune(&root);
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let scratch = std::path::absolute(root.join(format!("{}-{nanos}", std::process::id())))
        .map_err(|e| format!("bad scratch path: {e}"))?;
    let session = Session {
        scratch: scratch.clone(),
    };
    let (bin, tmp) = (scratch.join("bin"), scratch.join("tmp"));
    for d in [&bin, &tmp] {
        fs::create_dir_all(d).map_err(|e| format!("cannot create {}: {e}", d.display()))?;
    }
    private(&scratch);

    let exe = std::env::current_exe().map_err(|e| format!("cannot find the engine: {e}"))?;
    let link = bin.join(if cfg!(windows) {
        "kpsewhich.exe"
    } else {
        "kpsewhich"
    });
    link_executable(&exe, &link)?;

    let mut path: Vec<PathBuf> = vec![bin];
    path.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    let path: OsString = std::env::join_paths(path).map_err(|e| format!("bad PATH: {e}"))?;
    std::env::set_var("PATH", path);
    std::env::set_var("KPSEWHICH", &link);
    for var in ["TMPDIR", "TEMP", "TMP"] {
        std::env::set_var(var, &tmp);
    }
    std::env::set_var("MALEFICIUM_SCRATCH", &scratch);
    std::env::set_var("MALEFICIUM_BUNDLE_URL", bundle);
    std::env::set_var(
        "MALEFICIUM_CACHED_ONLY",
        if cached_only { "1" } else { "0" },
    );
    Ok(session)
}

/// A symlink to the engine on unix; a copy on Windows, where creating a
/// symlink needs a privilege.
fn link_executable(exe: &Path, link: &Path) -> Result<(), String> {
    #[cfg(unix)]
    let made = std::os::unix::fs::symlink(exe, link);
    #[cfg(not(unix))]
    let made = fs::copy(exe, link).map(|_| ());
    made.map_err(|e| format!("cannot link {} to {}: {e}", link.display(), exe.display()))
}

/// Owner-only, so another user cannot plant files the engine will read.
fn private(dir: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(dir, fs::Permissions::from_mode(0o700));
    }
    #[cfg(not(unix))]
    let _ = dir;
}

/// Removes scratch directories a killed run left behind: ones older than
/// `STALE_AFTER`, and ones whose process id no longer runs. A live run's
/// directory is never touched, whatever its age.
fn prune(root: &Path) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for e in entries.flatten() {
        if stale(&e.path()) {
            let _ = fs::remove_dir_all(e.path());
        }
    }
}

/// Whether a scratch directory is a leftover. Its name is `<pid>-<nanos>`
/// (see `prepare`): when the pid parses and that process is gone, the run
/// that owned the directory is dead, so it goes regardless of age. A live
/// pid's directory, and any unfamiliar name, stays; age over `STALE_AFTER`
/// catches the rest (an unparseable name, a clock that jumped).
fn stale(dir: &Path) -> bool {
    if let Some(name) = dir.file_name().and_then(|n| n.to_str()) {
        let pid = name.split('-').next().and_then(|p| p.parse::<u32>().ok());
        if let Some(pid) = pid {
            if !process_alive(pid) {
                return true;
            }
        }
    }
    dir.metadata()
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.elapsed().ok())
        .is_some_and(|age| age > STALE_AFTER)
}

/// Whether a process id runs now. A `/proc` entry on Linux; on other
/// systems every id reads as alive, so only the age rule sweeps. Pid reuse
/// can only keep a dead run's directory longer (its recycled pid reads
/// alive): it can never delete a live run's, whose pid always reads alive.
fn process_alive(pid: u32) -> bool {
    #[cfg(target_os = "linux")]
    {
        fs::metadata(format!("/proc/{pid}")).is_ok()
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = pid;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sweep_root(tag: &str) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("maleficium-sweep-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        root
    }

    /// A pid that ran and was reaped: dead for the sweep's purposes (pid
    /// reuse between the reap and the check is the only flake, and would
    /// need the OS to recycle the pid within microseconds).
    #[cfg(target_os = "linux")]
    fn dead_pid_by_spawn() -> u32 {
        let mut child = std::process::Command::new("true")
            .spawn()
            .expect("spawn true");
        let pid = child.id();
        let status = child.wait().expect("reap true");
        assert!(status.success());
        assert!(
            fs::metadata(format!("/proc/{pid}")).is_err(),
            "the reaped child lingers"
        );
        pid
    }

    #[test]
    fn a_live_pid_survives_on_proc() {
        assert!(process_alive(std::process::id()));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn prune_removes_a_dead_run_and_keeps_a_live_one() {
        let root = sweep_root("pid");
        let live = root.join(format!("{}-1", std::process::id()));
        let dead = root.join(format!("{}-1", dead_pid_by_spawn()));
        let strange = root.join("not-a-scratch-dir");
        for d in [&live, &dead, &strange] {
            fs::create_dir_all(d).unwrap();
            fs::write(d.join("x"), "x").unwrap();
        }
        prune(&root);
        assert!(live.is_dir(), "a live run's directory stays");
        assert!(strange.is_dir(), "an unfamiliar name stays");
        assert!(!dead.exists(), "a killed run's directory goes");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn prune_ignores_a_missing_root() {
        prune(&std::env::temp_dir().join("maleficium-no-such-sweep-root"));
    }

    #[test]
    fn product_name_matches_the_tauri_config() {
        let conf = include_str!("../../tauri.conf.json");
        assert!(
            conf.contains(&format!("\"productName\": \"{PRODUCT_NAME}\"")),
            "PRODUCT_NAME drifted from tauri.conf.json"
        );
    }

    #[test]
    fn dump_dir_prefers_the_argument_and_reports_a_missing_directory() {
        let here = std::env::temp_dir();
        assert_eq!(dump_dir(Some(here.clone())).unwrap(), here);
        let missing = here.join("maleficium-no-such-dumps");
        let err = dump_dir(Some(missing.clone())).unwrap_err();
        assert!(err.contains(&missing.display().to_string()), "{err}");
    }
}
