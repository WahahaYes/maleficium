//! The shared conformance suite against the real Linux helper. Needs an X
//! display (run under Xvfb by `e2e/widgets-run.sh`), so it is ignored by a
//! plain `cargo test`.

#![cfg(target_os = "linux")]

use maleficium_widget_host::conformance::{Outcome, Platform};
use maleficium_widget_host::linux::{tagged, LinuxBackend, LinuxConfig};
use maleficium_widget_host::proto::{Placement, Rect};
use maleficium_widget_host::{HostConfig, SystemClock, WidgetHost};
use std::sync::Arc;
use x11::xlib;

struct Linux {
    parent: u64,
    netns: bool,
    sessions: Vec<String>,
}

impl Platform for Linux {
    fn host(&mut self, cfg: HostConfig) -> WidgetHost {
        let b = LinuxBackend::new(LinuxConfig {
            helper: env!("CARGO_BIN_EXE_maleficium-widget-helper").into(),
            helper_args: Vec::new(),
            parent: self.parent,
            network_namespaces: self.netns,
        });
        self.sessions.push(b.session().to_string());
        WidgetHost::new(Box::new(b), Arc::new(SystemClock::default()), cfg)
    }

    fn slot(&self, i: usize) -> Placement {
        let slot = Rect {
            x: 20 + 420 * (i % 2) as i32,
            y: 20 + 220 * (i / 2) as i32,
            w: 400,
            h: 200,
        };
        Placement::clipped(
            slot,
            Rect {
                x: 0,
                y: 0,
                w: 1200,
                h: 900,
            },
        )
    }

    fn stray_processes(&self) -> usize {
        self.sessions.iter().map(|s| tagged(s, true).len()).sum()
    }
}

/// A plain mapped X window standing in for the editor.
fn parent_window() -> u64 {
    unsafe {
        let dpy = xlib::XOpenDisplay(std::ptr::null());
        assert!(!dpy.is_null(), "no X display: run under Xvfb");
        let root = xlib::XDefaultRootWindow(dpy);
        let w = xlib::XCreateSimpleWindow(dpy, root, 0, 0, 1200, 900, 0, 0, 0x00dd_dddd);
        xlib::XMapWindow(dpy, w);
        xlib::XFlush(dpy);
        // The connection is never closed, so the window outlives the cases.
        w
    }
}

#[test]
#[ignore = "needs an X display; run by e2e/widgets-run.sh"]
fn the_linux_helper_passes_the_conformance_suite() {
    // CONFORMANCE_NETNS=0 runs the dead-proxy fallback.
    let netns = std::env::var("CONFORMANCE_NETNS").map_or(true, |v| v != "0");
    let mut p = Linux {
        parent: parent_window(),
        netns,
        sessions: Vec::new(),
    };
    let only = std::env::var("CONFORMANCE_ONLY").ok();
    let caps = p.host(HostConfig::default()).capabilities().clone();
    println!("capabilities: {}", serde_json::to_string(&caps).unwrap());
    let mut failed = Vec::new();
    for (name, case) in maleficium_widget_host::conformance::CASES {
        if only
            .as_deref()
            .is_some_and(|o| !o.split(',').any(|x| x == *name))
        {
            continue;
        }
        let t = std::time::Instant::now();
        let o = case(&mut p);
        println!("conformance {name}: {o:?} ({} ms)", t.elapsed().as_millis());
        if o != Outcome::Pass {
            failed.push(name);
        }
    }
    assert!(failed.is_empty(), "failed: {failed:?}");
    assert_eq!(
        p.stray_processes(),
        0,
        "helper processes outlived the suite"
    );
}
