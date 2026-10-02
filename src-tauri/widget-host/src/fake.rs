//! An in-memory backend on a virtual clock. It plays a helper for each
//! conformance fixture (read from the document's `data-conformance`
//! attribute), so the host policy and the suite run without a display. Its
//! egress model connects to the probe listener exactly when a real engine
//! would leak, so the suite's red controls stay red here too.

use crate::caps::{Capabilities, EgressContainment, Embedding, ProcessIsolation, WatchdogSignal};
use crate::host::{BackendEvent, WidgetBackend, WidgetSpec};
use crate::proto::{ActiveWhy, FromHelper, Hardening, Placement, ToHelper};
use crate::Clock;
use serde_json::{json, Value};
use std::collections::{BTreeMap, VecDeque};
use std::io::Write;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// A clock that moves only when told to.
#[derive(Clone, Default)]
pub struct FakeClock(Arc<AtomicU64>);

impl FakeClock {
    pub fn advance(&self, ms: u64) {
        self.0.fetch_add(ms, Ordering::SeqCst);
    }
}

impl Clock for FakeClock {
    fn now_ms(&self) -> u64 {
        self.0.load(Ordering::SeqCst)
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Behavior {
    Green,
    /// Freezes this long after `init`.
    Loop(u64),
    Hostile,
    Probe {
        fetch: u16,
        preconnect: u16,
    },
}

fn attr<'a>(doc: &'a str, name: &str) -> Option<&'a str> {
    let key = format!("{name}=\"");
    let start = doc.find(&key)? + key.len();
    let end = doc[start..].find('"')? + start;
    Some(&doc[start..end])
}

fn behavior(doc: &str) -> Behavior {
    match attr(doc, "data-conformance").unwrap_or("green") {
        "hostile" => Behavior::Hostile,
        "probe" => Behavior::Probe {
            fetch: attr(doc, "data-fetch-port")
                .and_then(|p| p.parse().ok())
                .unwrap_or(0),
            preconnect: attr(doc, "data-preconnect-port")
                .and_then(|p| p.parse().ok())
                .unwrap_or(0),
        },
        b => match b.strip_prefix("loop:").and_then(|ms| ms.parse().ok()) {
            Some(ms) => Behavior::Loop(ms),
            None => Behavior::Green,
        },
    }
}

struct Helper {
    behavior: Behavior,
    hardening: Hardening,
    netns: bool,
    sources: Vec<(String, usize)>,
    launched: u64,
    pid: u32,
    stage: u8,
    last_beat: u64,
    frozen_at: Option<u64>,
    outbox: VecDeque<FromHelper>,
    connected: bool,
}

/// Everything the fake records, shared with the test that made it.
#[derive(Default)]
pub struct FakeState {
    helpers: BTreeMap<String, Helper>,
    /// Every message sent to a helper, in order.
    pub sent: Vec<(String, ToHelper)>,
    /// Launch count per widget id.
    pub launches: BTreeMap<String, u32>,
    next_pid: u32,
}

pub struct FakeBackend {
    clock: FakeClock,
    netns: bool,
    state: Arc<Mutex<FakeState>>,
}

impl FakeBackend {
    /// `netns`: whether the fake platform has network namespaces.
    pub fn new(clock: FakeClock, netns: bool) -> Self {
        Self {
            clock,
            netns,
            state: Arc::new(Mutex::new(FakeState {
                next_pid: 1000,
                ..FakeState::default()
            })),
        }
    }

    pub fn state(&self) -> Arc<Mutex<FakeState>> {
        self.state.clone()
    }
}

impl FakeState {
    /// Widgets with a helper alive.
    pub fn alive(&self) -> usize {
        self.helpers.len()
    }
}

const BEAT_MS: u64 = 200;

fn connect(port: u16, bytes: &[u8]) {
    if port == 0 {
        return;
    }
    if let Ok(mut s) = std::net::TcpStream::connect(("127.0.0.1", port)) {
        let _ = s.write_all(bytes);
    }
}

fn bridge(m: Value) -> FromHelper {
    FromHelper::Bridge { m }
}

impl Helper {
    /// The runtime's reaction to `init`, as the fixture's script would do.
    fn on_init(&mut self, now: u64, theme_mode: &str) {
        let srcs: Vec<String> = self
            .sources
            .iter()
            .map(|(r, n)| format!("{r}:{n}"))
            .collect();
        let init = json!({"mfw": 1, "type": "status", "state": "loaded",
            "message": format!("init {theme_mode} {}", srcs.join(","))});
        match self.behavior.clone() {
            Behavior::Green => self.outbox.push_back(bridge(init)),
            Behavior::Loop(ms) => {
                self.outbox.push_back(bridge(init));
                self.frozen_at = Some(now + ms);
            }
            Behavior::Hostile => {
                self.outbox.push_back(bridge(init));
                for m in hostile_set() {
                    let allowed = m.get("mfw").and_then(Value::as_u64) == Some(1)
                        && matches!(
                            m.get("type").and_then(Value::as_str),
                            Some("ready" | "status" | "resize" | "snapshot")
                        );
                    self.outbox.push_back(if allowed {
                        bridge(m)
                    } else {
                        FromHelper::Drop {
                            why: "prefilter".into(),
                            kind: m.get("type").map(|t| t.to_string()).unwrap_or_default(),
                        }
                    });
                }
            }
            Behavior::Probe { fetch, preconnect } => {
                let contained = self.netns && self.hardening.contain_egress;
                if !contained && !self.hardening.csp_header && !self.hardening.dead_proxy {
                    connect(fetch, b"GET /fetch HTTP/1.1\r\n\r\n");
                }
                if !contained && !self.hardening.speculative_off {
                    connect(preconnect, b"");
                }
                self.outbox.push_back(bridge(
                    json!({"mfw": 1, "type": "status", "state": "loaded",
                    "message": "probe", "detail": {
                        "parent": "blocked", "top": "blocked", "tauri": "undefined",
                        "handlers": "undefined", "forge": "blocked", "storage": "blocked",
                        "open": "null"}}),
                ));
            }
        }
    }

    fn step(&mut self, now: u64) {
        let t = now.saturating_sub(self.launched);
        if self.stage == 0 && t >= 40 {
            self.stage = 1;
            self.outbox.push_back(FromHelper::Ready {
                pid: self.pid,
                window: 1,
                features_off: if self.hardening.speculative_off {
                    vec!["LinkPreconnect".into(), "LinkPrefetch".into()]
                } else {
                    Vec::new()
                },
                sandbox: true,
                dead_proxy: self.hardening.dead_proxy,
            });
        }
        if self.stage == 1 && t >= 80 {
            self.stage = 2;
            self.outbox.push_back(FromHelper::Loaded { took_ms: 80 });
            self.outbox
                .push_back(bridge(json!({"mfw": 1, "type": "ready"})));
            self.last_beat = now;
            self.outbox.push_back(FromHelper::Hb);
        }
        if self.stage == 2 {
            let frozen = self.frozen_at.is_some_and(|f| now >= f);
            while !frozen && self.last_beat + BEAT_MS <= now {
                self.last_beat += BEAT_MS;
                self.outbox.push_back(FromHelper::Hb);
            }
        }
    }
}

/// The hostile message set every implementation must drop (one valid
/// status is the control).
pub fn hostile_set() -> Vec<Value> {
    vec![
        json!({"mfw": 1, "type": "invoke", "cmd": "write_file"}),
        json!({"mfw": 1, "type": "open-url", "url": "https://example.org"}),
        json!({"type": "status", "state": "loaded"}),
        json!({"mfw": 1, "type": "status", "state": "pwned", "message": "x"}),
        json!({"mfw": 1, "type": "status", "state": "loaded", "message": "y".repeat(5000)}),
        json!({"mfw": 1, "type": "resize", "height": 1e9}),
        json!({"mfw": 1, "type": "snapshot", "requestId": "r", "png": "javascript:alert(1)"}),
        json!({"mfw": 1, "type": "status", "state": "loaded", "message": "hostile ok"}),
    ]
}

impl WidgetBackend for FakeBackend {
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            platform: "fake".into(),
            embedding: Embedding::Offscreen,
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

    fn launch(&mut self, spec: &WidgetSpec, _placement: Placement) -> Result<Option<u32>, String> {
        let mut st = self.state.lock().unwrap();
        st.next_pid += 1;
        let pid = st.next_pid;
        *st.launches.entry(spec.id.clone()).or_insert(0) += 1;
        st.helpers.insert(
            spec.id.clone(),
            Helper {
                behavior: behavior(&spec.document),
                hardening: spec.hardening,
                netns: self.netns,
                sources: spec
                    .sources
                    .iter()
                    .map(|s| (s.role.clone(), s.bytes.len()))
                    .collect(),
                launched: self.clock.now_ms(),
                pid,
                stage: 0,
                last_beat: 0,
                frozen_at: None,
                outbox: VecDeque::new(),
                connected: true,
            },
        );
        Ok(Some(pid))
    }

    fn send(&mut self, id: &str, msg: &ToHelper) -> Result<(), String> {
        let now = self.clock.now_ms();
        let mut st = self.state.lock().unwrap();
        st.sent.push((id.to_string(), msg.clone()));
        let h = st.helpers.get_mut(id).ok_or("no such helper")?;
        if h.frozen_at.is_some_and(|f| now >= f) {
            return Ok(());
        }
        match msg {
            ToHelper::Init { theme, .. } => {
                let mode = serde_json::to_value(theme.mode).unwrap();
                h.on_init(now, mode.as_str().unwrap_or("light"));
            }
            ToHelper::Theme(t) => {
                let mode = serde_json::to_value(t.mode).unwrap();
                h.outbox.push_back(bridge(
                    json!({"mfw": 1, "type": "status", "state": "loaded",
                    "message": format!("theme {}", mode.as_str().unwrap_or(""))}),
                ));
            }
            ToHelper::SnapshotRequest { request_id } => h.outbox.push_back(bridge(
                json!({"mfw": 1, "type": "snapshot", "requestId": request_id,
                    "png": "data:image/png;base64,iVBORw0KGgo="}),
            )),
            ToHelper::Activate { on } => h.outbox.push_back(FromHelper::Active {
                on: *on,
                why: ActiveWhy::Host,
            }),
            ToHelper::Place { seq, .. } => h.outbox.push_back(FromHelper::Placed { seq: *seq }),
            _ => {}
        }
        Ok(())
    }

    fn stop(&mut self, id: &str) {
        self.state.lock().unwrap().helpers.remove(id);
    }

    fn disconnect(&mut self, id: &str) {
        if let Some(h) = self.state.lock().unwrap().helpers.get_mut(id) {
            h.connected = false;
        }
    }

    fn residue(&self, id: &str) -> usize {
        usize::from(self.state.lock().unwrap().helpers.contains_key(id))
    }

    fn memory_kib(&self, id: &str) -> Option<u64> {
        self.state
            .lock()
            .unwrap()
            .helpers
            .contains_key(id)
            .then_some(80 * 1024)
    }

    fn poll(&mut self, wait: Duration) -> Vec<BackendEvent> {
        self.clock.advance(wait.as_millis() as u64);
        let now = self.clock.now_ms();
        let mut st = self.state.lock().unwrap();
        // A disconnected helper sees end of file and exits.
        st.helpers.retain(|_, h| h.connected);
        let mut out = Vec::new();
        for (id, h) in st.helpers.iter_mut() {
            h.step(now);
            while let Some(msg) = h.outbox.pop_front() {
                out.push(BackendEvent::Message {
                    id: id.clone(),
                    msg,
                });
            }
        }
        out
    }
}
