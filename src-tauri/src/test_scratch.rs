//! Scratch dirs for tests: one root per test process under the OS temp dir,
//! removed when the process exits. Roots left by processes that died without
//! exiting cleanly are swept on first use.

use std::path::PathBuf;
use std::sync::Once;

const PREFIX: &str = "maleficium-cargo-test-";

static INIT: Once = Once::new();

fn root_of(pid: u32) -> PathBuf {
    std::env::temp_dir().join(format!("{PREFIX}{pid}"))
}

extern "C" fn remove_root() {
    let _ = std::fs::remove_dir_all(root_of(std::process::id()));
}

/// Remove roots whose process is gone.
fn sweep() {
    let Ok(entries) = std::fs::read_dir(std::env::temp_dir()) else {
        return;
    };
    for e in entries.flatten() {
        let name = e.file_name();
        let Some(pid) = name
            .to_str()
            .and_then(|n| n.strip_prefix(PREFIX))
            .and_then(|p| p.parse::<u32>().ok())
        else {
            continue;
        };
        if pid != std::process::id() && !alive(pid) {
            let _ = std::fs::remove_dir_all(e.path());
        }
    }
}

#[cfg(target_os = "linux")]
fn alive(pid: u32) -> bool {
    std::path::Path::new(&format!("/proc/{pid}")).exists()
}

#[cfg(not(target_os = "linux"))]
fn alive(pid: u32) -> bool {
    // SAFETY: signal 0 only checks that the process exists.
    unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
}

/// A path for `name` inside this process's scratch root. The dir itself is
/// not created; callers set it up as they need.
pub fn dir(name: &str) -> PathBuf {
    INIT.call_once(|| {
        sweep();
        let _ = std::fs::create_dir_all(root_of(std::process::id()));
        // SAFETY: registers a plain extern "C" fn with no captured state.
        unsafe {
            libc::atexit(remove_root);
        }
    });
    root_of(std::process::id()).join(name)
}
