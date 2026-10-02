//! The desktop shell's widget adapter: the only widget code that knows
//! Tauri. It finds the editor window's X11 id, runs a [`WidgetHost`] on its
//! own thread, forwards the preview's commands to it, and emits every host
//! event to the window as a typed bus event (`widget-event`). Approvals go
//! through [`widgets_approve`], a user action the automation surface has no
//! equivalent of.

use maleficium_core::widget_live::{self, LiveWidget, WidgetApprovalScope, WidgetSession};
use maleficium_core::Core;
use maleficium_events::{
    Actor, AppEvent, BusEvent, EventKind, EventScope, WidgetKillReason, WidgetSuspendReason,
};
use maleficium_widget_host::bridge::BridgeMessage;
use maleficium_widget_host::proto::{Hardening, Placement, Rect, Theme, ThemeMode};
use maleficium_widget_host::{
    HostConfig, HostEvent, KillReason, SourceBytes, SuspendReason, SystemClock, View,
    WidgetBackend, WidgetHost, WidgetSpec,
};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, State};

/// Where the preview puts one widget, in device px of the webview's
/// viewport.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewItem {
    id: String,
    slot: Rect,
    clip: Rect,
    visible: bool,
    in_view: bool,
    rank: u32,
}

enum Cmd {
    Set(Vec<WidgetSpec>),
    View(BTreeMap<String, View>),
    Activate(String, bool),
    DeactivateAll,
    Hidden(bool),
    Theme(Theme),
    Reload(String),
}

/// The editor window as the widget host sees it.
#[derive(Debug, Clone, Copy, Default)]
struct Window {
    /// The X11 id of the toplevel's window; 0 where there is none.
    xid: u64,
    /// The webview's offset inside it, device px.
    dx: i32,
    dy: i32,
}

struct Shell {
    tx: Sender<Cmd>,
    missing: Vec<String>,
    window: Window,
    /// Per widget: may reach the network, egress cut below the engine.
    meta: Arc<Mutex<BTreeMap<String, (bool, bool)>>>,
}

/// The adapter's state: the host thread, once started.
#[derive(Default)]
pub struct Widgets {
    shell: Mutex<Option<Shell>>,
}

#[cfg(target_os = "linux")]
fn window_of(app: &AppHandle) -> Window {
    use gtk::prelude::*;
    let Some(w) = app.get_webview_window("main") else {
        return Window::default();
    };
    let (Ok(gw), Ok(vbox)) = (w.gtk_window(), w.default_vbox()) else {
        return Window::default();
    };
    let Some(xw) = gw
        .window()
        .and_then(|g| g.downcast::<gdkx11::X11Window>().ok())
    else {
        return Window::default();
    };
    let s = gw.scale_factor().max(1);
    let (x, y) = vbox.translate_coordinates(&gw, 0, 0).unwrap_or((0, 0));
    Window {
        xid: xw.xid(),
        dx: x * s,
        dy: y * s,
    }
}

#[cfg(not(target_os = "linux"))]
fn window_of(_: &AppHandle) -> Window {
    Window::default()
}

#[cfg(target_os = "linux")]
fn backend(window: Window) -> Box<dyn WidgetBackend> {
    use maleficium_widget_host::linux::{LinuxBackend, LinuxConfig};
    match std::env::current_exe() {
        Ok(exe) if window.xid != 0 => Box::new(LinuxBackend::new(LinuxConfig {
            helper: exe,
            helper_args: vec!["--widget-helper".into()],
            parent: window.xid,
            network_namespaces: true,
        })),
        _ => Box::new(maleficium_widget_host::Unavailable(
            "linux without an X11 window",
        )),
    }
}

#[cfg(not(target_os = "linux"))]
fn backend(_: Window) -> Box<dyn WidgetBackend> {
    Box::new(maleficium_widget_host::Unavailable(std::env::consts::OS))
}

/// Gives the keyboard back to the editor's own window: inside one toplevel,
/// X focus stays in a widget's child window until moved.
#[cfg(target_os = "linux")]
fn refocus_editor(app: &AppHandle) {
    use gtk::prelude::*;
    let Some(w) = app.get_webview_window("main") else {
        return;
    };
    let Ok(gw) = w.gtk_window() else {
        return;
    };
    let Some(xw) = gw
        .window()
        .and_then(|g| g.downcast::<gdkx11::X11Window>().ok())
    else {
        return;
    };
    let display = gw.display();
    let Ok(xd) = display.downcast::<gdkx11::X11Display>() else {
        return;
    };
    unsafe {
        let dpy = gdkx11::ffi::gdk_x11_display_get_xdisplay(
            gtk::glib::translate::ToGlibPtr::to_glib_none(&xd).0,
        ) as *mut x11::xlib::Display;
        x11::xlib::XSetInputFocus(
            dpy,
            xw.xid(),
            x11::xlib::RevertToParent,
            x11::xlib::CurrentTime,
        );
        x11::xlib::XFlush(dpy);
    }
}

#[cfg(not(target_os = "linux"))]
fn refocus_editor(_: &AppHandle) {}

fn bus(kind: EventKind, message: String, event: AppEvent) -> BusEvent {
    BusEvent {
        at: maleficium_core::eventlog::now_ms(),
        scope: EventScope::Preview,
        kind,
        actor: Actor::System,
        message,
        event,
    }
}

/// The bus event for a host event, where the app reports one.
fn to_bus(e: &HostEvent, meta: &BTreeMap<String, (bool, bool)>) -> Option<BusEvent> {
    use EventKind::{Error, Info, Success, Warn};
    Some(match e.clone() {
        HostEvent::Launched { id, .. } => {
            let (network, contained) = meta.get(&id).copied().unwrap_or((false, true));
            bus(
                Info,
                format!("widget {id} started"),
                AppEvent::WidgetLaunched {
                    id,
                    network,
                    contained,
                },
            )
        }
        HostEvent::Loaded { id, ms } => bus(
            Success,
            format!("widget {id} loaded in {ms} ms"),
            AppEvent::WidgetLoaded { id, ms },
        ),
        HostEvent::Bridge {
            id,
            message: BridgeMessage::Status { state, message, .. },
        } => {
            let state = serde_json::to_value(state)
                .ok()
                .and_then(|v| v.as_str().map(str::to_string))
                .unwrap_or_default();
            bus(
                if state == "error" { Error } else { Info },
                format!("widget {id} {state}"),
                AppEvent::WidgetStatus { id, state, message },
            )
        }
        HostEvent::Bridge { .. } => return None,
        HostEvent::Dropped { id, by, reason } => {
            let by = serde_json::to_value(by)
                .ok()
                .and_then(|v| v.as_str().map(str::to_string))
                .unwrap_or_default();
            bus(
                Warn,
                format!("widget {id}: message dropped ({reason})"),
                AppEvent::WidgetDropped { id, by, reason },
            )
        }
        HostEvent::Unresponsive { id, gap_ms } => bus(
            Warn,
            format!("widget {id} is not responding"),
            AppEvent::WidgetUnresponsive { id, gap_ms },
        ),
        HostEvent::Responsive { id } => bus(
            Info,
            format!("widget {id} responds again"),
            AppEvent::WidgetResponsive { id },
        ),
        HostEvent::Killed {
            id,
            reason,
            gap_ms,
            kills,
        } => bus(
            Error,
            format!("widget {id} stopped"),
            AppEvent::WidgetKilled {
                id,
                reason: match reason {
                    KillReason::Frozen => WidgetKillReason::Frozen,
                    KillReason::Protocol => WidgetKillReason::Protocol,
                },
                gap_ms,
                kills,
            },
        ),
        HostEvent::Quarantined { id } => bus(
            Error,
            format!("widget {id} stays a poster this session"),
            AppEvent::WidgetQuarantined { id },
        ),
        HostEvent::Suspended { id, reason } => bus(
            Info,
            format!("widget {id} suspended"),
            AppEvent::WidgetSuspended {
                id,
                reason: match reason {
                    SuspendReason::Evicted => WidgetSuspendReason::Evicted,
                    SuspendReason::Requested => WidgetSuspendReason::Requested,
                },
            },
        ),
        HostEvent::Resumed { id } => bus(
            Info,
            format!("widget {id} resumed"),
            AppEvent::WidgetResumed { id },
        ),
        HostEvent::Active { id, on, why } => {
            let why = serde_json::to_value(why)
                .ok()
                .and_then(|v| v.as_str().map(str::to_string))
                .unwrap_or_default();
            bus(
                Info,
                format!("widget {id} {}", if on { "active" } else { "released" }),
                AppEvent::WidgetActive { id, on, why },
            )
        }
        HostEvent::Exited { id, why } => bus(
            Error,
            format!("widget {id} exited"),
            AppEvent::WidgetExited { id, why },
        ),
        HostEvent::LaunchFailed { id, error } => bus(
            Error,
            format!("widget {id} could not start"),
            AppEvent::WidgetLaunchFailed { id, error },
        ),
        HostEvent::HelperReady { .. } | HostEvent::Refused { .. } => return None,
    })
}

fn run_host(
    app: AppHandle,
    rx: Receiver<Cmd>,
    backend: Box<dyn WidgetBackend>,
    meta: Arc<Mutex<BTreeMap<String, (bool, bool)>>>,
) {
    let mut host = WidgetHost::new(
        backend,
        Arc::new(SystemClock::default()),
        HostConfig::default(),
    );
    let mut idle = true;
    loop {
        // With nothing to run, sleep until the preview asks for something.
        let first = if idle {
            match rx.recv() {
                Ok(c) => Some(c),
                Err(_) => return,
            }
        } else {
            None
        };
        let mut cmds: Vec<Cmd> = first.into_iter().collect();
        loop {
            match rx.try_recv() {
                Ok(c) => cmds.push(c),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return,
            }
        }
        let busy = !cmds.is_empty();
        for c in cmds {
            match c {
                Cmd::Set(specs) => host.set_widgets(specs),
                Cmd::View(v) => host.view(&v),
                Cmd::Activate(id, on) => host.activate(&id, on),
                Cmd::DeactivateAll => host.deactivate_all(),
                Cmd::Hidden(h) => host.set_hidden(h),
                Cmd::Theme(t) => host.set_theme(t),
                Cmd::Reload(id) => host.reload(&id),
            }
        }
        let wait = Duration::from_millis(if busy { 0 } else { 8 });
        let meta_now = meta.lock().map(|m| m.clone()).unwrap_or_default();
        for e in host.pump(wait) {
            if let Some(b) = to_bus(&e, &meta_now) {
                let _ = app.emit("widget-event", b);
            }
        }
        idle = host.ids().is_empty();
    }
}

fn spec_of(w: LiveWidget) -> WidgetSpec {
    let hardening = if w.network {
        Hardening {
            dead_proxy: false,
            contain_egress: false,
            ..Hardening::default()
        }
    } else {
        Hardening::default()
    };
    WidgetSpec {
        id: w.id,
        document: w.document,
        csp: w.csp,
        runtime: w.runtime,
        alt: w.alt,
        options: w.options,
        sources: w
            .sources
            .into_iter()
            .map(|s| SourceBytes {
                role: s.role,
                name: s.name,
                mime: s.mime,
                sha256: s.sha256,
                bytes: s.bytes,
            })
            .collect(),
        hardening,
    }
}

fn theme(dark: bool) -> Theme {
    Theme {
        mode: if dark {
            ThemeMode::Dark
        } else {
            ThemeMode::Light
        },
        tokens: widget_live::house_tokens(dark),
    }
}

/// Starts the host thread on first use (the window id is read on the main
/// thread).
fn shell<'a>(
    app: &AppHandle,
    w: &'a Widgets,
) -> Result<std::sync::MutexGuard<'a, Option<Shell>>, String> {
    let mut guard = w.shell.lock().map_err(|_| "widget shell lock poisoned")?;
    if guard.is_none() {
        let (wtx, wrx) = channel();
        let a = app.clone();
        app.run_on_main_thread(move || {
            let _ = wtx.send(window_of(&a));
        })
        .map_err(|e| e.to_string())?;
        let window = wrx
            .recv_timeout(Duration::from_secs(5))
            .map_err(|_| "the editor window did not answer")?;
        let b = backend(window);
        let missing = b
            .capabilities()
            .missing()
            .into_iter()
            .map(str::to_string)
            .collect();
        let (tx, rx) = channel();
        let meta = Arc::new(Mutex::new(BTreeMap::new()));
        let (a, m) = (app.clone(), meta.clone());
        std::thread::Builder::new()
            .name("widget-host".into())
            .spawn(move || run_host(a, rx, b, m))
            .map_err(|e| e.to_string())?;
        *guard = Some(Shell {
            tx,
            missing,
            window,
            meta,
        });
    }
    Ok(guard)
}

fn session(
    app: &AppHandle,
    cx: &Core,
    w: &Widgets,
    root_id: &str,
    main_rel: &str,
) -> Result<WidgetSession, String> {
    let plan = widget_live::live_plan(cx, root_id, main_rel)?;
    let guard = shell(app, w)?;
    let s = guard.as_ref().ok_or("no widget shell")?;
    if !s.missing.is_empty() && !plan.entries.is_empty() {
        let _ = app.emit(
            "widget-event",
            bus(
                EventKind::Warn,
                "widgets show as posters: the widget host is unavailable here".into(),
                AppEvent::WidgetsUnavailable {
                    missing: s.missing.clone(),
                },
            ),
        );
    }
    if let Ok(mut m) = s.meta.lock() {
        *m = plan
            .live
            .iter()
            .map(|l| (l.id.clone(), (l.network, !l.network)))
            .collect();
    }
    let specs = if s.missing.is_empty() {
        plan.live.into_iter().map(spec_of).collect()
    } else {
        Vec::new()
    };
    let _ = s.tx.send(Cmd::Set(specs));
    Ok(WidgetSession {
        approval: plan.approval,
        entries: plan.entries,
        unavailable: s.missing.clone(),
    })
}

/// The preview opened a compiled paper: what its widgets do here, and the
/// live ones start as they come into view.
#[tauri::command(async)]
pub fn widgets_open(
    app: AppHandle,
    cx: State<'_, Core>,
    w: State<'_, Widgets>,
    root_id: String,
    main_rel: String,
    dark: bool,
) -> Result<WidgetSession, String> {
    let s = session(&app, &cx, &w, &root_id, &main_rel)?;
    send(&w, Cmd::Theme(theme(dark)));
    Ok(s)
}

/// The user granted or revoked one approval: stored, reported, and applied
/// at once.
#[tauri::command(async)]
pub fn widgets_approve(
    app: AppHandle,
    cx: State<'_, Core>,
    w: State<'_, Widgets>,
    root_id: String,
    main_rel: String,
    scope: WidgetApprovalScope,
    granted: bool,
) -> Result<WidgetSession, String> {
    let a = widget_live::set_approval(&cx, &root_id, scope, granted)?;
    let ev = widget_live::approval_event(&root_id, scope, granted, &a);
    let _ = app.emit("widget-event", ev);
    session(&app, &cx, &w, &root_id, &main_rel)
}

fn send(w: &Widgets, c: Cmd) {
    if let Ok(g) = w.shell.lock() {
        if let Some(s) = g.as_ref() {
            let _ = s.tx.send(c);
        }
    }
}

fn offset(w: &Widgets) -> (i32, i32) {
    w.shell
        .lock()
        .ok()
        .and_then(|g| g.as_ref().map(|s| (s.window.dx, s.window.dy)))
        .unwrap_or((0, 0))
}

#[tauri::command]
pub fn widgets_view(w: State<'_, Widgets>, items: Vec<ViewItem>) {
    let (dx, dy) = offset(&w);
    let shift = |r: Rect| Rect {
        x: r.x + dx,
        y: r.y + dy,
        ..r
    };
    let views = items
        .into_iter()
        .map(|i| {
            (
                i.id,
                View {
                    placement: Placement {
                        slot: shift(i.slot),
                        clip: shift(i.clip),
                        visible: i.visible,
                    },
                    in_view: i.in_view,
                    rank: i.rank,
                },
            )
        })
        .collect();
    send(&w, Cmd::View(views));
}

#[tauri::command]
pub fn widgets_activate(w: State<'_, Widgets>, id: String, on: bool) {
    send(&w, Cmd::Activate(id, on));
}

/// Takes input back from every widget and returns the keyboard to the
/// editor.
#[tauri::command]
pub fn widgets_deactivate_all(app: AppHandle, w: State<'_, Widgets>) {
    send(&w, Cmd::DeactivateAll);
    refocus_editor(&app);
}

#[tauri::command]
pub fn widgets_hidden(w: State<'_, Widgets>, hidden: bool) {
    send(&w, Cmd::Hidden(hidden));
}

#[tauri::command]
pub fn widgets_theme(w: State<'_, Widgets>, dark: bool) {
    send(&w, Cmd::Theme(theme(dark)));
}

#[tauri::command]
pub fn widgets_reload(w: State<'_, Widgets>, id: String) {
    send(&w, Cmd::Reload(id));
}

/// The preview left the paper: every helper ends.
#[tauri::command]
pub fn widgets_close(w: State<'_, Widgets>) {
    send(&w, Cmd::Set(Vec::new()));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_network_widget_drops_only_the_egress_containment() {
        let mut lw = LiveWidget {
            id: "a".into(),
            document: String::new(),
            csp: String::new(),
            runtime: String::new(),
            alt: String::new(),
            options: serde_json::json!({}),
            sources: Vec::new(),
            network: false,
        };
        assert_eq!(spec_of(lw.clone()).hardening, Hardening::default());
        lw.network = true;
        let h = spec_of(lw).hardening;
        assert!(
            h.csp_header && h.speculative_off,
            "the policy and the switch stay on"
        );
        assert!(!h.dead_proxy && !h.contain_egress);
    }

    #[test]
    fn every_reported_host_event_maps_to_a_typed_bus_event() {
        let meta = BTreeMap::from([("a".to_string(), (true, false))]);
        let e = to_bus(
            &HostEvent::Launched {
                id: "a".into(),
                pid: Some(1),
            },
            &meta,
        )
        .unwrap();
        assert_eq!(
            e.event,
            AppEvent::WidgetLaunched {
                id: "a".into(),
                network: true,
                contained: false
            }
        );
        let e = to_bus(
            &HostEvent::Killed {
                id: "a".into(),
                reason: KillReason::Frozen,
                gap_ms: Some(5001),
                kills: 1,
            },
            &meta,
        )
        .unwrap();
        let v = serde_json::to_value(&e.event).unwrap();
        assert_eq!(v["action"], "widget.killed");
        assert_eq!(v["gapMs"], 5001);
    }
}
