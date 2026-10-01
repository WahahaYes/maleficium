//! Bridge protocol 1, runtime to host: the one validator every message
//! passes before the host acts on it. The spec is `vectors/bridge.json`,
//! which every host validator (this one, the reader's) is pinned against.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub const MAX_MESSAGE: usize = 200;
pub const MAX_DETAIL_JSON: usize = 2048;
pub const MAX_HEIGHT: u64 = 20_000;
pub const MAX_REQUEST_ID: usize = 64;
pub const MAX_PNG: usize = 8 * 1024 * 1024;
const PNG_PREFIX: &str = "data:image/png;base64,";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StatusState {
    Loading,
    Loaded,
    Error,
}

/// A runtime message the host accepted.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum BridgeMessage {
    Ready,
    Status {
        state: StatusState,
        #[serde(skip_serializing_if = "Option::is_none")]
        message: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        detail: Option<Value>,
    },
    Resize {
        height: u32,
    },
    #[serde(rename_all = "camelCase")]
    Snapshot {
        request_id: String,
        png: String,
    },
}

/// Why a message was refused; the strings are the spec's `reason`s.
pub type Refusal = &'static str;

fn only(m: &Map<String, Value>, allowed: &[&str]) -> Result<(), Refusal> {
    if m.keys()
        .all(|k| k == "mfw" || k == "type" || allowed.contains(&k.as_str()))
    {
        Ok(())
    } else {
        Err("fields")
    }
}

/// Accepts a runtime message or names the rule it broke.
pub fn validate(v: &Value) -> Result<BridgeMessage, Refusal> {
    let m = v.as_object().ok_or("not-object")?;
    if m.get("mfw").and_then(Value::as_u64) != Some(1) {
        return Err("no-mfw");
    }
    match m.get("type").and_then(Value::as_str) {
        Some("ready") => {
            only(m, &[])?;
            Ok(BridgeMessage::Ready)
        }
        Some("status") => {
            let state = match m.get("state").and_then(Value::as_str) {
                Some("loading") => StatusState::Loading,
                Some("loaded") => StatusState::Loaded,
                Some("error") => StatusState::Error,
                _ => return Err("state"),
            };
            let message = match m.get("message") {
                None => None,
                Some(Value::String(s)) if s.chars().count() <= MAX_MESSAGE => Some(s.clone()),
                Some(_) => return Err("message"),
            };
            let detail = match m.get("detail") {
                None => None,
                Some(d @ Value::Object(_))
                    if serde_json::to_string(d)
                        .map(|s| s.len())
                        .unwrap_or(usize::MAX)
                        <= MAX_DETAIL_JSON =>
                {
                    Some(d.clone())
                }
                Some(_) => return Err("detail"),
            };
            only(m, &["state", "message", "detail"])?;
            Ok(BridgeMessage::Status {
                state,
                message,
                detail,
            })
        }
        Some("resize") => {
            let height = m
                .get("height")
                .and_then(Value::as_u64)
                .filter(|h| (1..=MAX_HEIGHT).contains(h))
                .ok_or("height")?;
            only(m, &["height"])?;
            Ok(BridgeMessage::Resize {
                height: height as u32,
            })
        }
        Some("snapshot") => {
            let request_id = m
                .get("requestId")
                .and_then(Value::as_str)
                .filter(|r| !r.is_empty() && r.len() <= MAX_REQUEST_ID)
                .ok_or("request-id")?;
            let png = m
                .get("png")
                .and_then(Value::as_str)
                .filter(|p| p.starts_with(PNG_PREFIX) && p.len() <= MAX_PNG)
                .ok_or("png")?;
            only(m, &["requestId", "png"])?;
            Ok(BridgeMessage::Snapshot {
                request_id: request_id.to_string(),
                png: png.to_string(),
            })
        }
        _ => Err("type"),
    }
}

/// The spec's test vectors, shared with the reader's validator.
pub const VECTORS: &str = include_str!("../vectors/bridge.json");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_vector_holds() {
        let spec: Value = serde_json::from_str(VECTORS).unwrap();
        let cases = spec["cases"].as_array().unwrap();
        assert!(cases.len() >= 30);
        for c in cases {
            let name = c["name"].as_str().unwrap();
            let got = validate(&c["message"]);
            if c["accept"].as_bool().unwrap() {
                assert!(got.is_ok(), "{name}: refused {got:?}");
            } else {
                assert_eq!(got.err(), c["reason"].as_str(), "{name}");
            }
        }
    }

    #[test]
    fn limits_in_the_spec_match_the_code() {
        let spec: Value = serde_json::from_str(VECTORS).unwrap();
        let l = &spec["limits"];
        assert_eq!(l["message"], MAX_MESSAGE);
        assert_eq!(l["detailJson"], MAX_DETAIL_JSON);
        assert_eq!(l["heightMax"], MAX_HEIGHT);
        assert_eq!(l["requestId"], MAX_REQUEST_ID);
        assert_eq!(l["png"], MAX_PNG);
    }

    #[test]
    fn a_snapshot_over_the_cap_is_refused() {
        let png = format!("{PNG_PREFIX}{}", "A".repeat(MAX_PNG));
        let v = serde_json::json!({"mfw": 1, "type": "snapshot", "requestId": "r", "png": png});
        assert_eq!(validate(&v), Err("png"));
    }

    #[test]
    fn mutating_the_type_allowlist_turns_the_hostile_set_red() {
        // Red control: a validator that only checks `mfw` accepts the
        // hostile vectors, so the suite above can fail.
        let spec: Value = serde_json::from_str(VECTORS).unwrap();
        let lax = |v: &Value| v.get("mfw").and_then(Value::as_u64) == Some(1);
        let leaked = spec["cases"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|c| !c["accept"].as_bool().unwrap() && lax(&c["message"]))
            .count();
        assert!(leaked >= 20, "only {leaked} hostile vectors would leak");
    }
}
