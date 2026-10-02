use super::*;
use crate::conformance::{green, looping, spec};
use crate::fake::{FakeBackend, FakeClock, FakeState};
use crate::proto::Rect;
use std::sync::{Arc, Mutex};

fn host(cap: usize) -> (WidgetHost, Arc<Mutex<FakeState>>) {
    let clock = FakeClock::default();
    let backend = FakeBackend::new(clock.clone(), true);
    let state = backend.state();
    let cfg = HostConfig {
        live_cap: cap,
        ..HostConfig::default()
    };
    (
        WidgetHost::new(Box::new(backend), Arc::new(clock), cfg),
        state,
    )
}

fn slot(i: usize) -> Placement {
    Placement::clipped(
        Rect {
            x: 10,
            y: 10 + 100 * i as i32,
            w: 80,
            h: 60,
        },
        Rect {
            x: 0,
            y: 0,
            w: 800,
            h: 600,
        },
    )
}

fn views(ids: &[(&str, bool, u32)]) -> BTreeMap<String, View> {
    ids.iter()
        .enumerate()
        .map(|(i, (id, in_view, rank))| {
            (
                id.to_string(),
                View {
                    placement: slot(i),
                    in_view: *in_view,
                    rank: *rank,
                },
            )
        })
        .collect()
}

fn pump(h: &mut WidgetHost, ms: u64) -> Vec<HostEvent> {
    let mut out = Vec::new();
    for _ in 0..ms / 20 {
        out.extend(h.pump(Duration::from_millis(20)));
    }
    out
}

#[test]
fn nothing_runs_until_a_widget_is_in_view() {
    let (mut h, st) = host(6);
    h.set_widgets(vec![spec("a", green())]);
    pump(&mut h, 200);
    assert_eq!(st.lock().unwrap().alive(), 0);
    assert_eq!(h.state("a"), Some(WidgetState::Idle));
    h.view(&views(&[("a", true, 0)]));
    let evs = pump(&mut h, 400);
    assert!(evs.iter().any(|e| matches!(e, HostEvent::Loaded { .. })));
    assert_eq!(h.state("a"), Some(WidgetState::Live));
}

#[test]
fn the_cap_prefers_in_view_rank_and_evicts_the_least_recently_seen() {
    let (mut h, st) = host(2);
    h.set_widgets(
        ["a", "b", "c", "d"]
            .iter()
            .map(|i| spec(i, green()))
            .collect(),
    );
    h.view(&views(&[("a", true, 0), ("b", true, 1), ("c", true, 2)]));
    assert_eq!(h.live_count(), 2);
    assert_eq!(h.state("c"), Some(WidgetState::Idle), "rank 2 waits");
    // a scrolls away, d comes in: d takes a's slot; b stays.
    h.view(&views(&[("a", false, 9), ("b", true, 0), ("d", true, 1)]));
    let evs = pump(&mut h, 100);
    assert_eq!(h.state("a"), Some(WidgetState::Suspended));
    assert_eq!(h.state("d"), Some(WidgetState::Live));
    assert!(evs.iter().any(|e| matches!(e,
        HostEvent::Suspended { id, reason: SuspendReason::Evicted } if id == "a")));
    assert_eq!(st.lock().unwrap().alive(), 2);
}

#[test]
fn an_out_of_view_widget_keeps_running_while_there_is_room() {
    let (mut h, _) = host(3);
    h.set_widgets(["a", "b"].iter().map(|i| spec(i, green())).collect());
    h.view(&views(&[("a", true, 0), ("b", true, 1)]));
    h.view(&views(&[("a", false, 0), ("b", true, 0)]));
    assert_eq!(h.state("a"), Some(WidgetState::Live));
    // Out of view means hidden, not torn down.
    let st = pump(&mut h, 20);
    assert!(!st.iter().any(|e| matches!(e, HostEvent::Suspended { .. })));
}

#[test]
fn placements_are_sent_once_per_change_and_hidden_under_a_popup() {
    let (mut h, st) = host(6);
    h.set_widgets(vec![spec("a", green())]);
    let v = views(&[("a", true, 0)]);
    h.view(&v);
    h.view(&v);
    let places = |st: &Arc<Mutex<FakeState>>| {
        st.lock()
            .unwrap()
            .sent
            .iter()
            .filter_map(|(_, m)| match m {
                ToHelper::Place { placement, .. } => Some(*placement),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(places(&st).len(), 1);
    h.set_hidden(true);
    assert_eq!(places(&st).last(), Some(&Placement::hidden()));
    h.set_hidden(false);
    assert_eq!(places(&st).last(), Some(&slot(0)));
}

#[test]
fn init_is_sent_once_with_the_current_theme() {
    let (mut h, st) = host(6);
    h.set_theme(Theme {
        mode: crate::proto::ThemeMode::Dark,
        tokens: Default::default(),
    });
    h.set_widgets(vec![spec("a", green())]);
    h.view(&views(&[("a", true, 0)]));
    let evs = pump(&mut h, 400);
    let inits: Vec<_> = st
        .lock()
        .unwrap()
        .sent
        .iter()
        .filter(|(_, m)| matches!(m, ToHelper::Init { .. }))
        .cloned()
        .collect();
    assert_eq!(inits.len(), 1);
    assert!(evs.iter().any(|e| matches!(e,
        HostEvent::Bridge { message: BridgeMessage::Status { message: Some(m), .. }, .. }
            if m.starts_with("init dark"))));
}

#[test]
fn an_unchanged_spec_keeps_running_and_a_changed_one_restarts() {
    let (mut h, st) = host(6);
    h.set_widgets(vec![spec("a", green()), spec("b", green())]);
    h.view(&views(&[("a", true, 0), ("b", true, 1)]));
    pump(&mut h, 200);
    let mut b2 = spec("b", green());
    b2.csp = "default-src 'none'".into();
    h.set_widgets(vec![spec("a", green()), b2]);
    assert_eq!(h.state("a"), Some(WidgetState::Live));
    assert_eq!(h.state("b"), Some(WidgetState::Idle));
    h.view(&views(&[("a", true, 0), ("b", true, 1)]));
    let launches = st.lock().unwrap().launches.clone();
    assert_eq!(launches["a"], 1);
    assert_eq!(launches["b"], 2);
}

#[test]
fn revocation_stops_everything_at_once() {
    let (mut h, st) = host(6);
    h.set_widgets(vec![spec("a", green()), spec("b", green())]);
    h.view(&views(&[("a", true, 0), ("b", true, 1)]));
    pump(&mut h, 200);
    h.set_widgets(Vec::new());
    assert_eq!(st.lock().unwrap().alive(), 0);
    assert!(h.ids().is_empty());
}

#[test]
fn a_frozen_widget_is_killed_then_quarantined_on_the_second_kill() {
    let (mut h, st) = host(6);
    h.set_widgets(vec![spec("l", looping(100))]);
    h.view(&views(&[("l", true, 0)]));
    let evs = pump(&mut h, 7_000);
    assert!(evs
        .iter()
        .any(|e| matches!(e, HostEvent::Unresponsive { .. })));
    assert!(evs
        .iter()
        .any(|e| matches!(e, HostEvent::Killed { kills: 1, .. })));
    assert_eq!(h.state("l"), Some(WidgetState::Stopped));
    // A stopped widget does not restart by scrolling, only by reload.
    h.view(&views(&[("l", true, 0)]));
    assert_eq!(st.lock().unwrap().alive(), 0);
    h.reload("l");
    let evs = pump(&mut h, 7_000);
    assert!(evs
        .iter()
        .any(|e| matches!(e, HostEvent::Quarantined { .. })));
    assert_eq!(h.state("l"), Some(WidgetState::Quarantined));
}

#[test]
fn an_unsupported_platform_runs_nothing() {
    let mut h = WidgetHost::new(
        Box::new(crate::Unavailable("windows")),
        Arc::new(crate::SystemClock::default()),
        HostConfig::default(),
    );
    h.set_widgets(vec![spec("a", green())]);
    h.view(&views(&[("a", true, 0)]));
    assert_eq!(h.state("a"), Some(WidgetState::Idle));
    assert!(!h.capabilities().runs_widgets());
}

#[test]
fn host_events_have_stable_wire_names() {
    let v = serde_json::to_value(HostEvent::Killed {
        id: "a".into(),
        reason: KillReason::Frozen,
        gap_ms: Some(5001),
        kills: 1,
    })
    .unwrap();
    assert_eq!(
        v,
        serde_json::json!({"kind": "killed", "id": "a", "reason": "frozen", "gapMs": 5001, "kills": 1})
    );
}
