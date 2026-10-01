//! Helper protocol 1: the messages between a widget host and one widget
//! helper, and their framing. Each frame is a big-endian `u32` length then
//! that many bytes of UTF-8 JSON; a frame over [`MAX_FRAME`] is a protocol
//! violation and ends the helper.

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const PROTOCOL: u32 = 1;
/// Largest frame either side accepts: an 8 MiB snapshot plus its envelope.
pub const MAX_FRAME: usize = 9 * 1024 * 1024;
/// Bytes of document or source carried by one frame.
pub const CHUNK: usize = 512 * 1024;

/// Defences a launch applies. Every field is on in production; a conformance
/// red control turns one off to prove the probe can fail.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Hardening {
    /// Send the widget's CSP header on every widget response.
    pub csp_header: bool,
    /// Turn off the engine's speculative loading (preconnect, prefetch).
    pub speculative_off: bool,
    /// Route the engine's network through a dead proxy.
    pub dead_proxy: bool,
    /// Ask the backend for its strongest egress containment (a network
    /// namespace where the platform has one).
    pub contain_egress: bool,
}

impl Default for Hardening {
    fn default() -> Self {
        Self {
            csp_header: true,
            speculative_off: true,
            dead_proxy: true,
            contain_egress: true,
        }
    }
}

/// Theme tokens for a widget: the colour mode and `--m-*` custom properties.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Theme {
    pub mode: ThemeMode,
    pub tokens: std::collections::BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ThemeMode {
    #[default]
    Light,
    Dark,
}

/// An axis-aligned rectangle in device pixels of the host window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub fn is_empty(&self) -> bool {
        self.w <= 0 || self.h <= 0
    }

    pub fn intersect(&self, o: &Rect) -> Rect {
        let x0 = self.x.max(o.x);
        let y0 = self.y.max(o.y);
        let x1 = (self.x + self.w).min(o.x + o.w);
        let y1 = (self.y + self.h).min(o.y + o.h);
        Rect {
            x: x0,
            y: y0,
            w: (x1 - x0).max(0),
            h: (y1 - y0).max(0),
        }
    }
}

/// Where a widget sits: its full slot and the visible part of it, both in
/// host-window device pixels. A hidden placement keeps the process but
/// shows nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Placement {
    pub slot: Rect,
    pub clip: Rect,
    pub visible: bool,
}

impl Placement {
    pub fn hidden() -> Self {
        Self::default()
    }

    /// The slot clipped to `pane`; visible only when something remains.
    pub fn clipped(slot: Rect, pane: Rect) -> Self {
        let clip = slot.intersect(&pane);
        Self {
            slot,
            clip,
            visible: !clip.is_empty(),
        }
    }
}

/// Host to helper.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "kebab-case")]
pub enum ToHelper {
    /// First frame: who the helper is and where it draws.
    #[serde(rename_all = "camelCase")]
    Config {
        proto: u32,
        widget_id: String,
        /// The host window the helper embeds into (an X11 window id on X11).
        parent: u64,
        width: i32,
        height: i32,
        /// The CSP header for widget responses.
        csp: String,
        hardening: Hardening,
    },
    /// A piece of the widget document; the last piece has `last`.
    Document {
        chunk: String,
        last: bool,
    },
    /// A piece of one runtime source, base64; the last piece has `last`.
    #[serde(rename_all = "camelCase")]
    Source {
        role: String,
        name: String,
        mime: String,
        sha256: String,
        chunk_b64: String,
        last: bool,
    },
    /// Everything is sent: load the widget.
    Load,
    /// Bridge `init` for the runtime, sent after it posts `ready`. The
    /// helper adds the sources it holds as transferred bytes.
    Init {
        runtime: String,
        alt: String,
        options: Value,
        theme: Theme,
    },
    Theme(Theme),
    #[serde(rename_all = "camelCase")]
    Place {
        seq: u64,
        placement: Placement,
    },
    /// Take or release input: pointer, wheel and keys reach the widget only
    /// while active.
    Activate {
        on: bool,
    },
    #[serde(rename_all = "camelCase")]
    SnapshotRequest {
        request_id: String,
    },
    Shutdown,
}

/// Why a helper's input state changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ActiveWhy {
    Host,
    Escape,
    Blur,
}

/// Helper to host.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "kebab-case")]
pub enum FromHelper {
    /// The view exists. `features_off` names the speculative-loading
    /// features the engine turned off.
    #[serde(rename_all = "camelCase")]
    Ready {
        pid: u32,
        window: u64,
        features_off: Vec<String>,
        sandbox: bool,
        dead_proxy: bool,
    },
    #[serde(rename_all = "camelCase")]
    Loaded {
        took_ms: u64,
    },
    /// The widget page's main thread is alive.
    Hb,
    /// A runtime message that passed the helper's prefilter. The host
    /// validates it again.
    Bridge {
        m: Value,
    },
    /// A runtime message the helper dropped.
    Drop {
        why: String,
        kind: String,
    },
    Active {
        on: bool,
        why: ActiveWhy,
    },
    Placed {
        seq: u64,
    },
    Log {
        m: String,
    },
}

pub fn encode<T: Serialize>(msg: &T) -> Vec<u8> {
    let body = serde_json::to_vec(msg).expect("protocol messages serialize");
    let mut out = Vec::with_capacity(4 + body.len());
    out.extend_from_slice(&(body.len() as u32).to_be_bytes());
    out.extend_from_slice(&body);
    out
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrameError {
    /// A length prefix over [`MAX_FRAME`]: the stream cannot be trusted.
    Oversize(usize),
}

/// Splits a byte stream into frame bodies.
#[derive(Debug, Default)]
pub struct Decoder {
    buf: Vec<u8>,
}

impl Decoder {
    pub fn push(&mut self, bytes: &[u8]) {
        self.buf.extend_from_slice(bytes);
    }

    /// The next complete frame body, `Ok(None)` until one is buffered.
    pub fn next_frame(&mut self) -> Result<Option<Vec<u8>>, FrameError> {
        if self.buf.len() < 4 {
            return Ok(None);
        }
        let n = u32::from_be_bytes([self.buf[0], self.buf[1], self.buf[2], self.buf[3]]) as usize;
        if n > MAX_FRAME {
            return Err(FrameError::Oversize(n));
        }
        if self.buf.len() < 4 + n {
            return Ok(None);
        }
        let body = self.buf[4..4 + n].to_vec();
        self.buf.drain(..4 + n);
        Ok(Some(body))
    }
}

/// The frames that carry one widget's document and sources, in order,
/// ending with `Load`.
pub fn content_frames(document: &str, sources: &[crate::SourceBytes]) -> Vec<ToHelper> {
    use base64::Engine;
    let mut out = Vec::new();
    let mut rest = document;
    loop {
        let mut cut = rest.len().min(CHUNK);
        while !rest.is_char_boundary(cut) {
            cut -= 1;
        }
        let (head, tail) = rest.split_at(cut);
        out.push(ToHelper::Document {
            chunk: head.to_string(),
            last: tail.is_empty(),
        });
        if tail.is_empty() {
            break;
        }
        rest = tail;
    }
    for s in sources {
        let mut chunks = s.bytes.chunks(CHUNK).peekable();
        if chunks.peek().is_none() {
            out.push(ToHelper::Source {
                role: s.role.clone(),
                name: s.name.clone(),
                mime: s.mime.clone(),
                sha256: s.sha256.clone(),
                chunk_b64: String::new(),
                last: true,
            });
        }
        while let Some(c) = chunks.next() {
            out.push(ToHelper::Source {
                role: s.role.clone(),
                name: s.name.clone(),
                mime: s.mime.clone(),
                sha256: s.sha256.clone(),
                chunk_b64: base64::engine::general_purpose::STANDARD.encode(c),
                last: chunks.peek().is_none(),
            });
        }
    }
    out.push(ToHelper::Load);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn frames_round_trip_through_the_decoder_in_pieces() {
        let a = encode(&ToHelper::Activate { on: true });
        let b = encode(&FromHelper::Hb);
        let mut all = a.clone();
        all.extend_from_slice(&b);
        let mut d = Decoder::default();
        for byte in &all {
            d.push(std::slice::from_ref(byte));
        }
        let f1: ToHelper = serde_json::from_slice(&d.next_frame().unwrap().unwrap()).unwrap();
        let f2: FromHelper = serde_json::from_slice(&d.next_frame().unwrap().unwrap()).unwrap();
        assert_eq!(f1, ToHelper::Activate { on: true });
        assert_eq!(f2, FromHelper::Hb);
        assert_eq!(d.next_frame(), Ok(None));
    }

    #[test]
    fn an_oversize_length_is_a_violation() {
        let mut d = Decoder::default();
        d.push(&((MAX_FRAME + 1) as u32).to_be_bytes());
        assert_eq!(d.next_frame(), Err(FrameError::Oversize(MAX_FRAME + 1)));
    }

    #[test]
    fn wire_names_are_stable() {
        let v = serde_json::to_value(ToHelper::Place {
            seq: 3,
            placement: Placement::clipped(
                Rect {
                    x: 0,
                    y: -10,
                    w: 100,
                    h: 50,
                },
                Rect {
                    x: 0,
                    y: 0,
                    w: 800,
                    h: 600,
                },
            ),
        })
        .unwrap();
        assert_eq!(
            v,
            json!({"t": "place", "seq": 3, "placement": {
                "slot": {"x": 0, "y": -10, "w": 100, "h": 50},
                "clip": {"x": 0, "y": 0, "w": 100, "h": 40},
                "visible": true}})
        );
        let v = serde_json::to_value(ToHelper::SnapshotRequest {
            request_id: "r".into(),
        })
        .unwrap();
        assert_eq!(v, json!({"t": "snapshot-request", "requestId": "r"}));
        let r: FromHelper =
            serde_json::from_value(json!({"t": "active", "on": false, "why": "escape"})).unwrap();
        assert_eq!(
            r,
            FromHelper::Active {
                on: false,
                why: ActiveWhy::Escape
            }
        );
    }

    #[test]
    fn an_off_screen_slot_is_not_visible() {
        let p = Placement::clipped(
            Rect {
                x: 0,
                y: 700,
                w: 100,
                h: 50,
            },
            Rect {
                x: 0,
                y: 0,
                w: 800,
                h: 600,
            },
        );
        assert!(!p.visible);
        assert!(p.clip.is_empty());
    }

    #[test]
    fn content_is_chunked_on_char_boundaries_and_ends_with_load() {
        let doc = "é".repeat(CHUNK); // 2 bytes each: spans several chunks
        let src = crate::SourceBytes {
            role: "data".into(),
            name: "a.csv".into(),
            mime: "text/csv".into(),
            sha256: "00".into(),
            bytes: vec![7u8; CHUNK + 1],
        };
        let frames = content_frames(&doc, std::slice::from_ref(&src));
        let mut text = String::new();
        let mut srcs = 0;
        for f in &frames {
            match f {
                ToHelper::Document { chunk, .. } => text.push_str(chunk),
                ToHelper::Source { .. } => srcs += 1,
                _ => {}
            }
        }
        assert_eq!(text, doc);
        assert_eq!(srcs, 2);
        assert_eq!(frames.last(), Some(&ToHelper::Load));
        assert!(frames.iter().all(|f| encode(f).len() <= MAX_FRAME));
    }
}
