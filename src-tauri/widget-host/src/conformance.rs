//! The conformance suite every widget-host implementation passes. It drives
//! a [`WidgetHost`] through a [`Platform`] and observes only host events,
//! backend residue and a loopback listener, so it holds for any platform.
//! A case that needs a capability the platform reports missing ends as
//! [`Outcome::Missing`], never as a pass.

use crate::caps::EgressContainment;
use crate::host::{
    DropBy, HostConfig, HostEvent, KillReason, SuspendReason, View, WidgetHost, WidgetSpec,
    WidgetState,
};
use crate::proto::{Hardening, Placement, Theme, ThemeMode};
use crate::{bridge::BridgeMessage, bridge::StatusState, SourceBytes};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::io::Read;
use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// What a platform gives the suite.
pub trait Platform {
    /// A fresh host over a fresh backend.
    fn host(&mut self, cfg: HostConfig) -> WidgetHost;
    /// A visible placement for slot `i` in the platform's test window.
    fn slot(&self, i: usize) -> Placement;
    /// Processes this platform started for widgets that are still alive,
    /// across every host it made.
    fn stray_processes(&self) -> usize;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Pass,
    Fail(String),
    /// The platform lacks a capability the case needs.
    Missing(String),
}

pub type Case = fn(&mut dyn Platform) -> Outcome;

/// Every case, by name.
pub const CASES: &[(&str, Case)] = &[
    ("lifecycle", lifecycle),
    ("theme-and-snapshot", theme_and_snapshot),
    ("activation", activation),
    ("hostile-bridge", hostile_bridge),
    ("isolation-probes", isolation_probes),
    ("egress-red-controls", egress_red_controls),
    ("watchdog", watchdog),
    ("quarantine", quarantine),
    ("live-cap", live_cap),
    ("suspend-resume", suspend_resume),
    ("revocation", revocation),
    ("host-gone", host_gone),
];

/// Runs every case; one line per case.
pub fn run_all(p: &mut dyn Platform) -> Vec<(&'static str, Outcome)> {
    CASES
        .iter()
        .map(|(n, c)| (*n, gate(p).unwrap_or_else(|| c(p))))
        .collect()
}

// ---- fixtures ---------------------------------------------------------------

const RUNTIME: &str = r#"const post=(m)=>window.parent.postMessage(Object.assign({mfw:1},m),'*');
window.addEventListener('message',(e)=>{if(e.source!==window.parent)return;const d=e.data||{};if(d.mfw!==1)return;
if(d.type==='init'){const s=d.sources||{};const n=Object.keys(s).map(k=>k+':'+(s[k].bytes?s[k].bytes.byteLength:'?')).join(',');
document.documentElement.dataset.mode=d.theme&&d.theme.mode;post({type:'status',state:'loaded',message:'init '+(d.theme&&d.theme.mode)+' '+n});afterInit();}
else if(d.type==='theme'){post({type:'status',state:'loaded',message:'theme '+d.mode});}
else if(d.type==='snapshot-request'){post({type:'snapshot',requestId:d.requestId,png:'data:image/png;base64,iVBORw0KGgo='});}});
let n=0;(function f(){n++;document.getElementById('t').textContent='w '+n;requestAnimationFrame(f);})();
post({type:'ready'});"#;

fn document(kind: &str, extra_attrs: &str, head: &str, after_init: &str) -> String {
    format!(
        "<!doctype html><html data-conformance=\"{kind}\"{extra_attrs}><head><meta charset=\"utf-8\">{head}\
<style>html,body{{margin:0;height:100%;overflow:hidden;background:#00ff00}}#t{{font:20px sans-serif}}</style></head>\
<body><div id=\"t\">w</div><script>function afterInit(){{{after_init}}}\n{RUNTIME}</script></body></html>"
    )
}

pub fn green() -> String {
    document("green", "", "", "")
}

/// Freezes its main thread `ms` after `init`.
pub fn looping(ms: u64) -> String {
    document(
        &format!("loop:{ms}"),
        "",
        "",
        &format!("setTimeout(()=>{{while(true){{}}}},{ms});"),
    )
}

pub fn hostile() -> String {
    let posts: String = crate::fake::hostile_set()
        .iter()
        .map(|m| format!("window.parent.postMessage({m},'*');"))
        .collect();
    document("hostile", "", "", &posts)
}

/// Isolation checks plus every egress channel aimed at the listener.
pub fn probe(fetch: u16, preconnect: u16) -> String {
    let after = format!(
        r#"const r={{}};
try{{r.parent=String(window.parent.document.title)}}catch(e){{r.parent='blocked'}}
try{{r.top=String(window.top.location.href)}}catch(e){{r.top='blocked'}}
r.tauri=typeof window.__TAURI_INTERNALS__;
r.handlers=typeof(window.webkit&&window.webkit.messageHandlers);
try{{window.webkit.messageHandlers.mfw.postMessage('{{}}');r.forge='sent'}}catch(e){{r.forge='blocked'}}
try{{localStorage.setItem('x','1');r.storage='yes'}}catch(e){{r.storage='blocked'}}
r.open=String(window.open('http://127.0.0.1:{fetch}/open'));
fetch('http://127.0.0.1:{fetch}/fetch',{{mode:'no-cors'}}).catch(()=>0);
new Image().src='http://127.0.0.1:{fetch}/img';
try{{new WebSocket('ws://127.0.0.1:{fetch}/ws')}}catch(e){{}}
for(const k of ['preconnect','dns-prefetch','prefetch','prerender']){{const l=document.createElement('link');l.rel=k;l.href='http://127.0.0.1:{preconnect}/'+k;document.head.appendChild(l);}}
setTimeout(()=>post({{type:'status',state:'loaded',message:'probe',detail:r}}),1500);
setTimeout(()=>{{location.href='http://127.0.0.1:{fetch}/nav'}},2000);"#
    );
    document(
        "probe",
        &format!(" data-fetch-port=\"{fetch}\" data-preconnect-port=\"{preconnect}\""),
        &format!("<link rel=\"preconnect\" href=\"http://127.0.0.1:{preconnect}/static\">"),
        &after,
    )
}

/// The base policy, as the app renders it for a widget host.
pub fn policy(id: &str) -> String {
    format!(
        "default-src 'none'; script-src mfw://{id} 'unsafe-inline'; style-src mfw://{id} 'unsafe-inline'; img-src mfw://{id} data: blob:; media-src mfw://{id} data: blob:; font-src mfw://{id} data:; connect-src 'none'; form-action 'none'; base-uri 'none'"
    )
}

pub fn spec(id: &str, document: String) -> WidgetSpec {
    WidgetSpec {
        id: id.to_string(),
        document,
        csp: policy(id),
        runtime: "conformance@1".into(),
        alt: "conformance widget".into(),
        options: json!({}),
        sources: vec![SourceBytes {
            role: "data".into(),
            name: "d.csv".into(),
            mime: "text/csv".into(),
            sha256: "0".repeat(64),
            bytes: b"a,b\n1".to_vec(),
        }],
        hardening: Hardening::default(),
    }
}

// ---- the loopback listener -------------------------------------------------

/// Records every connection to two loopback ports: whether bytes arrived
/// and the first line.
pub struct Listener {
    pub fetch: u16,
    pub preconnect: u16,
    hits: Arc<Mutex<Vec<(u16, String)>>>,
    stop: Arc<AtomicBool>,
}

impl Listener {
    pub fn start() -> Self {
        let hits = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let mut ports = Vec::new();
        for _ in 0..2 {
            let l = TcpListener::bind("127.0.0.1:0").expect("bind a loopback port");
            l.set_nonblocking(true).unwrap();
            let port = l.local_addr().unwrap().port();
            ports.push(port);
            let (hits, stop) = (hits.clone(), stop.clone());
            std::thread::spawn(move || {
                while !stop.load(Ordering::SeqCst) {
                    match l.accept() {
                        Ok((mut s, _)) => {
                            let _ = s.set_nonblocking(false);
                            let _ = s.set_read_timeout(Some(Duration::from_millis(300)));
                            let mut buf = [0u8; 256];
                            let n = s.read(&mut buf).unwrap_or(0);
                            let line = String::from_utf8_lossy(&buf[..n])
                                .lines()
                                .next()
                                .unwrap_or("")
                                .to_string();
                            hits.lock().unwrap().push((port, line));
                        }
                        Err(_) => std::thread::sleep(Duration::from_millis(10)),
                    }
                }
            });
        }
        Self {
            fetch: ports[0],
            preconnect: ports[1],
            hits,
            stop,
        }
    }

    pub fn hits(&self) -> Vec<(u16, String)> {
        self.hits.lock().unwrap().clone()
    }

    fn on(&self, port: u16) -> Vec<String> {
        self.hits()
            .into_iter()
            .filter(|(p, _)| *p == port)
            .map(|(_, l)| l)
            .collect()
    }
}

impl Drop for Listener {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

// ---- driving ----------------------------------------------------------------

struct Run {
    host: WidgetHost,
    events: Vec<(u64, HostEvent)>,
}

impl Run {
    fn new(p: &mut dyn Platform, cfg: HostConfig) -> Self {
        Self {
            host: p.host(cfg),
            events: Vec::new(),
        }
    }

    fn pump(&mut self) -> Vec<HostEvent> {
        let evs = self.host.pump(Duration::from_millis(20));
        let now = self.host.now_ms();
        self.events.extend(evs.iter().cloned().map(|e| (now, e)));
        evs
    }

    /// Pump until `done` holds over the events so far, or fail after
    /// `timeout_ms`.
    fn until(
        &mut self,
        what: &str,
        timeout_ms: u64,
        mut done: impl FnMut(&[(u64, HostEvent)]) -> bool,
    ) -> Result<(), String> {
        let end = self.host.now_ms() + timeout_ms;
        loop {
            self.pump();
            if done(&self.events) {
                return Ok(());
            }
            if self.host.now_ms() > end {
                return Err(format!(
                    "timed out after {timeout_ms} ms waiting for {what}; events: {}",
                    summary(&self.events)
                ));
            }
        }
    }

    fn idle(&mut self, ms: u64) {
        let end = self.host.now_ms() + ms;
        while self.host.now_ms() < end {
            self.pump();
        }
    }

    fn show(&mut self, p: &dyn Platform, ids: &[&str]) {
        let views: BTreeMap<String, View> = ids
            .iter()
            .enumerate()
            .map(|(i, id)| {
                (
                    id.to_string(),
                    View {
                        placement: p.slot(i),
                        in_view: true,
                        rank: i as u32,
                    },
                )
            })
            .collect();
        self.host.view(&views);
    }

    fn residue(&self, id: &str) -> usize {
        self.host.backend().residue(id)
    }

    fn gone_within(&mut self, id: &str, ms: u64) -> Result<(), String> {
        let end = self.host.now_ms() + ms;
        loop {
            if self.residue(id) == 0 {
                return Ok(());
            }
            if self.host.now_ms() > end {
                return Err(format!(
                    "{id}: {} processes left after {ms} ms",
                    self.residue(id)
                ));
            }
            self.pump();
        }
    }
}

fn summary(evs: &[(u64, HostEvent)]) -> String {
    let s: Vec<String> = evs
        .iter()
        .map(|(t, e)| format!("{t}:{}", serde_json::to_string(e).unwrap_or_default()))
        .collect();
    let joined = s.join(" ");
    joined.chars().take(3000).collect()
}

fn status_of(e: &HostEvent, id: &str) -> Option<(StatusState, String, Option<Value>)> {
    match e {
        HostEvent::Bridge {
            id: i,
            message:
                BridgeMessage::Status {
                    state,
                    message,
                    detail,
                },
        } if i == id => Some((*state, message.clone().unwrap_or_default(), detail.clone())),
        _ => None,
    }
}

fn has_status(evs: &[(u64, HostEvent)], id: &str, prefix: &str) -> bool {
    evs.iter()
        .any(|(_, e)| status_of(e, id).is_some_and(|(_, m, _)| m.starts_with(prefix)))
}

fn loaded(evs: &[(u64, HostEvent)], id: &str) -> bool {
    has_status(evs, id, "init ")
}

/// Loaded since its latest launch.
fn loaded_now(evs: &[(u64, HostEvent)], id: &str) -> bool {
    let Some(at) = evs
        .iter()
        .rposition(|(_, e)| matches!(e, HostEvent::Launched { id: i, .. } if i == id))
    else {
        return false;
    };
    has_status(&evs[at..], id, "init ")
}

/// Missing capabilities end a case before it starts.
fn gate(p: &mut dyn Platform) -> Option<Outcome> {
    let caps = p.host(HostConfig::default()).capabilities().clone();
    (!caps.runs_widgets()).then(|| Outcome::Missing(caps.missing().join(", ")))
}

fn outcome(r: Result<(), String>) -> Outcome {
    match r {
        Ok(()) => Outcome::Pass,
        Err(e) => Outcome::Fail(e),
    }
}

fn check(cond: bool, what: impl FnOnce() -> String) -> Result<(), String> {
    if cond {
        Ok(())
    } else {
        Err(what())
    }
}

/// Budget from launch to the runtime's `loaded`, per widget.
pub const LOAD_BUDGET_MS: u64 = 5_000;

// ---- cases ------------------------------------------------------------------

/// A widget launches, its helper reports ready with speculative loading off,
/// the runtime gets `init` with its source bytes and the theme, and reports
/// loaded within budget.
pub fn lifecycle(p: &mut dyn Platform) -> Outcome {
    let mut r = Run::new(p, HostConfig::default());
    r.host.set_widgets(vec![spec("green-0", green())]);
    r.show(p, &["green-0"]);
    outcome((|| {
        r.until("loaded", LOAD_BUDGET_MS, |e| loaded(e, "green-0"))?;
        let evs = &r.events;
        let pos = |f: &dyn Fn(&HostEvent) -> bool| evs.iter().position(|(_, e)| f(e));
        let launched = pos(&|e| matches!(e, HostEvent::Launched { .. }));
        let ready = pos(&|e| matches!(e, HostEvent::HelperReady { .. }));
        let bridge_ready = pos(&|e| {
            matches!(
                e,
                HostEvent::Bridge {
                    message: BridgeMessage::Ready,
                    ..
                }
            )
        });
        let status = pos(&|e| status_of(e, "green-0").is_some());
        check(
            launched < ready && ready < bridge_ready && bridge_ready < status && launched.is_some(),
            || format!("events out of order: {}", summary(evs)),
        )?;
        let features_off = evs.iter().find_map(|(_, e)| match e {
            HostEvent::HelperReady { features_off, .. } => Some(features_off.clone()),
            _ => None,
        });
        check(
            features_off.is_some_and(|f| f.iter().any(|x| x == "LinkPreconnect")),
            || format!("LinkPreconnect is not reported off: {}", summary(evs)),
        )?;
        check(has_status(evs, "green-0", "init light data:5"), || {
            format!(
                "init did not carry the theme and the 5 source bytes: {}",
                summary(evs)
            )
        })?;
        check(r.host.state("green-0") == Some(WidgetState::Live), || {
            "not live".into()
        })?;
        check(r.residue("green-0") > 0, || {
            "no process for a live widget".into()
        })
    })())
}

/// Theme changes reach the runtime; a snapshot request is answered.
pub fn theme_and_snapshot(p: &mut dyn Platform) -> Outcome {
    let mut r = Run::new(p, HostConfig::default());
    r.host.set_widgets(vec![spec("green-0", green())]);
    r.show(p, &["green-0"]);
    outcome((|| {
        r.until("loaded", LOAD_BUDGET_MS, |e| loaded(e, "green-0"))?;
        r.host.set_theme(Theme {
            mode: ThemeMode::Dark,
            tokens: [("--m-color-text".to_string(), "#fff".to_string())].into(),
        });
        r.until("theme dark", 2_000, |e| {
            has_status(e, "green-0", "theme dark")
        })?;
        r.host.request_snapshot("green-0", "snap-1");
        r.until("snapshot", 2_000, |e| {
            e.iter().any(|(_, e)| {
                matches!(e, HostEvent::Bridge { message: BridgeMessage::Snapshot { request_id, .. }, .. } if request_id == "snap-1")
            })
        })
    })())
}

/// The host gives input to a widget and takes it back.
pub fn activation(p: &mut dyn Platform) -> Outcome {
    let mut r = Run::new(p, HostConfig::default());
    r.host.set_widgets(vec![spec("green-0", green())]);
    r.show(p, &["green-0"]);
    outcome((|| {
        r.until("loaded", LOAD_BUDGET_MS, |e| loaded(e, "green-0"))?;
        r.host.activate("green-0", true);
        r.until("active", 2_000, |e| {
            e.iter()
                .any(|(_, e)| matches!(e, HostEvent::Active { on: true, .. }))
        })?;
        r.host.deactivate_all();
        r.until("released", 2_000, |e| {
            e.iter()
                .any(|(_, e)| matches!(e, HostEvent::Active { on: false, .. }))
        })
    })())
}

/// Every hostile message is dropped (by the helper's prefilter or the host
/// validator); the one valid status passes.
pub fn hostile_bridge(p: &mut dyn Platform) -> Outcome {
    let mut r = Run::new(p, HostConfig::default());
    r.host.set_widgets(vec![spec("hostile-0", hostile())]);
    r.show(p, &["hostile-0"]);
    outcome((|| {
        r.until("hostile ok", LOAD_BUDGET_MS + 1_000, |e| {
            has_status(e, "hostile-0", "hostile ok")
        })?;
        r.idle(300);
        let drops: Vec<(DropBy, String)> = r
            .events
            .iter()
            .filter_map(|(_, e)| match e {
                HostEvent::Dropped { by, reason, .. } => Some((*by, reason.clone())),
                _ => None,
            })
            .collect();
        let by_host: Vec<&str> = drops
            .iter()
            .filter(|(b, _)| *b == DropBy::Host)
            .map(|(_, r)| r.as_str())
            .collect();
        for want in ["state", "message", "height", "png"] {
            check(by_host.contains(&want), || {
                format!("the host did not drop `{want}`: {drops:?}")
            })?;
        }
        check(drops.len() >= 7, || {
            format!("expected 7 drops, saw {drops:?}")
        })?;
        let accepted: Vec<String> = r
            .events
            .iter()
            .filter_map(|(_, e)| status_of(e, "hostile-0").map(|(_, m, _)| m))
            .collect();
        check(
            accepted
                .iter()
                .all(|m| m == "hostile ok" || m.starts_with("init ")),
            || format!("a hostile status got through: {accepted:?}"),
        )?;
        check(
            !r.events.iter().any(|(_, e)| {
                matches!(
                    e,
                    HostEvent::Bridge {
                        message: BridgeMessage::Resize { .. } | BridgeMessage::Snapshot { .. },
                        ..
                    }
                )
            }),
            || "a hostile resize or snapshot got through".into(),
        )
    })())
}

fn probe_run(
    p: &mut dyn Platform,
    hardening: Hardening,
) -> Result<(Vec<(u64, HostEvent)>, Listener), String> {
    let l = Listener::start();
    let mut r = Run::new(p, HostConfig::default());
    let mut s = spec("probe-0", probe(l.fetch, l.preconnect));
    s.hardening = hardening;
    if !hardening.csp_header {
        s.csp.clear();
    }
    r.host.set_widgets(vec![s]);
    r.show(p, &["probe-0"]);
    r.until("probe report", LOAD_BUDGET_MS + 3_000, |e| {
        has_status(e, "probe-0", "probe")
    })?;
    r.idle(1_500);
    // The listener runs in real time even when the platform's clock does not.
    std::thread::sleep(Duration::from_millis(300));
    Ok((std::mem::take(&mut r.events), l))
}

/// The widget frame cannot reach the parent, the top window, a native
/// message handler, storage or a popup, and opens no connection.
pub fn isolation_probes(p: &mut dyn Platform) -> Outcome {
    outcome((|| {
        let (evs, l) = probe_run(p, Hardening::default())?;
        let detail = evs
            .iter()
            .find_map(|(_, e)| status_of(e, "probe-0").filter(|(_, m, _)| m == "probe"))
            .and_then(|(_, _, d)| d)
            .ok_or("no probe detail")?;
        let want = json!({"parent": "blocked", "top": "blocked", "tauri": "undefined",
            "handlers": "undefined", "forge": "blocked", "storage": "blocked", "open": "null"});
        for (k, v) in want.as_object().unwrap() {
            check(detail.get(k) == Some(v), || format!("probe {k}: {detail}"))?;
        }
        check(l.hits().is_empty(), || {
            format!("connections reached the listener: {:?}", l.hits())
        })
    })())
}

/// Egress is observed, not inferred. Green: nothing connects. Red: with
/// every defence off, the fetch-class request arrives; with speculative
/// loading on (CSP still strict), the preconnect TCP accept arrives; with
/// only speculative loading off, it does not. Where the platform has a
/// network namespace, it alone stops the fully open widget.
pub fn egress_red_controls(p: &mut dyn Platform) -> Outcome {
    let caps = p.host(HostConfig::default()).capabilities().clone();
    if !caps.speculative_loading_switch {
        return Outcome::Missing("speculative-loading-switch".into());
    }
    if caps.egress == EgressContainment::None {
        return Outcome::Missing("egress-containment".into());
    }
    let open = Hardening {
        csp_header: false,
        speculative_off: false,
        dead_proxy: false,
        contain_egress: false,
    };
    outcome((|| {
        let (_, l) = probe_run(p, Hardening::default())?;
        check(l.hits().is_empty(), || {
            format!("green leaked: {:?}", l.hits())
        })?;

        let (_, l) = probe_run(p, open)?;
        let fetch = l.on(l.fetch);
        check(fetch.iter().any(|h| h.starts_with("GET ")), || {
            format!(
                "red control (all defences off) saw no request: {:?}",
                l.hits()
            )
        })?;

        let pre_on = Hardening {
            speculative_off: false,
            dead_proxy: false,
            contain_egress: false,
            ..Hardening::default()
        };
        let (_, l) = probe_run(p, pre_on)?;
        check(!l.on(l.preconnect).is_empty(), || {
            format!(
                "red control (speculative loading on) saw no preconnect: {:?}",
                l.hits()
            )
        })?;
        check(l.on(l.fetch).is_empty(), || {
            format!("the strict CSP let a request through: {:?}", l.hits())
        })?;

        let pre_off = Hardening {
            dead_proxy: false,
            contain_egress: false,
            ..Hardening::default()
        };
        let (_, l) = probe_run(p, pre_off)?;
        check(l.hits().is_empty(), || {
            format!("speculative loading off still connected: {:?}", l.hits())
        })?;

        if caps.egress == EgressContainment::NetworkNamespace {
            let (_, l) = probe_run(
                p,
                Hardening {
                    contain_egress: true,
                    ..open
                },
            )?;
            check(l.hits().is_empty(), || {
                format!("the network namespace let a connection out: {:?}", l.hits())
            })?;
        }
        Ok(())
    })())
}

/// A frozen widget is badged at 2 s and killed at 5 s with every process
/// gone within a second; a sibling is untouched.
pub fn watchdog(p: &mut dyn Platform) -> Outcome {
    let mut r = Run::new(p, HostConfig::default());
    r.host
        .set_widgets(vec![spec("green-0", green()), spec("loop-1", looping(500))]);
    r.show(p, &["green-0", "loop-1"]);
    outcome((|| {
        r.until("both loaded", LOAD_BUDGET_MS, |e| {
            loaded(e, "green-0") && loaded(e, "loop-1")
        })?;
        r.until("kill", 9_000, |e| {
            e.iter().any(|(_, e)| matches!(e, HostEvent::Killed { .. }))
        })?;
        let badge = r.events.iter().find_map(|(_, e)| match e {
            HostEvent::Unresponsive { id, gap_ms } if id == "loop-1" => Some(*gap_ms),
            _ => None,
        });
        let kill = r.events.iter().find_map(|(_, e)| match e {
            HostEvent::Killed {
                id,
                reason: KillReason::Frozen,
                gap_ms,
                kills: 1,
            } if id == "loop-1" => *gap_ms,
            _ => None,
        });
        check(badge.is_some_and(|g| (2_000..2_700).contains(&g)), || {
            format!("badge gap {badge:?}, want 2000..2700")
        })?;
        check(kill.is_some_and(|g| (5_000..5_700).contains(&g)), || {
            format!("kill gap {kill:?}, want 5000..5700")
        })?;
        r.gone_within("loop-1", 1_000)?;
        check(r.host.state("loop-1") == Some(WidgetState::Stopped), || {
            "a killed widget is not stopped".into()
        })?;
        r.idle(500);
        check(
            !r.events.iter().any(|(_, e)| {
                matches!(e, HostEvent::Unresponsive { id, .. } | HostEvent::Killed { id, .. } if id == "green-0")
            }),
            || "the sibling was badged or killed".into(),
        )?;
        check(r.host.state("green-0") == Some(WidgetState::Live), || {
            "the sibling is not live".into()
        })
    })())
}

/// The second kill in a session quarantines the widget; it stays a poster.
pub fn quarantine(p: &mut dyn Platform) -> Outcome {
    let mut r = Run::new(p, HostConfig::default());
    r.host.set_widgets(vec![spec("loop-0", looping(200))]);
    r.show(p, &["loop-0"]);
    let killed = |n: u32| {
        move |e: &[(u64, HostEvent)]| {
            e.iter()
                .any(|(_, e)| matches!(e, HostEvent::Killed { kills, .. } if *kills == n))
        }
    };
    outcome((|| {
        r.until("first kill", LOAD_BUDGET_MS + 7_000, killed(1))?;
        r.host.reload("loop-0");
        r.until("second kill", LOAD_BUDGET_MS + 7_000, killed(2))?;
        check(
            r.events
                .iter()
                .any(|(_, e)| matches!(e, HostEvent::Quarantined { id } if id == "loop-0")),
            || "no quarantine after two kills".into(),
        )?;
        check(
            r.host.state("loop-0") == Some(WidgetState::Quarantined),
            || "not quarantined".into(),
        )?;
        r.host.reload("loop-0");
        r.host.activate("loop-0", true);
        r.show(p, &["loop-0"]);
        r.idle(300);
        let launches = r
            .events
            .iter()
            .filter(|(_, e)| matches!(e, HostEvent::Launched { .. }))
            .count();
        check(launches == 2, || {
            format!("{launches} launches; a quarantined widget restarted")
        })?;
        let refused = r
            .events
            .iter()
            .filter(|(_, e)| matches!(e, HostEvent::Refused { .. }))
            .count();
        check(refused >= 2, || {
            "reload and activate were not refused".into()
        })?;
        r.gone_within("loop-0", 1_000)
    })())
}

/// Twenty widgets scrolled past never put more than the cap live; the
/// evicted are suspended (posters), memory stays in budget, and scrolling
/// back resumes them.
pub fn live_cap(p: &mut dyn Platform) -> Outcome {
    const N: usize = 20;
    const WINDOW: usize = 4;
    let cfg = HostConfig::default();
    let cap = cfg.live_cap;
    let mut r = Run::new(p, cfg);
    let ids: Vec<String> = (0..N).map(|i| format!("w-{i}")).collect();
    r.host
        .set_widgets(ids.iter().map(|id| spec(id, green())).collect());
    let mut peak = 0;
    let mut peak_kib = 0u64;
    let mut steps: Vec<usize> = (0..=N - WINDOW).step_by(2).collect();
    steps.extend([0]);
    outcome((|| {
        for (n, start) in steps.iter().enumerate() {
            let shown: Vec<&str> = ids[*start..start + WINDOW]
                .iter()
                .map(String::as_str)
                .collect();
            r.show(p, &shown);
            let shown_owned: Vec<String> = shown.iter().map(|s| s.to_string()).collect();
            r.until("the window loaded", LOAD_BUDGET_MS * 2, |e| {
                shown_owned.iter().all(|id| loaded_now(e, id))
            })?;
            peak = peak.max(r.host.live_count());
            let kib: u64 = ids
                .iter()
                .filter_map(|id| r.host.backend().memory_kib(id))
                .sum();
            peak_kib = peak_kib.max(kib);
            check(r.host.live_count() <= cap, || {
                format!("step {n}: {} live, cap {cap}", r.host.live_count())
            })?;
            for id in &shown_owned {
                check(r.host.state(id) == Some(WidgetState::Live), || {
                    format!("step {n}: in-view {id} is not live")
                })?;
            }
        }
        r.idle(1_500);
        let running = ids.iter().filter(|id| r.residue(id) > 0).count();
        check(running <= cap, || {
            format!("{running} widgets have processes, cap {cap}")
        })?;
        check(peak == cap, || {
            format!("peak live {peak}, expected the cap {cap}")
        })?;
        let suspended: Vec<&String> = ids
            .iter()
            .filter(|id| r.host.state(id) == Some(WidgetState::Suspended))
            .collect();
        check(suspended.len() >= N - cap - WINDOW, || {
            format!("only {} suspended", suspended.len())
        })?;
        check(
            r.events.iter().any(|(_, e)| {
                matches!(
                    e,
                    HostEvent::Suspended {
                        reason: SuspendReason::Evicted,
                        ..
                    }
                )
            }),
            || "no eviction event".into(),
        )?;
        check(
            r.events
                .iter()
                .any(|(_, e)| matches!(e, HostEvent::Resumed { id } if id == "w-0")),
            || "scrolling back did not resume w-0".into(),
        )?;
        const BUDGET_KIB: u64 = 150 * 1024;
        check(peak_kib <= cap as u64 * BUDGET_KIB, || {
            format!("peak memory {} MiB over {cap} x 150 MiB", peak_kib / 1024)
        })
    })())
}

/// A suspended widget has no process and stays a poster in view; resuming
/// restores it.
pub fn suspend_resume(p: &mut dyn Platform) -> Outcome {
    let mut r = Run::new(p, HostConfig::default());
    r.host.set_widgets(vec![spec("green-0", green())]);
    r.show(p, &["green-0"]);
    outcome((|| {
        r.until("loaded", LOAD_BUDGET_MS, |e| loaded(e, "green-0"))?;
        r.host.suspend("green-0");
        r.pump();
        check(
            r.events.iter().any(|(_, e)| {
                matches!(
                    e,
                    HostEvent::Suspended {
                        reason: SuspendReason::Requested,
                        ..
                    }
                )
            }),
            || "no suspend event".into(),
        )?;
        r.gone_within("green-0", 1_500)?;
        r.show(p, &["green-0"]);
        r.idle(300);
        check(
            r.host.state("green-0") == Some(WidgetState::Suspended),
            || "a requested suspension ended on scroll".into(),
        )?;
        let since = r.events.len();
        r.host.resume("green-0");
        r.until("resumed and loaded", LOAD_BUDGET_MS, |e| {
            e[since..]
                .iter()
                .any(|(_, e)| matches!(e, HostEvent::Resumed { .. }))
                && e[since..]
                    .iter()
                    .any(|(_, e)| status_of(e, "green-0").is_some())
        })
    })())
}

/// Revoking (an empty widget set) ends every helper without a restart.
pub fn revocation(p: &mut dyn Platform) -> Outcome {
    let mut r = Run::new(p, HostConfig::default());
    r.host
        .set_widgets(vec![spec("green-0", green()), spec("green-1", green())]);
    r.show(p, &["green-0", "green-1"]);
    outcome((|| {
        r.until("loaded", LOAD_BUDGET_MS, |e| {
            loaded(e, "green-0") && loaded(e, "green-1")
        })?;
        r.host.set_widgets(Vec::new());
        check(r.host.live_count() == 0, || "widgets still live".into())?;
        r.gone_within("green-0", 1_500)?;
        r.gone_within("green-1", 1_500)?;
        r.host.set_widgets(vec![spec("green-0", green())]);
        r.show(p, &["green-0"]);
        let since = r.events.len();
        r.until("relaunch after re-approval", LOAD_BUDGET_MS, |e| {
            e[since..]
                .iter()
                .any(|(_, e)| status_of(e, "green-0").is_some())
        })
    })())
}

/// A helper whose host goes away exits by itself, and dropping a host ends
/// every helper.
pub fn host_gone(p: &mut dyn Platform) -> Outcome {
    let mut r = Run::new(p, HostConfig::default());
    r.host
        .set_widgets(vec![spec("green-0", green()), spec("green-1", green())]);
    r.show(p, &["green-0", "green-1"]);
    let res = (|| {
        r.until("loaded", LOAD_BUDGET_MS, |e| {
            loaded(e, "green-0") && loaded(e, "green-1")
        })?;
        r.host.backend_mut().disconnect("green-0");
        r.gone_within("green-0", 3_000)
    })();
    drop(r);
    if let Err(e) = res {
        return Outcome::Fail(e);
    }
    let start = std::time::Instant::now();
    while p.stray_processes() > 0 && start.elapsed() < Duration::from_secs(3) {
        std::thread::sleep(Duration::from_millis(50));
    }
    outcome(check(p.stray_processes() == 0, || {
        format!("{} processes outlived their host", p.stray_processes())
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fake::{FakeBackend, FakeClock, FakeState};

    struct Fake {
        netns: bool,
        states: Vec<Arc<Mutex<FakeState>>>,
    }

    impl Platform for Fake {
        fn host(&mut self, cfg: HostConfig) -> WidgetHost {
            let clock = FakeClock::default();
            let b = FakeBackend::new(clock.clone(), self.netns);
            self.states.push(b.state());
            WidgetHost::new(Box::new(b), Arc::new(clock), cfg)
        }
        fn slot(&self, i: usize) -> Placement {
            Placement::clipped(
                crate::proto::Rect {
                    x: 0,
                    y: 220 * i as i32,
                    w: 400,
                    h: 200,
                },
                crate::proto::Rect {
                    x: 0,
                    y: 0,
                    w: 1200,
                    h: 900,
                },
            )
        }
        fn stray_processes(&self) -> usize {
            self.states.iter().map(|s| s.lock().unwrap().alive()).sum()
        }
    }

    #[test]
    fn the_fake_passes_the_suite_with_and_without_a_network_namespace() {
        for netns in [true, false] {
            let mut p = Fake {
                netns,
                states: Vec::new(),
            };
            let results = run_all(&mut p);
            let failed: Vec<_> = results
                .iter()
                .filter(|(_, o)| *o != Outcome::Pass)
                .collect();
            assert!(failed.is_empty(), "netns={netns}: {failed:?}");
            assert_eq!(results.len(), CASES.len());
        }
    }

    #[test]
    fn a_platform_without_capabilities_reports_them_missing() {
        struct Bare;
        impl Platform for Bare {
            fn host(&mut self, cfg: HostConfig) -> WidgetHost {
                let clock = FakeClock::default();
                struct Unsupported(FakeBackend);
                impl crate::host::WidgetBackend for Unsupported {
                    fn capabilities(&self) -> crate::Capabilities {
                        crate::Capabilities::unsupported("macos")
                    }
                    fn launch(
                        &mut self,
                        s: &WidgetSpec,
                        p: Placement,
                    ) -> Result<Option<u32>, String> {
                        self.0.launch(s, p)
                    }
                    fn send(&mut self, id: &str, m: &crate::proto::ToHelper) -> Result<(), String> {
                        self.0.send(id, m)
                    }
                    fn stop(&mut self, id: &str) {
                        self.0.stop(id)
                    }
                    fn disconnect(&mut self, id: &str) {
                        self.0.disconnect(id)
                    }
                    fn residue(&self, id: &str) -> usize {
                        self.0.residue(id)
                    }
                    fn memory_kib(&self, id: &str) -> Option<u64> {
                        self.0.memory_kib(id)
                    }
                    fn poll(&mut self, w: Duration) -> Vec<crate::host::BackendEvent> {
                        self.0.poll(w)
                    }
                }
                WidgetHost::new(
                    Box::new(Unsupported(FakeBackend::new(clock.clone(), false))),
                    Arc::new(clock),
                    cfg,
                )
            }
            fn slot(&self, _: usize) -> Placement {
                Placement::hidden()
            }
            fn stray_processes(&self) -> usize {
                0
            }
        }
        for (name, o) in run_all(&mut Bare) {
            assert!(matches!(o, Outcome::Missing(_)), "{name}: {o:?}");
        }
    }

    #[test]
    fn a_backend_that_ignores_speculative_loading_fails_the_egress_case() {
        // Red control for the suite itself: a fake whose engine always
        // preconnects (the switch is claimed but does nothing) is caught.
        struct Leaky(FakeBackend);
        impl crate::host::WidgetBackend for Leaky {
            fn capabilities(&self) -> crate::Capabilities {
                self.0.capabilities()
            }
            fn launch(&mut self, s: &WidgetSpec, p: Placement) -> Result<Option<u32>, String> {
                let mut s = s.clone();
                s.hardening.speculative_off = false;
                self.0.launch(&s, p)
            }
            fn send(&mut self, id: &str, m: &crate::proto::ToHelper) -> Result<(), String> {
                self.0.send(id, m)
            }
            fn stop(&mut self, id: &str) {
                self.0.stop(id)
            }
            fn disconnect(&mut self, id: &str) {
                self.0.disconnect(id)
            }
            fn residue(&self, id: &str) -> usize {
                self.0.residue(id)
            }
            fn memory_kib(&self, id: &str) -> Option<u64> {
                self.0.memory_kib(id)
            }
            fn poll(&mut self, w: Duration) -> Vec<crate::host::BackendEvent> {
                self.0.poll(w)
            }
        }
        struct P;
        impl Platform for P {
            fn host(&mut self, cfg: HostConfig) -> WidgetHost {
                let clock = FakeClock::default();
                WidgetHost::new(
                    Box::new(Leaky(FakeBackend::new(clock.clone(), false))),
                    Arc::new(clock),
                    cfg,
                )
            }
            fn slot(&self, _: usize) -> Placement {
                Placement::hidden()
            }
            fn stray_processes(&self) -> usize {
                0
            }
        }
        assert!(matches!(egress_red_controls(&mut P), Outcome::Fail(_)));
    }
}
