//! What the X11 embedding shows and where input goes, read back from the X
//! server: the slot is drawn exactly, clipped by the bounding shape, hidden
//! under a popup; an inactive widget lets clicks and wheel fall through to
//! the editor window, an active one takes them, and Escape releases it.
//! Needs an X display with XTEST (run under Xvfb by `e2e/widgets-run.sh`).

#![cfg(target_os = "linux")]

use maleficium_widget_host::conformance::{green, spec};
use maleficium_widget_host::linux::{LinuxBackend, LinuxConfig};
use maleficium_widget_host::proto::{ActiveWhy, Placement, Rect};
use maleficium_widget_host::{HostConfig, HostEvent, SystemClock, View, WidgetHost};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use x11::{xlib, xtest};

const BG: u32 = 0x00dd_dddd;
const GREEN: u32 = 0x0000_ff00;
const PANE: Rect = Rect {
    x: 0,
    y: 100,
    w: 1200,
    h: 700,
};

struct X {
    dpy: *mut xlib::Display,
    root: xlib::Window,
    parent: xlib::Window,
}

impl X {
    fn open() -> Self {
        unsafe {
            let dpy = xlib::XOpenDisplay(std::ptr::null());
            assert!(!dpy.is_null(), "no X display: run under Xvfb");
            let root = xlib::XDefaultRootWindow(dpy);
            let parent = xlib::XCreateSimpleWindow(dpy, root, 0, 0, 1200, 900, 0, 0, BG.into());
            xlib::XSelectInput(dpy, parent, xlib::ButtonPressMask);
            xlib::XMapWindow(dpy, parent);
            xlib::XSync(dpy, 0);
            Self { dpy, root, parent }
        }
    }

    fn pixel(&self, x: i32, y: i32) -> u32 {
        unsafe {
            let img = xlib::XGetImage(self.dpy, self.root, x, y, 1, 1, !0, xlib::ZPixmap);
            assert!(!img.is_null());
            let p = xlib::XGetPixel(img, 0, 0) as u32 & 0x00ff_ffff;
            xlib::XDestroyImage(img);
            p
        }
    }

    /// Button presses the editor window received since the last call.
    fn presses(&self) -> Vec<u32> {
        let mut out = Vec::new();
        unsafe {
            xlib::XSync(self.dpy, 0);
            while xlib::XPending(self.dpy) > 0 {
                let mut ev: xlib::XEvent = std::mem::zeroed();
                xlib::XNextEvent(self.dpy, &mut ev);
                if ev.get_type() == xlib::ButtonPress {
                    out.push(ev.button.button);
                }
            }
        }
        out
    }

    fn button(&self, x: i32, y: i32, button: u32) {
        unsafe {
            xtest::XTestFakeMotionEvent(self.dpy, -1, x, y, 0);
            xtest::XTestFakeButtonEvent(self.dpy, button, 1, 0);
            xtest::XTestFakeButtonEvent(self.dpy, button, 0, 0);
            xlib::XSync(self.dpy, 0);
        }
    }

    fn key(&self, sym: u32) {
        unsafe {
            let code = xlib::XKeysymToKeycode(self.dpy, sym.into()) as u32;
            xtest::XTestFakeKeyEvent(self.dpy, code, 1, 0);
            xtest::XTestFakeKeyEvent(self.dpy, code, 0, 0);
            xlib::XSync(self.dpy, 0);
        }
    }
}

fn pump_until(h: &mut WidgetHost, ms: u64, mut f: impl FnMut(&HostEvent) -> bool) -> bool {
    let end = Instant::now() + Duration::from_millis(ms);
    while Instant::now() < end {
        if h.pump(Duration::from_millis(20)).iter().any(&mut f) {
            return true;
        }
    }
    false
}

fn settle(h: &mut WidgetHost, ms: u64) {
    pump_until(h, ms, |_| false);
}

fn show(h: &mut WidgetHost, slot: Rect) {
    let mut v = BTreeMap::new();
    v.insert(
        "w".to_string(),
        View {
            placement: Placement::clipped(slot, PANE),
            in_view: true,
            rank: 0,
        },
    );
    h.view(&v);
}

#[test]
#[ignore = "needs an X display with XTEST; run by e2e/widgets-run.sh"]
fn the_embedded_view_draws_its_slot_and_routes_input_by_activation() {
    let x = X::open();
    let mut h = WidgetHost::new(
        Box::new(LinuxBackend::new(LinuxConfig {
            helper: env!("CARGO_BIN_EXE_maleficium-widget-helper").into(),
            helper_args: Vec::new(),
            parent: x.parent,
            network_namespaces: true,
        })),
        Arc::new(SystemClock::default()),
        HostConfig::default(),
    );
    h.set_widgets(vec![spec("w", green())]);
    let slot = Rect {
        x: 100,
        y: 150,
        w: 400,
        h: 200,
    };
    show(&mut h, slot);
    assert!(pump_until(&mut h, 5_000, |e| matches!(
        e,
        HostEvent::Bridge { .. }
    ) && serde_json::to_string(e)
        .unwrap()
        .contains("init ")));
    settle(&mut h, 500);

    // Drawn exactly over the slot, nowhere else.
    assert_eq!(x.pixel(100, 150), GREEN, "top-left corner");
    assert_eq!(x.pixel(499, 349), GREEN, "bottom-right corner");
    assert_eq!(x.pixel(99, 150), BG, "left of the slot");
    assert_eq!(x.pixel(500, 349), BG, "right of the slot");
    assert_eq!(x.pixel(100, 350), BG, "below the slot");

    // Half above the pane: only the part inside the pane is drawn.
    show(&mut h, Rect { y: 0, ..slot });
    settle(&mut h, 300);
    assert_eq!(x.pixel(200, 99), BG, "clipped above the pane");
    assert_eq!(x.pixel(200, 100), GREEN, "drawn inside the pane");
    assert_eq!(x.pixel(200, 199), GREEN);

    // A popup over the preview hides every widget; closing it shows them.
    show(&mut h, slot);
    h.set_hidden(true);
    settle(&mut h, 300);
    assert_eq!(x.pixel(300, 250), BG, "hidden under a popup");
    h.set_hidden(false);
    settle(&mut h, 300);
    assert_eq!(x.pixel(300, 250), GREEN, "shown again");

    // Inactive: click and wheel fall through to the editor window.
    x.presses();
    x.button(300, 250, 1);
    x.button(300, 250, 5);
    settle(&mut h, 200);
    assert_eq!(
        x.presses(),
        vec![1, 5],
        "the editor got the click and the wheel"
    );

    // Active: the widget takes them, Escape gives them back.
    h.activate("w", true);
    assert!(pump_until(&mut h, 2_000, |e| matches!(
        e,
        HostEvent::Active { on: true, .. }
    )));
    x.button(300, 250, 5);
    x.button(300, 250, 1);
    settle(&mut h, 200);
    assert_eq!(
        x.presses(),
        Vec::<u32>::new(),
        "the active widget took the input"
    );
    x.key(x11::keysym::XK_Escape);
    assert!(
        pump_until(&mut h, 2_000, |e| matches!(
            e,
            HostEvent::Active {
                on: false,
                why: ActiveWhy::Escape,
                ..
            }
        )),
        "Escape released the widget"
    );
    x.button(300, 250, 5);
    settle(&mut h, 200);
    assert_eq!(x.presses(), vec![5], "the wheel reaches the editor again");
    h.shutdown();
}
