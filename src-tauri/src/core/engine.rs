//! The Tectonic engine: the pinned bundle, the app-owned cache it resolves
//! into, and the one way a compile spawns the sidecar and pumps its output.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;

use maleficium_events::{CompileLine, CompileStream};

use super::{JobOutcome, JobStatus, MainOutputs};

/// The bundle every compile resolves against: an immutable direct URL, never
/// the relay redirect, so an upstream rotation cannot move it.
pub const BUNDLE_URL: &str = "https://data1b.fullyjustified.net/tlextras-2022.0r0.tar";

/// SHA-256 digest `BUNDLE_URL` must resolve to.
pub const BUNDLE_DIGEST: &str = "6ffe055852f8faf66c0acbe1a7fb27f87b869a90bad1204f3bf4d9683f597c7c";

/// The engine's cache for the pinned bundle: app-owned, beside the output
/// shards, one directory per digest so its size can be reported and cleared.
pub fn cache_dir() -> PathBuf {
    super::out_base_dir()
        .join("maleficium-tectonic")
        .join(BUNDLE_DIGEST)
}

/// The engine's file name for a bundle URL: every byte outside
/// `[A-Za-z0-9._-]` becomes `,<decimal byte>,`.
fn url_key(url: &str) -> String {
    let mut out = String::with_capacity(url.len() + 16);
    for b in url.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_') {
            out.push(b as char);
        } else {
            out.push_str(&format!(",{},", b));
        }
    }
    out
}

/// The digest the engine recorded when it resolved `url` into `cache`, if it
/// ever did.
pub fn resolved_digest(cache: &Path, url: &str) -> Option<String> {
    let text =
        std::fs::read_to_string(cache.join("bundles").join("hashes").join(url_key(url))).ok()?;
    let digest = text.trim();
    (!digest.is_empty()).then(|| digest.to_string())
}

/// How the cache stands against the pin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DigestCheck {
    /// The bundle was never resolved into this cache.
    Unresolved,
    Pinned,
    /// The URL now resolves to a different digest.
    Changed(String),
}

pub fn check_digest(cache: &Path) -> DigestCheck {
    match resolved_digest(cache, BUNDLE_URL) {
        None => DigestCheck::Unresolved,
        Some(d) if d == BUNDLE_DIGEST => DigestCheck::Pinned,
        Some(d) => DigestCheck::Changed(d),
    }
}

/// The engine's command line for one main file.
fn command(out: &MainOutputs, cache: &Path) -> Result<Command, String> {
    let mut cmd = Command::new(super::sidecar_path_for("tectonic")?);
    cmd.args(["-X", "compile", &out.main_file, "--outdir"])
        .arg(&out.outdir)
        .args(["--synctex", "-b", BUNDLE_URL])
        .env("TECTONIC_CACHE_DIR", cache)
        .current_dir(&out.dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    Ok(cmd)
}

/// One engine run: how it ended, every line it printed in arrival order,
/// and how the cache stood against the pin afterwards.
pub struct Attempt {
    pub status: JobStatus,
    pub lines: Vec<CompileLine>,
    pub bundle: DigestCheck,
}

impl Attempt {
    pub fn texts(&self) -> Vec<String> {
        self.lines.iter().map(|l| l.text.clone()).collect()
    }
}

/// Spawn the engine, park the child in `slot` (whoever takes it from there
/// owns reaping it: a cancel, or this call once output ends), hand each line
/// to `on_line` as it arrives, then wait up to `timeout_secs`.
pub fn run(
    out: &MainOutputs,
    slot: &Mutex<Option<Child>>,
    timeout_secs: u64,
    on_line: &mut dyn FnMut(&CompileLine),
) -> Result<Attempt, String> {
    let cache = cache_dir();
    std::fs::create_dir_all(&cache).map_err(|e| format!("engine cache unreachable: {}", e))?;
    std::fs::create_dir_all(&out.outdir).map_err(|e| format!("outdir unreachable: {}", e))?;
    let mut child = command(out, &cache)?
        .spawn()
        .map_err(|e| format!("bundled tectonic spawn failed: {}", e))?;
    let (Some(stdout), Some(stderr)) = (child.stdout.take(), child.stderr.take()) else {
        let _ = child.kill();
        let _ = child.wait();
        return Err(String::from(
            "bundled tectonic spawn failed: sidecar pipes unavailable",
        ));
    };
    slot.lock().unwrap().replace(child);

    let (tx, rx) = std::sync::mpsc::channel::<CompileLine>();
    let pump = |pipe: Box<dyn std::io::Read + Send>, stream: CompileStream| {
        let tx = tx.clone();
        std::thread::spawn(move || {
            use std::io::{BufRead, BufReader};
            for text in BufReader::new(pipe).lines().map_while(Result::ok) {
                if tx.send(CompileLine { stream, text }).is_err() {
                    break;
                }
            }
        })
    };
    let readers = [
        pump(Box::new(stdout), CompileStream::Stdout),
        pump(Box::new(stderr), CompileStream::Stderr),
    ];
    drop(tx);
    let mut lines = Vec::new();
    for line in rx {
        on_line(&line);
        lines.push(line);
    }
    for r in readers {
        let _ = r.join();
    }

    let taken = slot.lock().unwrap().take();
    let status = match taken {
        None => JobStatus::Cancelled,
        Some(c) => match super::wait_for_child(c, timeout_secs) {
            JobOutcome::Cancelled => JobStatus::Cancelled,
            JobOutcome::TimedOut => JobStatus::TimedOut,
            JobOutcome::Exited(s) if s.success() => JobStatus::Success,
            JobOutcome::Exited(_) => JobStatus::Failed,
        },
    };
    Ok(Attempt {
        status,
        lines,
        bundle: check_digest(&cache),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_key_matches_the_engine_cache_layout() {
        assert_eq!(
            url_key(BUNDLE_URL),
            "https,58,,47,,47,data1b.fullyjustified.net,47,tlextras-2022.0r0.tar"
        );
    }

    #[test]
    fn cache_is_app_owned_and_keyed_by_digest() {
        let c = cache_dir();
        assert!(c.starts_with(super::super::out_base_dir()));
        assert!(c.ends_with(format!("maleficium-tectonic/{BUNDLE_DIGEST}")));
    }

    fn cache_with(digest: Option<&str>) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "maleficium-engine-{}-{}",
            std::process::id(),
            digest.unwrap_or("none").get(..8).unwrap_or("none")
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let hashes = dir.join("bundles").join("hashes");
        std::fs::create_dir_all(&hashes).unwrap();
        if let Some(d) = digest {
            std::fs::write(hashes.join(url_key(BUNDLE_URL)), format!("{d}\n")).unwrap();
        }
        dir
    }

    #[test]
    fn digest_check_reads_what_the_engine_resolved() {
        assert_eq!(check_digest(&cache_with(None)), DigestCheck::Unresolved);
        assert_eq!(
            check_digest(&cache_with(Some(BUNDLE_DIGEST))),
            DigestCheck::Pinned
        );
        let other = "0".repeat(64);
        assert_eq!(
            check_digest(&cache_with(Some(&other))),
            DigestCheck::Changed(other)
        );
    }

    #[test]
    fn command_pins_the_bundle_and_the_cache() {
        let out = super::super::main_outputs(Path::new("/home/u/paper/main.tex")).unwrap();
        let cache = Path::new("/c");
        let cmd = command(&out, cache).unwrap();
        let args: Vec<String> = cmd
            .get_args()
            .map(|a| a.to_string_lossy().to_string())
            .collect();
        let b = args.iter().position(|a| a == "-b").unwrap();
        assert_eq!(args[b + 1], BUNDLE_URL);
        let env: Vec<_> = cmd.get_envs().collect();
        assert!(env
            .iter()
            .any(|(k, v)| *k == "TECTONIC_CACHE_DIR" && *v == Some(cache.as_os_str())));
        assert_eq!(cmd.get_current_dir(), Some(Path::new("/home/u/paper")));
    }
}
