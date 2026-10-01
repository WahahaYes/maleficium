//! What a platform implementation of the widget host can do. The app and the
//! conformance suite read this descriptor, never the platform: a missing
//! capability is reported as missing and the app shows posters instead.

use serde::{Deserialize, Serialize};

/// How a live widget's view reaches the editor window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Embedding {
    /// A child window of the editor's X11 window, owned by another process.
    X11Child,
    /// A native subview of the editor's view hierarchy.
    NativeSubview,
    /// Frames rendered off screen and drawn by the editor.
    Offscreen,
    None,
}

/// Where widget code runs relative to the editor and to other widgets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProcessIsolation {
    /// One helper process per widget, owning its engine processes.
    PerWidgetProcess,
    /// The engine gives each view its own content process.
    EnginePerView,
    None,
}

/// What stops a widget reaching the network before its network approval.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EgressContainment {
    /// An empty network namespace: no route exists.
    NetworkNamespace,
    /// A dead proxy plus speculative loading off and the CSP header.
    DeadProxy,
    None,
}

/// How a frozen widget is detected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WatchdogSignal {
    /// A beat from the widget page's main thread.
    Heartbeat,
    None,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Capabilities {
    /// `linux`, `windows`, `macos`, `fake`.
    pub platform: String,
    pub embedding: Embedding,
    pub process_isolation: ProcessIsolation,
    pub egress: EgressContainment,
    /// The engine's speculative loading (preconnect) can be turned off.
    pub speculative_loading_switch: bool,
    pub watchdog: WatchdogSignal,
    /// The implementation passed the conformance suite on this platform.
    pub verified: bool,
}

impl Capabilities {
    /// A platform with no widget host: every capability missing.
    pub fn unsupported(platform: &str) -> Self {
        Self {
            platform: platform.to_string(),
            embedding: Embedding::None,
            process_isolation: ProcessIsolation::None,
            egress: EgressContainment::None,
            speculative_loading_switch: false,
            watchdog: WatchdogSignal::None,
            verified: false,
        }
    }

    /// The capabilities a widget needs to run, by name, that are missing.
    pub fn missing(&self) -> Vec<&'static str> {
        let mut out = Vec::new();
        if self.embedding == Embedding::None {
            out.push("embedding");
        }
        if self.process_isolation == ProcessIsolation::None {
            out.push("process-isolation");
        }
        if self.egress == EgressContainment::None {
            out.push("egress-containment");
        }
        if !self.speculative_loading_switch {
            out.push("speculative-loading-switch");
        }
        if self.watchdog == WatchdogSignal::None {
            out.push("watchdog");
        }
        if !self.verified {
            out.push("verified");
        }
        out
    }

    /// Widgets may run live; otherwise they stay posters.
    pub fn runs_widgets(&self) -> bool {
        self.missing().is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unsupported_platform_reports_everything_missing() {
        let c = Capabilities::unsupported("windows");
        assert!(!c.runs_widgets());
        assert_eq!(
            c.missing(),
            vec![
                "embedding",
                "process-isolation",
                "egress-containment",
                "speculative-loading-switch",
                "watchdog",
                "verified"
            ]
        );
    }

    #[test]
    fn the_dead_proxy_counts_as_containment_but_an_unverified_host_does_not_run() {
        let mut c = Capabilities {
            platform: "linux".into(),
            embedding: Embedding::X11Child,
            process_isolation: ProcessIsolation::PerWidgetProcess,
            egress: EgressContainment::DeadProxy,
            speculative_loading_switch: true,
            watchdog: WatchdogSignal::Heartbeat,
            verified: true,
        };
        assert!(c.runs_widgets());
        c.verified = false;
        assert_eq!(c.missing(), vec!["verified"]);
        let v = serde_json::to_value(&c).unwrap();
        assert_eq!(v["embedding"], "x11-child");
        assert_eq!(v["egress"], "dead-proxy");
    }
}
