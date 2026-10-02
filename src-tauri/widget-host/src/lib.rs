//! The widget host port. Live widgets run outside the editor: on Linux in a
//! helper process per widget. This crate is everything a shell needs that is
//! not platform code: the helper protocol ([`proto`]), the bridge validator
//! ([`bridge`]), the freeze watchdog ([`watchdog`]), the capability
//! descriptor ([`caps`]), and [`WidgetHost`], which owns the live-widget
//! policy (cap, eviction, watchdog, quarantine) over any [`WidgetBackend`].
//! Platform backends pass the shared conformance suite (`conformance`).
//!
//! It depends on no Tauri crate and no GUI toolkit.

pub mod bridge;
pub mod caps;
pub mod fake;
mod host;
pub mod proto;
pub mod watchdog;

#[cfg(any(test, feature = "conformance"))]
pub mod conformance;

#[cfg(target_os = "linux")]
pub mod linux;

pub use caps::Capabilities;

/// The backend of a platform without a widget host: every capability is
/// reported missing and nothing ever runs.
pub struct Unavailable(pub &'static str);

impl WidgetBackend for Unavailable {
    fn capabilities(&self) -> Capabilities {
        Capabilities::unsupported(self.0)
    }
    fn launch(&mut self, _: &WidgetSpec, _: proto::Placement) -> Result<Option<u32>, String> {
        Err(format!("no widget host on {}", self.0))
    }
    fn send(&mut self, _: &str, _: &proto::ToHelper) -> Result<(), String> {
        Err(format!("no widget host on {}", self.0))
    }
    fn stop(&mut self, _: &str) {}
    fn disconnect(&mut self, _: &str) {}
    fn residue(&self, _: &str) -> usize {
        0
    }
    fn memory_kib(&self, _: &str) -> Option<u64> {
        None
    }
    fn poll(&mut self, wait: std::time::Duration) -> Vec<BackendEvent> {
        std::thread::sleep(wait);
        Vec::new()
    }
}
pub use host::*;

/// One runtime input, delivered to the widget as bytes in `init`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceBytes {
    pub role: String,
    pub name: String,
    pub mime: String,
    pub sha256: String,
    pub bytes: Vec<u8>,
}

/// Milliseconds on a monotonic clock. Backends and the host share one.
pub trait Clock: Send + Sync {
    fn now_ms(&self) -> u64;
}

/// Wall-clock time since the clock was made.
pub struct SystemClock(std::time::Instant);

impl Default for SystemClock {
    fn default() -> Self {
        Self(std::time::Instant::now())
    }
}

impl Clock for SystemClock {
    fn now_ms(&self) -> u64 {
        self.0.elapsed().as_millis() as u64
    }
}

#[cfg(test)]
mod deps;
