//! The toolkit-free half of the widget helper: the host page, the bridge
//! relay script, the prefilter for what the relay reports, and assembly of
//! the content the host streams in. The GTK and WebKit half is `main.rs`.

use base64::Engine;
use maleficium_widget_host::proto::{FromHelper, Theme, ToHelper};
use serde_json::{json, Value};
use std::collections::BTreeMap;

/// The isolated script world the bridge relay runs in. Widget frames and the
/// host page's own world never see its message handler.
pub const WORLD: &str = "mfwbridge";
/// The message handler name, registered in [`WORLD`] only.
pub const HANDLER: &str = "mfw";
/// The URI the view loads: the host page around the widget frame.
pub const HOST_URI: &str = "mfwhost://h/host.html";
/// Longest runtime message the relay forwards (a snapshot plus envelope).
pub const RELAY_LIMIT: usize = 8 * 1024 * 1024 + 4096;
/// Engine features that load speculatively and so reach the network outside
/// any CSP.
pub const SPECULATIVE: &[&str] = &[
    "LinkPreconnect",
    "LinkDNSPrefetch",
    "LinkPrefetch",
    "LinkPreconnectEarlyHints",
    "SpeculationRulesPrefetch",
];
/// Runtime message types the relay passes on; the host validates them again.
const ALLOWED: &[&str] = &["ready", "status", "resize", "snapshot"];

/// A widget id is a DNS label: it is the widget frame's host.
pub fn valid_id(id: &str) -> bool {
    let b = id.as_bytes();
    let edge = |c: u8| c.is_ascii_lowercase() || c.is_ascii_digit();
    !b.is_empty()
        && b.len() <= 63
        && edge(b[0])
        && edge(b[b.len() - 1])
        && b.iter().all(|&c| edge(c) || c == b'-')
}

/// The host page: no script of its own, one sandboxed frame on the widget's
/// own host, filling the view.
pub fn host_page(id: &str) -> String {
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><style>html,body{{margin:0;height:100%;overflow:hidden;background:#fff}}\
iframe{{position:absolute;left:0;top:0;border:0;width:100%;height:100%}}</style></head>\
<body><iframe id=\"w\" sandbox=\"allow-scripts\" src=\"mfw://{id}/index.html\"></iframe></body></html>"
    )
}

/// The host page's policy: no script, frames from the widget's host only.
pub fn host_policy(id: &str) -> String {
    format!("default-src 'none'; script-src 'none'; style-src 'unsafe-inline'; frame-src mfw://{id}; form-action 'none'; base-uri 'none'")
}

/// The relay: beats every 200 ms from the page's main thread (which the
/// widget frame shares, so a frozen widget stops it), forwards the widget
/// frame's messages, and posts host messages into the frame.
pub fn world_script() -> String {
    format!(
        r#"(() => {{
const H = window.webkit.messageHandlers.{HANDLER};
const send = (o) => H.postMessage(JSON.stringify(o));
const beat = () => send({{t: 'hb'}});
setInterval(beat, 200); beat();
const frame = () => document.getElementById('w');
window.addEventListener('message', (e) => {{
  const f = frame();
  if (!f || e.source !== f.contentWindow) return;
  let s;
  try {{ s = JSON.stringify(e.data); }} catch (_) {{ send({{t: 'drop', why: 'unserializable', kind: ''}}); return; }}
  if (typeof s !== 'string' || s.length > {RELAY_LIMIT}) {{ send({{t: 'drop', why: 'oversize', kind: ''}}); return; }}
  send({{t: 'bridge', origin: e.origin, m: JSON.parse(s)}});
}});
window.__mfwPost = (m, transfer) => {{ const f = frame(); if (f && f.contentWindow) f.contentWindow.postMessage(m, '*', transfer || []); }};
window.__mfwInit = (init, sources) => {{
  const out = {{}}, tr = [];
  for (const k of Object.keys(sources)) {{
    const s = sources[k], bin = atob(s.b64), u = new Uint8Array(bin.length);
    for (let i = 0; i < bin.length; i++) u[i] = bin.charCodeAt(i);
    out[k] = {{name: s.name, mime: s.mime, sha256: s.sha256, bytes: u.buffer}};
    tr.push(u.buffer);
  }}
  init.sources = out;
  window.__mfwPost(init, tr);
}};
window.__mfwFocus = () => {{ const f = frame(); if (f) f.focus(); }};
}})();"#
    )
}

/// What the relay reported, after the helper's prefilter.
pub fn relay(raw: &str) -> Option<FromHelper> {
    let v: Value = serde_json::from_str(raw).ok()?;
    match v.get("t")?.as_str()? {
        "hb" => Some(FromHelper::Hb),
        "drop" => Some(FromHelper::Drop {
            why: v.get("why")?.as_str()?.chars().take(40).collect(),
            kind: String::new(),
        }),
        "bridge" => {
            let m = v.get("m")?;
            let kind = m.get("type").and_then(Value::as_str).unwrap_or("");
            let ok = v.get("origin").and_then(Value::as_str) == Some("null")
                && m.get("mfw").and_then(Value::as_u64) == Some(1)
                && ALLOWED.contains(&kind);
            Some(if ok {
                FromHelper::Bridge { m: m.clone() }
            } else {
                FromHelper::Drop {
                    why: "prefilter".into(),
                    kind: kind.chars().take(40).collect(),
                }
            })
        }
        _ => None,
    }
}

/// One source as the host streamed it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Source {
    pub name: String,
    pub mime: String,
    pub sha256: String,
    pub bytes: Vec<u8>,
}

/// The widget's document and sources, gathered frame by frame until `Load`.
#[derive(Debug, Default)]
pub struct Content {
    pub document: String,
    pub sources: BTreeMap<String, Source>,
    pub loaded: bool,
}

impl Content {
    /// Takes one content frame; anything else is refused.
    pub fn take(&mut self, msg: &ToHelper) -> Result<(), String> {
        match msg {
            ToHelper::Document { chunk, .. } => {
                self.document.push_str(chunk);
                Ok(())
            }
            ToHelper::Source {
                role,
                name,
                mime,
                sha256,
                chunk_b64,
                ..
            } => {
                let bytes = base64::engine::general_purpose::STANDARD
                    .decode(chunk_b64)
                    .map_err(|e| format!("source {role}: {e}"))?;
                let s = self.sources.entry(role.clone()).or_default();
                s.name.clone_from(name);
                s.mime.clone_from(mime);
                s.sha256.clone_from(sha256);
                s.bytes.extend_from_slice(&bytes);
                Ok(())
            }
            ToHelper::Load => {
                self.loaded = true;
                Ok(())
            }
            other => Err(format!("unexpected {other:?} before load")),
        }
    }
}

/// The script that posts bridge `init` into the widget frame, its sources
/// as transferred bytes.
pub fn init_script(
    widget_id: &str,
    runtime: &str,
    alt: &str,
    options: &Value,
    theme: &Theme,
    sources: &BTreeMap<String, Source>,
) -> String {
    let init = json!({"mfw": 1, "type": "init", "protocol": 1, "widgetId": widget_id,
        "runtime": runtime, "alt": alt, "options": options, "theme": theme});
    let srcs: serde_json::Map<String, Value> = sources
        .iter()
        .map(|(role, s)| {
            (
                role.clone(),
                json!({"name": s.name, "mime": s.mime, "sha256": s.sha256,
                    "b64": base64::engine::general_purpose::STANDARD.encode(&s.bytes)}),
            )
        })
        .collect();
    format!("window.__mfwInit({init}, {});", Value::Object(srcs))
}

/// The script that posts a host message into the widget frame.
pub fn post_script(m: &Value) -> String {
    format!("window.__mfwPost({m});")
}

pub fn theme_message(t: &Theme) -> Value {
    json!({"mfw": 1, "type": "theme", "mode": t.mode, "tokens": t.tokens})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_prefilter_passes_only_allowlisted_types_from_an_opaque_origin() {
        let ok = r#"{"t":"bridge","origin":"null","m":{"mfw":1,"type":"status","state":"loaded"}}"#;
        assert!(matches!(relay(ok), Some(FromHelper::Bridge { .. })));
        for bad in [
            r#"{"t":"bridge","origin":"mfw://a","m":{"mfw":1,"type":"status"}}"#,
            r#"{"t":"bridge","origin":"null","m":{"mfw":1,"type":"invoke"}}"#,
            r#"{"t":"bridge","origin":"null","m":{"type":"status"}}"#,
        ] {
            assert!(matches!(relay(bad), Some(FromHelper::Drop { .. })), "{bad}");
        }
        assert_eq!(relay(r#"{"t":"hb"}"#), Some(FromHelper::Hb));
        assert_eq!(relay("not json"), None);
        assert_eq!(relay(r#"{"t":"other"}"#), None);
    }

    #[test]
    fn content_reassembles_chunked_sources_and_documents() {
        let src = maleficium_widget_host::SourceBytes {
            role: "model".into(),
            name: "m.glb".into(),
            mime: "model/gltf-binary".into(),
            sha256: "ab".into(),
            bytes: (0..2_000_000u32).map(|i| (i % 251) as u8).collect(),
        };
        let doc = "<p>".repeat(300_000);
        let mut c = Content::default();
        for f in maleficium_widget_host::proto::content_frames(&doc, std::slice::from_ref(&src)) {
            c.take(&f).unwrap();
        }
        assert!(c.loaded);
        assert_eq!(c.document, doc);
        assert_eq!(c.sources["model"].bytes, src.bytes);
        assert_eq!(c.sources["model"].mime, "model/gltf-binary");
        assert!(c.take(&ToHelper::Activate { on: true }).is_err());
    }

    #[test]
    fn the_host_page_frames_only_the_widget_host_with_no_script() {
        let p = host_page("fig-a");
        assert!(p.contains(r#"sandbox="allow-scripts" src="mfw://fig-a/index.html""#));
        assert!(!p.contains("allow-same-origin"));
        assert!(!p.contains("<script"));
        assert!(host_policy("fig-a").contains("script-src 'none'"));
        assert!(host_policy("fig-a").contains("frame-src mfw://fig-a;"));
    }

    #[test]
    fn ids_are_dns_labels() {
        assert!(valid_id("fig-model"));
        assert!(!valid_id("Fig"));
        assert!(!valid_id("a/b"));
        assert!(!valid_id("-a"));
        assert!(!valid_id(""));
    }

    #[test]
    fn init_carries_sources_as_base64_for_the_relay_to_transfer() {
        let mut sources = BTreeMap::new();
        sources.insert(
            "data".to_string(),
            Source {
                name: "d.csv".into(),
                mime: "text/csv".into(),
                sha256: "00".into(),
                bytes: b"a,b".to_vec(),
            },
        );
        let s = init_script(
            "t",
            "table@1",
            "alt",
            &json!({}),
            &Theme::default(),
            &sources,
        );
        assert!(s.starts_with("window.__mfwInit({"));
        assert!(s.contains(r#""protocol":1"#));
        assert!(s.contains(r#""b64":"YSxi""#));
    }
}
