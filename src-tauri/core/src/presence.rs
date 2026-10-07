//! Which app windows are open and what they hold, for the MCP server.
//!
//! The app and the MCP server are separate processes that share only the
//! filesystem, so each running app publishes one small file under the
//! app-data dir: `maleficium-instances/<pid>.json`, its open project folder,
//! the document's main file and the file in front. The app rewrites it when
//! any of those change and on a heartbeat, and removes it on exit; a reader
//! drops a file whose heartbeat stopped (a crash), so no liveness probe is
//! needed on any OS. Read-only for the agent: it can see the project the
//! user is in and grant it, never open or change anything in the app.

use crate::Core;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use ts_rs::TS;

/// The folder of presence files, under the app-data dir.
pub const DIR: &str = "maleficium-instances";
/// How often a running app rewrites its file.
pub const HEARTBEAT_MS: u64 = 30_000;
/// A file older than this belongs to an app that stopped without cleaning up.
pub const STALE_MS: u64 = 3 * HEARTBEAT_MS;

/// One running app: what it has open.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct Presence {
    pub pid: u32,
    /// The open project's folder (absolute); none when no folder is open.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub project: Option<String>,
    /// The document's main file, relative to the project.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub main_rel: Option<String>,
    /// The file in the editor, relative to the project.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub active_rel: Option<String>,
    /// Unix ms the app started and last wrote this.
    pub started_ms: u64,
    pub updated_ms: u64,
}

/// This process's published presence, rewritten on each heartbeat.
static CURRENT: Mutex<Option<Presence>> = Mutex::new(None);

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// The presence folder.
pub fn dir() -> PathBuf {
    crate::data_base_dir().join(DIR)
}

/// Publish what this app has open: the project by its session root id (none
/// when no folder is open), and the main and front files relative to it.
pub fn set(
    cx: &Core,
    root_id: Option<&str>,
    main_rel: Option<String>,
    active_rel: Option<String>,
) -> Result<(), String> {
    set_at(&dir(), cx, root_id, main_rel, active_rel)
}

pub(crate) fn set_at(
    base: &Path,
    cx: &Core,
    root_id: Option<&str>,
    main_rel: Option<String>,
    active_rel: Option<String>,
) -> Result<(), String> {
    let project = match root_id {
        Some(id) => Some(
            crate::fs::session_root(cx, id)?
                .to_string_lossy()
                .to_string(),
        ),
        None => None,
    };
    let (main_rel, active_rel) = match project {
        Some(_) => (main_rel, active_rel),
        None => (None, None),
    };
    let mut cur = CURRENT.lock().map_err(|_| "presence lock poisoned")?;
    let now = now_ms();
    let p = Presence {
        pid: std::process::id(),
        project,
        main_rel,
        active_rel,
        started_ms: cur.as_ref().map_or(now, |c| c.started_ms),
        updated_ms: now,
    };
    write(base, &p)?;
    *cur = Some(p);
    Ok(())
}

/// Rewrite this app's file with a fresh heartbeat (nothing before `set`).
pub fn heartbeat() {
    heartbeat_at(&dir());
}

pub(crate) fn heartbeat_at(base: &Path) {
    let Ok(mut cur) = CURRENT.lock() else { return };
    if let Some(p) = cur.as_mut() {
        p.updated_ms = now_ms();
        let _ = write(base, p);
    }
}

/// Remove this app's file (on exit).
pub fn withdraw() {
    withdraw_at(&dir());
}

pub(crate) fn withdraw_at(base: &Path) {
    if let Ok(mut cur) = CURRENT.lock() {
        *cur = None;
    }
    let _ = std::fs::remove_file(file_of(base, std::process::id()));
}

fn file_of(base: &Path, pid: u32) -> PathBuf {
    base.join(format!("{pid}.json"))
}

/// Written whole beside the target, then renamed over it, so a reader never
/// sees half a file.
fn write(base: &Path, p: &Presence) -> Result<(), String> {
    std::fs::create_dir_all(base).map_err(|e| format!("presence dir: {e}"))?;
    let tmp = base.join(format!(".{}.json.tmp", p.pid));
    let text = serde_json::to_vec(p).map_err(|e| e.to_string())?;
    std::fs::write(&tmp, text).map_err(|e| format!("presence write: {e}"))?;
    std::fs::rename(&tmp, file_of(base, p.pid)).map_err(|e| format!("presence write: {e}"))
}

/// Every running app, most recently active first. Files whose heartbeat
/// stopped are dropped (and removed); unreadable ones are skipped.
pub fn list() -> Vec<Presence> {
    list_at(&dir(), now_ms())
}

pub(crate) fn list_at(base: &Path, now: u64) -> Vec<Presence> {
    let Ok(entries) = std::fs::read_dir(base) else {
        return Vec::new();
    };
    let mut out: Vec<Presence> = Vec::new();
    for e in entries.flatten() {
        let path = e.path();
        let named = path
            .file_stem()
            .and_then(|s| s.to_str())
            .is_some_and(|s| s.parse::<u32>().is_ok())
            && path.extension().is_some_and(|x| x == "json");
        if !named {
            continue;
        }
        let Some(p) = std::fs::read(&path)
            .ok()
            .and_then(|b| serde_json::from_slice::<Presence>(&b).ok())
        else {
            continue;
        };
        if now.saturating_sub(p.updated_ms) > STALE_MS {
            let _ = std::fs::remove_file(&path);
            continue;
        }
        out.push(p);
    }
    out.sort_by(|a, b| b.updated_ms.cmp(&a.updated_ms).then(a.pid.cmp(&b.pid)));
    out
}

/// The root id this process granted for `project`, if any.
pub fn granted_as(cx: &Core, project: &str) -> Option<String> {
    crate::fs::granted_as(cx, project)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = crate::test_scratch::dir(name);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// One test drives the process-global state end to end, so parallel
    /// tests never race over it.
    #[test]
    fn an_app_publishes_what_it_holds_and_readers_drop_stopped_ones() {
        let cx = &Core::default();
        let base = scratch("presence-base");
        let proj = scratch("presence-project");
        let root = dunce::canonicalize(&proj).unwrap();
        crate::grant_root(cx, "pres", &root.to_string_lossy()).unwrap();

        // Nothing published yet: a heartbeat writes nothing.
        heartbeat_at(&base);
        assert!(list_at(&base, now_ms()).is_empty());

        set_at(&base, cx, None, Some("x.tex".into()), None).unwrap();
        let none = list_at(&base, now_ms());
        assert_eq!(none.len(), 1);
        assert_eq!(none[0].pid, std::process::id());
        assert!(none[0].project.is_none() && none[0].main_rel.is_none());

        set_at(
            &base,
            cx,
            Some("pres"),
            Some("main.tex".into()),
            Some("ch/intro.tex".into()),
        )
        .unwrap();
        let open = list_at(&base, now_ms());
        assert_eq!(open.len(), 1);
        assert_eq!(open[0].project.as_deref(), Some(&*root.to_string_lossy()));
        assert_eq!(open[0].main_rel.as_deref(), Some("main.tex"));
        assert_eq!(open[0].active_rel.as_deref(), Some("ch/intro.tex"));
        assert!(set_at(&base, cx, Some("nope"), None, None).is_err());
        assert_eq!(
            granted_as(cx, &root.to_string_lossy()).as_deref(),
            Some("pres")
        );
        assert!(granted_as(cx, &base.to_string_lossy()).is_none());

        // Another app that crashed long ago, one alive, and junk.
        let gone = Presence {
            pid: 1,
            project: Some("/x".into()),
            main_rel: None,
            active_rel: None,
            started_ms: 0,
            updated_ms: now_ms() - STALE_MS - 1,
        };
        write(&base, &gone).unwrap();
        let other = Presence {
            pid: 2,
            updated_ms: now_ms() - 1_000,
            ..gone.clone()
        };
        write(&base, &other).unwrap();
        std::fs::write(base.join("3.json"), b"not json").unwrap();
        std::fs::write(base.join("notes.txt"), b"x").unwrap();
        let all = list_at(&base, now_ms());
        assert_eq!(
            all.iter().map(|p| p.pid).collect::<Vec<_>>(),
            vec![std::process::id(), 2],
            "most recent first; the stopped one is dropped"
        );
        assert!(
            !base.join("1.json").exists(),
            "a stopped app's file is removed"
        );

        withdraw_at(&base);
        assert_eq!(
            list_at(&base, now_ms())
                .iter()
                .map(|p| p.pid)
                .collect::<Vec<_>>(),
            vec![2]
        );
        heartbeat_at(&base);
        assert!(
            !file_of(&base, std::process::id()).exists(),
            "no heartbeat after exit"
        );
    }
}
