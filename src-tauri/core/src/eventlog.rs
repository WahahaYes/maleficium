//! The one JSONL event-log writer: every adapter and the frontend emit
//! typed bus events here. One `events.jsonl` is shared by all writers; each
//! line goes out whole with `O_APPEND`, so concurrent writers interleave
//! lines without tearing them. The line caps are enforced by rotation: the
//! newest `MAX_LOG_EVENTS` lines survive, each at most `MAX_LINE_BYTES`.
//! Rotation is best-effort under concurrency (a line appended mid-rotate
//! can be lost); nothing else is.
//!
//! The log is never truncated on start: runs accumulate, and readers scope
//! to the last `log.open` for the current launch's segment.

use std::io::Write;
use std::path::{Path, PathBuf};

use maleficium_events::{AppEvent, BusEvent, LogLine};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Newest events kept; older lines are dropped by rotation.
pub const MAX_LOG_EVENTS: usize = 2000;
/// Ceiling on one serialized line, newline included.
pub const MAX_LINE_BYTES: usize = 2048;
/// Message text is cut to this many characters before serializing.
pub const MAX_MESSAGE_CHARS: usize = 300;

/// Epoch milliseconds now.
pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// The shared log file: `<app-data>/maleficium-log/events.jsonl`, the same
/// path the frontend's `eventLogPath` names.
pub fn log_path() -> PathBuf {
    crate::data_base_dir()
        .join("maleficium-log")
        .join("events.jsonl")
}

fn action_name(event: &AppEvent) -> String {
    serde_json::to_value(event)
        .ok()
        .and_then(|v| v.get("action")?.as_str().map(str::to_string))
        .unwrap_or_default()
}

fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        s.chars().take(max).collect()
    }
}

/// One JSONL line for a bus event. A payload that would break the line
/// ceiling is dropped down to its action, so every line stays parseable.
/// Byte-identical in shape to the frontend's `serializeEvent`.
pub fn serialize(event: &BusEvent) -> String {
    let mut line = LogLine {
        at: event.at,
        scope: event.scope,
        kind: event.kind,
        actor: event.actor,
        message: truncate_chars(&event.message, MAX_MESSAGE_CHARS),
        event: Some(event.event.clone()),
        dropped: None,
    };
    let mut text = serde_json::to_string(&line).unwrap_or_default();
    if text.len() + 1 > MAX_LINE_BYTES {
        line.event = None;
        line.dropped = Some(action_name(&event.event));
        text = serde_json::to_string(&line).unwrap_or_default();
    }
    text.push('\n');
    text
}

/// What a rotation kept and dropped.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct RotateReport {
    pub path: String,
    pub kept: u64,
    pub dropped: u64,
}

/// Append events as whole-line `O_APPEND` writes, creating the log dir on
/// the way. Past the size cap, a rotation follows the append. A dropped
/// write is an error to the caller; adapters log it nowhere (the log is
/// never the reason work stops).
pub fn append(events: &[BusEvent]) -> Result<(), String> {
    append_to(&log_path(), events)
}

fn append_to(path: &Path, events: &[BusEvent]) -> Result<(), String> {
    if events.is_empty() {
        return Ok(());
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("event log unreachable: {}", e))?;
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| format!("event log unwritable: {}", e))?;
    for e in events {
        file.write_all(serialize(e).as_bytes())
            .map_err(|e| format!("event log unwritable: {}", e))?;
    }
    let cap_bytes = (MAX_LINE_BYTES * MAX_LOG_EVENTS) as u64;
    if file.metadata().map(|m| m.len()).unwrap_or(0) > cap_bytes {
        rotate_path(path)?;
    }
    Ok(())
}

/// Enforce the line cap: keep the newest `MAX_LOG_EVENTS` lines, rewriting
/// only when something is dropped. A missing file rotates to empty.
pub fn rotate() -> Result<RotateReport, String> {
    rotate_path(&log_path())
}

fn rotate_path(path: &Path) -> Result<RotateReport, String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("event log unreachable: {}", e))?;
    }
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let all: Vec<&str> = text.lines().collect();
    let dropped = all.len().saturating_sub(MAX_LOG_EVENTS) as u64;
    let kept = &all[dropped as usize..];
    if dropped > 0 {
        let mut body = kept.join("\n");
        body.push('\n');
        std::fs::write(path, body).map_err(|e| format!("event log unwritable: {}", e))?;
    }
    Ok(RotateReport {
        path: path.to_string_lossy().to_string(),
        kept: kept.len() as u64,
        dropped,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use maleficium_events::{Actor, AppEvent, EventKind, EventScope};

    fn bus(action_event: AppEvent, message: &str) -> BusEvent {
        BusEvent {
            at: 1700000000000,
            scope: EventScope::Compile,
            kind: EventKind::Success,
            actor: Actor::User,
            message: message.to_string(),
            event: action_event,
        }
    }

    #[test]
    fn serialize_matches_the_frontend_shape() {
        let e = bus(
            AppEvent::CompileFinish {
                target: "/p/main.tex".to_string(),
                ok: true,
                ms: 1200,
                pdf_url: None,
                reason: None,
            },
            "compiled /p/main.pdf",
        );
        assert_eq!(
            serialize(&e),
            "{\"at\":1700000000000,\"scope\":\"compile\",\"kind\":\"success\",\
             \"actor\":\"user\",\"message\":\"compiled /p/main.pdf\",\
             \"event\":{\"action\":\"compile.finish\",\"target\":\"/p/main.tex\",\
             \"ok\":true,\"ms\":1200}}\n"
        );
    }

    #[test]
    fn serialize_caps_message_and_payload() {
        let big = bus(
            AppEvent::FileLoad {
                path: "y".repeat(MAX_LINE_BYTES * 2),
            },
            &"x".repeat(MAX_MESSAGE_CHARS * 2),
        );
        let line = serialize(&big);
        assert!(line.len() <= MAX_LINE_BYTES, "{line}");
        let back: LogLine = serde_json::from_str(&line).unwrap();
        assert_eq!(back.message.chars().count(), MAX_MESSAGE_CHARS);
        assert!(back.event.is_none());
        assert_eq!(back.dropped.as_deref(), Some("file.load"));
    }

    #[test]
    fn log_path_matches_the_frontend_log_location() {
        // The frontend's `eventLogPath` names `<app-data>/maleficium-log/events.jsonl`.
        assert!(log_path().ends_with("io.github.wahahayes.maleficium/maleficium-log/events.jsonl"));
    }

    #[test]
    fn append_round_trips_parseable_lines() {
        let dir = crate::test_scratch::dir("eventlog-append");
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("events.jsonl");
        let evts = vec![
            bus(
                AppEvent::FileLoad {
                    path: "a.tex".to_string(),
                },
                "one",
            ),
            bus(
                AppEvent::FileLoad {
                    path: "b.tex".to_string(),
                },
                "two",
            ),
        ];
        append_to(&path, &evts).unwrap();
        append_to(&path, &evts[..1]).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<LogLine> = text
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[2].message, "one");
    }

    #[test]
    fn rotate_keeps_the_newest_and_skips_the_rewrite_when_under_cap() {
        let dir = crate::test_scratch::dir("eventlog-rotate");
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("events.jsonl");
        let body: String = (0..MAX_LOG_EVENTS + 5)
            .map(|i| format!("{{\"m\":{i}}}\n"))
            .collect();
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&path, &body).unwrap();
        let report = rotate_path(&path).unwrap();
        assert_eq!((report.kept, report.dropped), (MAX_LOG_EVENTS as u64, 5));
        let text = std::fs::read_to_string(&path).unwrap();
        assert_eq!(text.lines().count(), MAX_LOG_EVENTS);
        assert!(text.contains("{\"m\":2004}"));
        assert!(!text.contains("{\"m\":0}"));

        let again = rotate_path(&path).unwrap();
        assert_eq!((again.kept, again.dropped), (MAX_LOG_EVENTS as u64, 0));
    }

    #[test]
    fn append_rotates_past_the_size_cap() {
        let dir = crate::test_scratch::dir("eventlog-cap");
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("events.jsonl");
        std::fs::create_dir_all(&dir).unwrap();
        // Pad past the byte cap with long lines, then one real event: the
        // append rotates the file back to the line cap.
        let pad = "x".repeat(900);
        let filler: String = (0..5000)
            .map(|i| format!("{{\"m\":{i},\"pad\":\"{pad}\"}}\n"))
            .collect();
        assert!((filler.len() as u64) > (MAX_LINE_BYTES * MAX_LOG_EVENTS) as u64);
        std::fs::write(&path, &filler).unwrap();
        append_to(
            &path,
            &[bus(
                AppEvent::FileLoad {
                    path: "z.tex".to_string(),
                },
                "last",
            )],
        )
        .unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(
            text.lines().count() <= MAX_LOG_EVENTS,
            "{}",
            text.lines().count()
        );
        assert!(text.contains("\"message\":\"last\""));
    }
}
