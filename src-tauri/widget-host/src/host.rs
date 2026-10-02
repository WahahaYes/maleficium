//! [`WidgetHost`]: the live-widget policy over a platform backend. It decides
//! which widgets run (at most `live_cap`, in-view first, the least recently
//! seen evicted first), validates every bridge message, runs the watchdog,
//! and quarantines a widget killed too often. Everything it observes comes
//! out of [`WidgetHost::pump`] as typed [`HostEvent`]s.

use crate::bridge::{self, BridgeMessage};
use crate::caps::Capabilities;
use crate::proto::{ActiveWhy, FromHelper, Hardening, Placement, Theme, ToHelper};
use crate::watchdog::{Verdict, Watchdog, WatchdogConfig};
use crate::{Clock, SourceBytes};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::time::Duration;

/// Everything needed to run one widget.
#[derive(Debug, Clone, PartialEq)]
pub struct WidgetSpec {
    pub id: String,
    /// The widget document, served as `index.html` on the widget's own host.
    pub document: String,
    /// The CSP header for the widget's responses.
    pub csp: String,
    /// `table@1`, or empty for an author bundle.
    pub runtime: String,
    pub alt: String,
    pub options: Value,
    pub sources: Vec<SourceBytes>,
    pub hardening: Hardening,
}

/// What a backend reports.
#[derive(Debug, Clone, PartialEq)]
pub enum BackendEvent {
    Message {
        id: String,
        msg: FromHelper,
    },
    /// The helper broke the protocol; the backend has already stopped it.
    Violation {
        id: String,
        why: String,
    },
    /// The helper ended without being stopped.
    Exited {
        id: String,
        why: String,
    },
}

/// A platform implementation: starts, drives and ends widget helpers.
pub trait WidgetBackend: Send {
    fn capabilities(&self) -> Capabilities;
    /// Start a helper for `spec` and send it its config and content. Returns
    /// the helper's process id where there is one.
    fn launch(&mut self, spec: &WidgetSpec, placement: Placement) -> Result<Option<u32>, String>;
    fn send(&mut self, id: &str, msg: &ToHelper) -> Result<(), String>;
    /// End the helper and every process it owns. Nothing from it is
    /// reported afterwards.
    fn stop(&mut self, id: &str);
    /// Close the channel without stopping anything: a helper must exit on
    /// its own when its host goes away.
    fn disconnect(&mut self, id: &str);
    /// Processes belonging to this widget that are still alive.
    fn residue(&self, id: &str) -> usize;
    /// Memory of this widget's processes in KiB, where measurable.
    fn memory_kib(&self, id: &str) -> Option<u64>;
    /// Events since the last poll, waiting up to `wait` for the first.
    fn poll(&mut self, wait: Duration) -> Vec<BackendEvent>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostConfig {
    /// Widgets live at once; the rest show posters.
    pub live_cap: usize,
    pub watchdog: WatchdogConfig,
}

impl Default for HostConfig {
    fn default() -> Self {
        Self {
            live_cap: 6,
            watchdog: WatchdogConfig::default(),
        }
    }
}

/// Where a widget is in the editor, sent by the shell on scroll, zoom and
/// layout changes. `rank` orders in-view widgets by importance (0 first).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct View {
    pub placement: Placement,
    pub in_view: bool,
    pub rank: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SuspendReason {
    /// Another widget needed the live slot.
    Evicted,
    Requested,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum KillReason {
    /// The watchdog saw no beat for too long.
    Frozen,
    /// The helper broke the protocol.
    Protocol,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DropBy {
    Helper,
    Host,
}

/// Everything the host observes, in order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum HostEvent {
    Launched {
        id: String,
        pid: Option<u32>,
    },
    #[serde(rename_all = "camelCase")]
    HelperReady {
        id: String,
        ms: u64,
        features_off: Vec<String>,
        sandbox: bool,
        dead_proxy: bool,
    },
    Loaded {
        id: String,
        ms: u64,
    },
    Bridge {
        id: String,
        message: BridgeMessage,
    },
    Dropped {
        id: String,
        by: DropBy,
        reason: String,
    },
    #[serde(rename_all = "camelCase")]
    Unresponsive {
        id: String,
        gap_ms: u64,
    },
    Responsive {
        id: String,
    },
    #[serde(rename_all = "camelCase")]
    Killed {
        id: String,
        reason: KillReason,
        gap_ms: Option<u64>,
        kills: u32,
    },
    Quarantined {
        id: String,
    },
    Suspended {
        id: String,
        reason: SuspendReason,
    },
    Resumed {
        id: String,
    },
    Active {
        id: String,
        on: bool,
        why: ActiveWhy,
    },
    Exited {
        id: String,
        why: String,
    },
    LaunchFailed {
        id: String,
        error: String,
    },
    Refused {
        id: String,
        why: String,
    },
}

/// What the shell shows for a widget: anything but `Live` is its poster.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WidgetState {
    /// Not started yet, or waiting for a live slot.
    Idle,
    Live,
    Suspended,
    /// Killed or crashed; a reload may start it again.
    Stopped,
    Quarantined,
}

struct Live {
    launched: u64,
    watchdog: Watchdog,
    init_sent: bool,
    placed: Option<Placement>,
    seq: u64,
}

enum Phase {
    Idle,
    Live(Box<Live>),
    Suspended(SuspendReason),
    Stopped,
    Quarantined,
}

struct Entry {
    spec: WidgetSpec,
    phase: Phase,
    kills: u32,
    view: View,
    /// When the widget was last in view or activated (LRU).
    seen: u64,
    /// Launched before, so a new launch is a resume.
    was_live: bool,
}

pub struct WidgetHost {
    backend: Box<dyn WidgetBackend>,
    clock: std::sync::Arc<dyn Clock>,
    cfg: HostConfig,
    caps: Capabilities,
    widgets: BTreeMap<String, Entry>,
    theme: Theme,
    hidden: bool,
    events: Vec<HostEvent>,
    tick: u64,
}

impl WidgetHost {
    pub fn new(
        backend: Box<dyn WidgetBackend>,
        clock: std::sync::Arc<dyn Clock>,
        cfg: HostConfig,
    ) -> Self {
        let caps = backend.capabilities();
        Self {
            backend,
            clock,
            cfg,
            caps,
            widgets: BTreeMap::new(),
            theme: Theme::default(),
            hidden: false,
            events: Vec::new(),
            tick: 0,
        }
    }

    pub fn capabilities(&self) -> &Capabilities {
        &self.caps
    }

    pub fn config(&self) -> &HostConfig {
        &self.cfg
    }

    pub fn backend(&self) -> &dyn WidgetBackend {
        self.backend.as_ref()
    }

    pub fn backend_mut(&mut self) -> &mut dyn WidgetBackend {
        self.backend.as_mut()
    }

    pub fn now_ms(&self) -> u64 {
        self.clock.now_ms()
    }

    /// Replace the widget set. A widget whose spec is unchanged keeps
    /// running; a changed one restarts; a removed one stops. An empty set
    /// stops everything (revocation).
    pub fn set_widgets(&mut self, specs: Vec<WidgetSpec>) {
        let mut next: BTreeMap<String, Entry> = BTreeMap::new();
        for spec in specs {
            match self.widgets.remove(&spec.id) {
                Some(mut e) if e.spec == spec => {
                    e.spec = spec;
                    next.insert(e.spec.id.clone(), e);
                }
                old => {
                    if let Some(mut e) = old {
                        self.end(&mut e);
                    }
                    next.insert(
                        spec.id.clone(),
                        Entry {
                            spec,
                            phase: Phase::Idle,
                            kills: 0,
                            view: View {
                                placement: Placement::hidden(),
                                in_view: false,
                                rank: u32::MAX,
                            },
                            seen: 0,
                            was_live: false,
                        },
                    );
                }
            }
        }
        for (_, mut e) in std::mem::take(&mut self.widgets) {
            self.end(&mut e);
        }
        self.widgets = next;
    }

    fn end(&mut self, e: &mut Entry) {
        if matches!(e.phase, Phase::Live(_)) {
            self.backend.stop(&e.spec.id);
        }
        e.phase = Phase::Idle;
    }

    pub fn ids(&self) -> Vec<String> {
        self.widgets.keys().cloned().collect()
    }

    pub fn state(&self, id: &str) -> Option<WidgetState> {
        self.widgets.get(id).map(|e| match e.phase {
            Phase::Idle => WidgetState::Idle,
            Phase::Live(_) => WidgetState::Live,
            Phase::Suspended(_) => WidgetState::Suspended,
            Phase::Stopped => WidgetState::Stopped,
            Phase::Quarantined => WidgetState::Quarantined,
        })
    }

    pub fn live_count(&self) -> usize {
        self.widgets
            .values()
            .filter(|e| matches!(e.phase, Phase::Live(_)))
            .count()
    }

    /// Where every widget is now. Widgets left out are out of view. Starts
    /// the in-view widgets that fit under the cap, evicting the least
    /// recently seen out-of-view ones first, and places every live widget.
    pub fn view(&mut self, views: &BTreeMap<String, View>) {
        self.tick += 1;
        let tick = self.tick;
        for (id, e) in self.widgets.iter_mut() {
            e.view = views.get(id).copied().unwrap_or(View {
                placement: Placement::hidden(),
                in_view: false,
                rank: u32::MAX,
            });
            if e.view.in_view {
                e.seen = tick;
            }
        }
        self.schedule();
        self.place_all();
    }

    fn runnable(e: &Entry) -> bool {
        matches!(
            e.phase,
            Phase::Idle | Phase::Live(_) | Phase::Suspended(SuspendReason::Evicted)
        )
    }

    fn schedule(&mut self) {
        if !self.caps.runs_widgets() {
            return;
        }
        let cap = self.cfg.live_cap;
        let mut want: Vec<(&String, &Entry)> = self
            .widgets
            .iter()
            .filter(|(_, e)| e.view.in_view && Self::runnable(e))
            .collect();
        want.sort_by_key(|(id, e)| (e.view.rank, (*id).clone()));
        let want: Vec<String> = want
            .into_iter()
            .take(cap)
            .map(|(id, _)| id.clone())
            .collect();

        let mut live: Vec<(u64, String)> = self
            .widgets
            .iter()
            .filter(|(id, e)| matches!(e.phase, Phase::Live(_)) && !want.contains(id))
            .map(|(id, e)| (e.seen, id.clone()))
            .collect();
        // Out-of-view live widgets keep running while there is room, the most
        // recently seen first.
        let keep_room = cap.saturating_sub(want.len());
        live.sort();
        let evict = live.len().saturating_sub(keep_room);
        for (_, id) in live.into_iter().take(evict) {
            self.suspend_with(&id, SuspendReason::Evicted);
        }
        for id in want {
            if !matches!(self.widgets[&id].phase, Phase::Live(_)) {
                self.launch(&id);
            }
        }
    }

    fn launch(&mut self, id: &str) {
        let now = self.clock.now_ms();
        let wd = self.cfg.watchdog;
        let Some(e) = self.widgets.get_mut(id) else {
            return;
        };
        let placement = if self.hidden {
            Placement::hidden()
        } else {
            e.view.placement
        };
        match self.backend.launch(&e.spec, placement) {
            Ok(pid) => {
                e.phase = Phase::Live(Box::new(Live {
                    launched: now,
                    watchdog: Watchdog::new(wd, now),
                    init_sent: false,
                    placed: None,
                    seq: 0,
                }));
                self.events.push(HostEvent::Launched {
                    id: id.to_string(),
                    pid,
                });
                if std::mem::replace(&mut e.was_live, true) {
                    self.events.push(HostEvent::Resumed { id: id.to_string() });
                }
            }
            Err(error) => {
                e.phase = Phase::Stopped;
                self.events.push(HostEvent::LaunchFailed {
                    id: id.to_string(),
                    error,
                });
            }
        }
    }

    fn place_all(&mut self) {
        let hidden = self.hidden;
        let mut sends = Vec::new();
        for (id, e) in self.widgets.iter_mut() {
            let Phase::Live(l) = &mut e.phase else {
                continue;
            };
            let p = if hidden || !e.view.in_view {
                Placement::hidden()
            } else {
                e.view.placement
            };
            if l.placed == Some(p) {
                continue;
            }
            l.placed = Some(p);
            l.seq += 1;
            sends.push((
                id.clone(),
                ToHelper::Place {
                    seq: l.seq,
                    placement: p,
                },
            ));
        }
        for (id, m) in sends {
            let _ = self.backend.send(&id, &m);
        }
    }

    /// Hide every live widget (a menu or dialog is open over the preview) or
    /// show them again.
    pub fn set_hidden(&mut self, hidden: bool) {
        self.hidden = hidden;
        self.place_all();
    }

    pub fn set_theme(&mut self, theme: Theme) {
        self.theme = theme;
        let ids: Vec<String> = self
            .widgets
            .iter()
            .filter(|(_, e)| matches!(&e.phase, Phase::Live(l) if l.init_sent))
            .map(|(id, _)| id.clone())
            .collect();
        for id in ids {
            let _ = self.backend.send(&id, &ToHelper::Theme(self.theme.clone()));
        }
    }

    /// Give a widget input, or take it back. A widget that is not live is
    /// marked most recently seen, so it starts at the next view.
    pub fn activate(&mut self, id: &str, on: bool) {
        self.tick += 1;
        let tick = self.tick;
        let Some(e) = self.widgets.get_mut(id) else {
            return;
        };
        match &e.phase {
            Phase::Live(_) => {
                let _ = self.backend.send(id, &ToHelper::Activate { on });
            }
            Phase::Quarantined => self.events.push(HostEvent::Refused {
                id: id.to_string(),
                why: "quarantined".into(),
            }),
            _ if on => {
                e.seen = tick;
                e.view.rank = 0;
                if matches!(e.phase, Phase::Suspended(_)) {
                    e.phase = Phase::Suspended(SuspendReason::Evicted);
                }
                self.schedule();
                self.place_all();
            }
            _ => {}
        }
    }

    /// Release input from every widget.
    pub fn deactivate_all(&mut self) {
        let ids: Vec<String> = self
            .widgets
            .iter()
            .filter(|(_, e)| matches!(e.phase, Phase::Live(_)))
            .map(|(id, _)| id.clone())
            .collect();
        for id in ids {
            let _ = self.backend.send(&id, &ToHelper::Activate { on: false });
        }
    }

    /// Tear a widget down until [`WidgetHost::resume`].
    pub fn suspend(&mut self, id: &str) {
        self.suspend_with(id, SuspendReason::Requested);
    }

    fn suspend_with(&mut self, id: &str, reason: SuspendReason) {
        let Some(e) = self.widgets.get_mut(id) else {
            return;
        };
        if let Phase::Live(_) = e.phase {
            self.backend.stop(id);
            e.phase = Phase::Suspended(reason);
            self.events.push(HostEvent::Suspended {
                id: id.to_string(),
                reason,
            });
        } else if matches!(e.phase, Phase::Idle) && reason == SuspendReason::Requested {
            e.phase = Phase::Suspended(reason);
        }
    }

    /// Let a requested suspension end; the widget runs again when in view.
    pub fn resume(&mut self, id: &str) {
        if let Some(e) = self.widgets.get_mut(id) {
            if matches!(e.phase, Phase::Suspended(_)) {
                e.phase = Phase::Suspended(SuspendReason::Evicted);
                self.schedule();
                self.place_all();
            }
        }
    }

    /// Start a stopped widget again. A quarantined widget is refused.
    pub fn reload(&mut self, id: &str) {
        let Some(e) = self.widgets.get_mut(id) else {
            return;
        };
        match e.phase {
            Phase::Stopped => {
                e.phase = Phase::Idle;
                self.schedule();
                self.place_all();
            }
            Phase::Quarantined => self.events.push(HostEvent::Refused {
                id: id.to_string(),
                why: "quarantined".into(),
            }),
            _ => {}
        }
    }

    pub fn request_snapshot(&mut self, id: &str, request_id: &str) {
        if let Some(Entry {
            phase: Phase::Live(_),
            ..
        }) = self.widgets.get(id)
        {
            let _ = self.backend.send(
                id,
                &ToHelper::SnapshotRequest {
                    request_id: request_id.to_string(),
                },
            );
        }
    }

    /// Wait up to `wait` for helper traffic, act on it, run the watchdog,
    /// and return every event since the last pump.
    pub fn pump(&mut self, wait: Duration) -> Vec<HostEvent> {
        for ev in self.backend.poll(wait) {
            self.on_backend(ev);
        }
        self.watch();
        std::mem::take(&mut self.events)
    }

    fn on_backend(&mut self, ev: BackendEvent) {
        let now = self.clock.now_ms();
        match ev {
            BackendEvent::Violation { id, why } => {
                if let Some(e) = self.widgets.get_mut(&id) {
                    if matches!(e.phase, Phase::Live(_)) {
                        e.phase = Phase::Stopped;
                        let kills = e.kills;
                        self.events.push(HostEvent::Killed {
                            id: id.clone(),
                            reason: KillReason::Protocol,
                            gap_ms: None,
                            kills,
                        });
                        self.events.push(HostEvent::Dropped {
                            id,
                            by: DropBy::Host,
                            reason: why,
                        });
                    }
                }
            }
            BackendEvent::Exited { id, why } => {
                if let Some(e) = self.widgets.get_mut(&id) {
                    if matches!(e.phase, Phase::Live(_)) {
                        e.phase = Phase::Stopped;
                        self.events.push(HostEvent::Exited { id, why });
                    }
                }
            }
            BackendEvent::Message { id, msg } => self.on_message(now, id, msg),
        }
    }

    fn on_message(&mut self, now: u64, id: String, msg: FromHelper) {
        let theme = self.theme.clone();
        let Some(e) = self.widgets.get_mut(&id) else {
            return;
        };
        let Phase::Live(l) = &mut e.phase else {
            return;
        };
        match msg {
            FromHelper::Ready {
                features_off,
                sandbox,
                dead_proxy,
                ..
            } => self.events.push(HostEvent::HelperReady {
                id,
                ms: now.saturating_sub(l.launched),
                features_off,
                sandbox,
                dead_proxy,
            }),
            FromHelper::Loaded { .. } => self.events.push(HostEvent::Loaded {
                id,
                ms: now.saturating_sub(l.launched),
            }),
            FromHelper::Hb => {
                if l.watchdog.beat(now) == Some(Verdict::Responsive) {
                    self.events.push(HostEvent::Responsive { id });
                }
            }
            FromHelper::Bridge { m } => match bridge::validate(&m) {
                Ok(message) => {
                    if message == BridgeMessage::Ready && !l.init_sent {
                        l.init_sent = true;
                        let init = ToHelper::Init {
                            runtime: e.spec.runtime.clone(),
                            alt: e.spec.alt.clone(),
                            options: e.spec.options.clone(),
                            theme,
                        };
                        let _ = self.backend.send(&id, &init);
                    }
                    self.events.push(HostEvent::Bridge { id, message });
                }
                Err(reason) => self.events.push(HostEvent::Dropped {
                    id,
                    by: DropBy::Host,
                    reason: reason.to_string(),
                }),
            },
            FromHelper::Drop { why, kind } => self.events.push(HostEvent::Dropped {
                id,
                by: DropBy::Helper,
                reason: format!("{why}: {}", kind.chars().take(40).collect::<String>()),
            }),
            FromHelper::Active { on, why } => self.events.push(HostEvent::Active { id, on, why }),
            FromHelper::Placed { .. } | FromHelper::Log { .. } => {}
        }
    }

    fn watch(&mut self) {
        let now = self.clock.now_ms();
        let quarantine_after = self.cfg.watchdog.quarantine_after;
        let mut kill = Vec::new();
        for (id, e) in self.widgets.iter_mut() {
            let Phase::Live(l) = &mut e.phase else {
                continue;
            };
            match l.watchdog.check(now) {
                Some(Verdict::Unresponsive { gap_ms }) => {
                    self.events.push(HostEvent::Unresponsive {
                        id: id.clone(),
                        gap_ms,
                    })
                }
                Some(Verdict::Kill { gap_ms }) => kill.push((id.clone(), gap_ms)),
                _ => {}
            }
        }
        for (id, gap_ms) in kill {
            self.backend.stop(&id);
            let e = self.widgets.get_mut(&id).expect("listed above");
            e.kills += 1;
            self.events.push(HostEvent::Killed {
                id: id.clone(),
                reason: KillReason::Frozen,
                gap_ms: Some(gap_ms),
                kills: e.kills,
            });
            if e.kills >= quarantine_after {
                e.phase = Phase::Quarantined;
                self.events.push(HostEvent::Quarantined { id });
            } else {
                e.phase = Phase::Stopped;
            }
        }
    }

    /// End every helper.
    pub fn shutdown(&mut self) {
        self.set_widgets(Vec::new());
    }
}

impl Drop for WidgetHost {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(test)]
mod tests;
