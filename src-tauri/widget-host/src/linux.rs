//! The Linux backend: one `maleficium-widget-helper` process per live widget,
//! embedded as an X11 child of the editor window, talking over an inherited
//! socket. Each helper runs in its own process group with a parent-death
//! signal, inside an empty network namespace where unprivileged user
//! namespaces are available, and is tagged in its environment so every
//! process it owns can be found and counted.

use crate::caps::{Capabilities, EgressContainment, Embedding, ProcessIsolation, WatchdogSignal};
use crate::host::{BackendEvent, WidgetBackend, WidgetSpec};
use crate::proto::{content_frames, encode, Decoder, FromHelper, Placement, ToHelper, PROTOCOL};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::time::Duration;

/// The environment variable that tags a helper's processes.
pub const TAG_VAR: &str = "MALEFICIUM_WIDGET_TAG";

#[derive(Debug, Clone)]
pub struct LinuxConfig {
    /// The helper binary, and the arguments that select the helper in it.
    pub helper: PathBuf,
    pub helper_args: Vec<String>,
    /// The editor window's X11 id.
    pub parent: u64,
    /// Use an empty network namespace where the system allows one; off
    /// runs the dead-proxy fallback.
    pub network_namespaces: bool,
}

enum Wire {
    Msg(FromHelper),
    Violation(String),
    Eof,
}

struct Proc {
    gen: u64,
    child: Child,
    sock: UnixStream,
}

pub struct LinuxBackend {
    cfg: LinuxConfig,
    netns: bool,
    session: String,
    procs: BTreeMap<String, Proc>,
    /// The tag of each widget's latest launch, kept after it stops.
    tags: BTreeMap<String, String>,
    gen: u64,
    tx: Sender<(String, u64, Wire)>,
    rx: Receiver<(String, u64, Wire)>,
}

/// Whether helpers can start in an empty network namespace here: the
/// display must be a local socket (an X server over TCP would be cut off)
/// and an unprivileged user namespace must be allowed.
pub fn netns_available() -> bool {
    let local = std::env::var("DISPLAY").is_ok_and(|d| d.starts_with(':'));
    local
        && Command::new("unshare")
            .args(["--user", "--map-root-user", "--net", "true"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
}

impl LinuxBackend {
    pub fn new(cfg: LinuxConfig) -> Self {
        let (tx, rx) = channel();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        Self {
            netns: cfg.network_namespaces && netns_available(),
            session: format!("wh{}x{nanos}", std::process::id()),
            procs: BTreeMap::new(),
            tags: BTreeMap::new(),
            gen: 0,
            tx,
            rx,
            cfg,
        }
    }

    /// The tag prefix every process of this backend carries.
    pub fn session(&self) -> &str {
        &self.session
    }

    fn write(&mut self, id: &str, msg: &ToHelper) -> Result<(), String> {
        let p = self.procs.get_mut(id).ok_or("no helper")?;
        p.sock
            .write_all(&encode(msg))
            .map_err(|e| format!("write to the helper: {e}"))
    }
}

/// Pids whose environment carries `TAG_VAR=<tag>` (exactly, or as a prefix
/// when `prefix`), not counting zombies.
pub fn tagged(tag: &str, prefix: bool) -> Vec<u32> {
    let needle = format!("{TAG_VAR}={tag}");
    let mut out = Vec::new();
    let Ok(dir) = std::fs::read_dir("/proc") else {
        return out;
    };
    for e in dir.flatten() {
        let Some(pid) = e.file_name().to_str().and_then(|s| s.parse::<u32>().ok()) else {
            continue;
        };
        let Ok(env) = std::fs::read(format!("/proc/{pid}/environ")) else {
            continue;
        };
        let hit = env.split(|b| *b == 0).any(|kv| {
            if prefix {
                kv.starts_with(needle.as_bytes())
            } else {
                kv == needle.as_bytes()
            }
        });
        if !hit {
            continue;
        }
        let zombie = std::fs::read_to_string(format!("/proc/{pid}/stat"))
            .ok()
            .and_then(|s| {
                s.rsplit_once(')')
                    .map(|(_, r)| r.trim_start().starts_with('Z'))
            })
            .unwrap_or(true);
        if !zombie {
            out.push(pid);
        }
    }
    out
}

fn pss_kib(pid: u32) -> u64 {
    std::fs::read_to_string(format!("/proc/{pid}/smaps_rollup"))
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("Pss:"))
                .and_then(|l| l.split_whitespace().nth(1))
                .and_then(|n| n.parse().ok())
        })
        .unwrap_or(0)
}

fn kill_all(pids: &[u32]) {
    for &p in pids {
        unsafe {
            libc::kill(p as i32, libc::SIGKILL);
        }
    }
}

fn reap(mut child: Child) {
    std::thread::spawn(move || {
        let _ = child.wait();
    });
}

fn reader(id: String, gen: u64, mut sock: UnixStream, tx: Sender<(String, u64, Wire)>) {
    std::thread::spawn(move || {
        let mut dec = Decoder::default();
        let mut buf = vec![0u8; 256 * 1024];
        loop {
            let n = match sock.read(&mut buf) {
                Ok(0) | Err(_) => {
                    let _ = tx.send((id, gen, Wire::Eof));
                    return;
                }
                Ok(n) => n,
            };
            dec.push(&buf[..n]);
            loop {
                match dec.next_frame() {
                    Ok(Some(body)) => {
                        let w = match serde_json::from_slice::<FromHelper>(&body) {
                            Ok(m) => Wire::Msg(m),
                            Err(e) => Wire::Violation(format!("malformed frame: {e}")),
                        };
                        let stop = matches!(w, Wire::Violation(_));
                        if tx.send((id.clone(), gen, w)).is_err() || stop {
                            return;
                        }
                    }
                    Ok(None) => break,
                    Err(e) => {
                        let _ = tx.send((id, gen, Wire::Violation(format!("{e:?}"))));
                        return;
                    }
                }
            }
        }
    });
}

impl WidgetBackend for LinuxBackend {
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            platform: "linux".into(),
            embedding: if self.cfg.parent != 0 {
                Embedding::X11Child
            } else {
                Embedding::None
            },
            process_isolation: ProcessIsolation::PerWidgetProcess,
            egress: if self.netns {
                EgressContainment::NetworkNamespace
            } else {
                EgressContainment::DeadProxy
            },
            speculative_loading_switch: true,
            watchdog: WatchdogSignal::Heartbeat,
            verified: true,
        }
    }

    fn launch(&mut self, spec: &WidgetSpec, placement: Placement) -> Result<Option<u32>, String> {
        if self.procs.contains_key(&spec.id) {
            self.stop(&spec.id);
        }
        self.gen += 1;
        let gen = self.gen;
        let tag = format!("{}-{}-{gen}", self.session, spec.id);
        let (ours, theirs) = UnixStream::pair().map_err(|e| format!("socketpair: {e}"))?;
        let their_fd = theirs.as_raw_fd();
        let contain = self.netns && spec.hardening.contain_egress;
        let mut cmd = if contain {
            let mut c = Command::new("unshare");
            c.args(["--user", "--map-root-user", "--net", "--"])
                .arg(&self.cfg.helper)
                .args(&self.cfg.helper_args);
            c
        } else {
            let mut c = Command::new(&self.cfg.helper);
            c.args(&self.cfg.helper_args);
            c
        };
        cmd.args(["--fd", "3"])
            .env(TAG_VAR, &tag)
            .env("GDK_BACKEND", "x11")
            .stdin(Stdio::null())
            .stdout(Stdio::null());
        if std::env::var_os("MALEFICIUM_WIDGET_STDERR").is_none() {
            cmd.stderr(Stdio::null());
        }
        unsafe {
            cmd.pre_exec(move || {
                // Its own process group, so one signal ends everything it
                // starts; killed with the host; the socket as fd 3.
                libc::setpgid(0, 0);
                libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL);
                if their_fd == 3 {
                    libc::fcntl(3, libc::F_SETFD, 0);
                } else if libc::dup2(their_fd, 3) < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let child = cmd
            .spawn()
            .map_err(|e| format!("cannot start the widget helper: {e}"))?;
        drop(theirs);
        let pid = child.id();
        let timeout = libc::timeval {
            tv_sec: 5,
            tv_usec: 0,
        };
        unsafe {
            libc::setsockopt(
                ours.as_raw_fd(),
                libc::SOL_SOCKET,
                libc::SO_SNDTIMEO,
                &timeout as *const libc::timeval as *const libc::c_void,
                std::mem::size_of::<libc::timeval>() as libc::socklen_t,
            );
        }
        let read_half = ours.try_clone().map_err(|e| e.to_string())?;
        self.procs.insert(
            spec.id.clone(),
            Proc {
                gen,
                child,
                sock: ours,
            },
        );
        self.tags.insert(spec.id.clone(), tag);
        reader(spec.id.clone(), gen, read_half, self.tx.clone());
        let config = ToHelper::Config {
            proto: PROTOCOL,
            widget_id: spec.id.clone(),
            parent: self.cfg.parent,
            width: placement.slot.w.max(1),
            height: placement.slot.h.max(1),
            csp: spec.csp.clone(),
            hardening: spec.hardening,
        };
        let sent = std::iter::once(config)
            .chain(content_frames(&spec.document, &spec.sources))
            .try_for_each(|f| self.write(&spec.id, &f));
        if let Err(e) = sent {
            self.stop(&spec.id);
            return Err(e);
        }
        Ok(Some(pid))
    }

    fn send(&mut self, id: &str, msg: &ToHelper) -> Result<(), String> {
        self.write(id, msg)
    }

    fn stop(&mut self, id: &str) {
        let Some(p) = self.procs.remove(id) else {
            return;
        };
        unsafe {
            libc::killpg(p.child.id() as i32, libc::SIGKILL);
        }
        if let Some(tag) = self.tags.get(id) {
            kill_all(&tagged(tag, false));
        }
        let _ = p.sock.shutdown(std::net::Shutdown::Both);
        reap(p.child);
    }

    fn disconnect(&mut self, id: &str) {
        if let Some(p) = self.procs.remove(id) {
            let _ = p.sock.shutdown(std::net::Shutdown::Both);
            reap(p.child);
        }
    }

    fn residue(&self, id: &str) -> usize {
        self.tags.get(id).map_or(0, |t| tagged(t, false).len())
    }

    fn memory_kib(&self, id: &str) -> Option<u64> {
        if !self.procs.contains_key(id) {
            return None;
        }
        let tag = self.tags.get(id)?;
        Some(tagged(tag, false).into_iter().map(pss_kib).sum())
    }

    fn poll(&mut self, wait: Duration) -> Vec<BackendEvent> {
        let mut raw = Vec::new();
        match self.rx.recv_timeout(wait) {
            Ok(e) => raw.push(e),
            Err(RecvTimeoutError::Timeout) | Err(RecvTimeoutError::Disconnected) => {}
        }
        while let Ok(e) = self.rx.try_recv() {
            raw.push(e);
        }
        let mut out = Vec::new();
        for (id, gen, w) in raw {
            if self.procs.get(&id).map(|p| p.gen) != Some(gen) {
                continue;
            }
            match w {
                Wire::Msg(msg) => out.push(BackendEvent::Message { id, msg }),
                Wire::Violation(why) => {
                    self.stop(&id);
                    out.push(BackendEvent::Violation { id, why });
                }
                Wire::Eof => {
                    if let Some(p) = self.procs.remove(&id) {
                        reap(p.child);
                    }
                    out.push(BackendEvent::Exited {
                        id,
                        why: "the helper closed its channel".into(),
                    });
                }
            }
        }
        out
    }
}

impl Drop for LinuxBackend {
    fn drop(&mut self) {
        let ids: Vec<String> = self.procs.keys().cloned().collect();
        for id in ids {
            self.stop(&id);
        }
    }
}
