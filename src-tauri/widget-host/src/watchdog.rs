//! The freeze watchdog for one live widget. The widget page beats every
//! 200 ms from its main thread, so a frozen widget stops beating: after
//! `badge_ms` without a beat it is marked unresponsive, after `kill_ms` it
//! is killed. A widget that never beats is killed `start_ms` after launch.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WatchdogConfig {
    pub badge_ms: u64,
    pub kill_ms: u64,
    /// Kills of one widget in a session before it stays a poster.
    pub quarantine_after: u32,
    /// Time from launch to the first beat before the widget is killed.
    pub start_ms: u64,
}

impl Default for WatchdogConfig {
    fn default() -> Self {
        Self {
            badge_ms: 2_000,
            kill_ms: 5_000,
            quarantine_after: 2,
            start_ms: 10_000,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Unresponsive { gap_ms: u64 },
    Responsive,
    Kill { gap_ms: u64 },
}

#[derive(Debug, Clone)]
pub struct Watchdog {
    cfg: WatchdogConfig,
    launched: u64,
    last_beat: Option<u64>,
    badged: bool,
}

impl Watchdog {
    pub fn new(cfg: WatchdogConfig, now: u64) -> Self {
        Self {
            cfg,
            launched: now,
            last_beat: None,
            badged: false,
        }
    }

    /// A beat arrived; clears the badge if one was shown.
    pub fn beat(&mut self, now: u64) -> Option<Verdict> {
        self.last_beat = Some(now);
        std::mem::take(&mut self.badged).then_some(Verdict::Responsive)
    }

    /// What the watchdog decides at `now`, if anything changed.
    pub fn check(&mut self, now: u64) -> Option<Verdict> {
        let Some(last) = self.last_beat else {
            let gap_ms = now.saturating_sub(self.launched);
            return (gap_ms > self.cfg.start_ms).then_some(Verdict::Kill { gap_ms });
        };
        let gap_ms = now.saturating_sub(last);
        if gap_ms > self.cfg.kill_ms {
            Some(Verdict::Kill { gap_ms })
        } else if gap_ms > self.cfg.badge_ms && !self.badged {
            self.badged = true;
            Some(Verdict::Unresponsive { gap_ms })
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn badges_at_two_seconds_and_kills_at_five() {
        let mut w = Watchdog::new(WatchdogConfig::default(), 0);
        w.beat(100);
        assert_eq!(w.check(2_000), None);
        assert_eq!(
            w.check(2_101),
            Some(Verdict::Unresponsive { gap_ms: 2_001 })
        );
        assert_eq!(w.check(3_000), None, "the badge is reported once");
        assert_eq!(w.check(5_100), None);
        assert_eq!(w.check(5_101), Some(Verdict::Kill { gap_ms: 5_001 }));
    }

    #[test]
    fn a_beat_after_the_badge_clears_it() {
        let mut w = Watchdog::new(WatchdogConfig::default(), 0);
        w.beat(0);
        assert!(matches!(w.check(2_500), Some(Verdict::Unresponsive { .. })));
        assert_eq!(w.beat(2_600), Some(Verdict::Responsive));
        assert_eq!(w.beat(2_800), None);
        assert_eq!(w.check(4_000), None);
    }

    #[test]
    fn a_widget_that_never_beats_is_killed_after_the_start_budget() {
        let mut w = Watchdog::new(WatchdogConfig::default(), 1_000);
        assert_eq!(w.check(9_000), None, "no badge before the first beat");
        assert_eq!(w.check(11_001), Some(Verdict::Kill { gap_ms: 10_001 }));
    }
}
