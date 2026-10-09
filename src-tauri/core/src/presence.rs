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

// -- open requests ----------------------------------------------------------
// An agent may ask a window to open a project (and a file in it). The ask is
// a file beside the presence files, `requests/<pid>.json`; the window shows
// it to the user, who opens it or dismisses it, and the answer is written
// back into the same file. Nothing opens without the user's click, and a
// window holds one ask at a time.

/// How long an unanswered ask waits in the window, and how long an answer
/// stays readable, before readers drop it.
pub const REQUEST_STALE_MS: u64 = 10 * 60 * 1000;

/// The user's answer to an open request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum OpenOutcome {
    Opened,
    Dismissed,
}

/// An agent's ask that a window open a project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct OpenRequest {
    pub id: String,
    /// The window asked.
    pub pid: u32,
    /// The project folder (absolute, canonical).
    pub project: String,
    /// A file to bring to the front, relative to the project.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub file: Option<String>,
    pub created_ms: u64,
    /// The user's answer; absent while it waits.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub outcome: Option<OpenOutcome>,
}

fn requests_dir(base: &Path) -> PathBuf {
    base.join("requests")
}

fn request_file(base: &Path, pid: u32) -> PathBuf {
    requests_dir(base).join(format!("{pid}.json"))
}

fn read_request(base: &Path, pid: u32, now: u64) -> Option<OpenRequest> {
    let path = request_file(base, pid);
    let r: OpenRequest = serde_json::from_slice(&std::fs::read(&path).ok()?).ok()?;
    if now.saturating_sub(r.created_ms) > REQUEST_STALE_MS {
        let _ = std::fs::remove_file(&path);
        return None;
    }
    Some(r)
}

fn write_request(base: &Path, r: &OpenRequest) -> Result<(), String> {
    let dir = requests_dir(base);
    std::fs::create_dir_all(&dir).map_err(|e| format!("request dir: {e}"))?;
    let tmp = dir.join(format!(".{}.json.tmp", r.pid));
    std::fs::write(&tmp, serde_json::to_vec(r).map_err(|e| e.to_string())?)
        .map_err(|e| format!("request write: {e}"))?;
    std::fs::rename(&tmp, request_file(base, r.pid)).map_err(|e| format!("request write: {e}"))
}

/// Ask a window to open `project` (an absolute folder) and optionally
/// `file` in it. `pid` picks the window; without it there must be exactly
/// one. Refused when the window already holds an unanswered ask.
pub fn request_open(
    pid: Option<u32>,
    project: &str,
    file: Option<&str>,
) -> Result<OpenRequest, String> {
    request_open_at(
        &dir(),
        &crate::data_base_dir(),
        now_ms(),
        pid,
        project,
        file,
    )
}

pub(crate) fn request_open_at(
    base: &Path,
    app_data: &Path,
    now: u64,
    pid: Option<u32>,
    project: &str,
    file: Option<&str>,
) -> Result<OpenRequest, String> {
    let windows = list_at(base, now);
    let target = match pid {
        Some(p) => windows
            .iter()
            .find(|w| w.pid == p)
            .ok_or_else(|| format!("no open Maleficium window has pid {p}; app_windows lists them"))?,
        None => match windows.as_slice() {
            [] => return Err("no Maleficium window is open: ask the user to start the app".into()),
            [one] => one,
            _ => {
                return Err(
                    "several Maleficium windows are open: pass the pid of the one to ask (app_windows lists them)".into(),
                )
            }
        },
    };
    if !Path::new(project).is_absolute() {
        return Err(format!("`{project}` is not an absolute folder path"));
    }
    let canon = dunce::canonicalize(project).map_err(|_| format!("`{project}` does not exist"))?;
    if !canon.is_dir() {
        return Err(format!("`{project}` is not a folder"));
    }
    if let Ok(data) = dunce::canonicalize(app_data) {
        if canon.starts_with(&data) || data.starts_with(&canon) {
            return Err(format!(
                "`{project}` holds Maleficium's own data, not a project"
            ));
        }
    }
    let file = match file.map(str::trim).filter(|f| !f.is_empty()) {
        None => None,
        Some(f) => {
            let p = Path::new(f);
            if p.is_absolute() {
                return Err(format!("file `{f}` must be relative to the project"));
            }
            let abs = dunce::canonicalize(canon.join(p))
                .map_err(|_| format!("file `{f}` does not exist in the project"))?;
            if !abs.starts_with(&canon) || !abs.is_file() {
                return Err(format!("file `{f}` is not a file in the project"));
            }
            Some(
                abs.strip_prefix(&canon)
                    .unwrap_or(p)
                    .to_string_lossy()
                    .replace('\\', "/"),
            )
        }
    };
    if read_request(base, target.pid, now).is_some_and(|r| r.outcome.is_none()) {
        return Err(
            "that window already shows an open request the user has not answered; wait for their answer".into(),
        );
    }
    let r = OpenRequest {
        id: format!("{:x}{:04x}", now, std::process::id() & 0xffff),
        pid: target.pid,
        project: canon.to_string_lossy().to_string(),
        file,
        created_ms: now,
        outcome: None,
    };
    write_request(base, &r)?;
    Ok(r)
}

/// The ask `id` sent to window `pid`, as it stands now.
pub fn request_state(pid: u32, id: &str) -> Option<OpenRequest> {
    read_request(&dir(), pid, now_ms()).filter(|r| r.id == id)
}

/// The ask this window holds, for the app to show (answered ones excluded).
pub fn pending() -> Option<OpenRequest> {
    pending_at(&dir(), now_ms())
}

pub(crate) fn pending_at(base: &Path, now: u64) -> Option<OpenRequest> {
    read_request(base, std::process::id(), now).filter(|r| r.outcome.is_none())
}

/// The user's answer to this window's ask `id`.
pub fn answer(id: &str, outcome: OpenOutcome) -> Result<(), String> {
    answer_at(&dir(), now_ms(), id, outcome)
}

pub(crate) fn answer_at(
    base: &Path,
    now: u64,
    id: &str,
    outcome: OpenOutcome,
) -> Result<(), String> {
    let mut r = pending_at(base, now)
        .filter(|r| r.id == id)
        .ok_or_else(|| format!("no open request {id} waits in this window"))?;
    r.outcome = Some(outcome);
    write_request(base, &r)
}

/// Every window's current ask, by pid (for the agent's view).
pub fn requests() -> Vec<OpenRequest> {
    requests_at(&dir(), now_ms())
}

pub(crate) fn requests_at(base: &Path, now: u64) -> Vec<OpenRequest> {
    let Ok(entries) = std::fs::read_dir(requests_dir(base)) else {
        return Vec::new();
    };
    let mut out: Vec<OpenRequest> = entries
        .flatten()
        .filter_map(|e| {
            let pid = e.path().file_stem()?.to_str()?.parse::<u32>().ok()?;
            read_request(base, pid, now)
        })
        .collect();
    out.sort_by_key(|r| r.pid);
    out
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

    #[test]
    fn an_open_request_waits_for_the_user_and_carries_the_answer() {
        let base = scratch("presence-req-base");
        let data = scratch("presence-req-data");
        let proj = scratch("presence-req-project");
        std::fs::create_dir_all(proj.join("ch")).unwrap();
        std::fs::write(proj.join("ch/intro.tex"), "x").unwrap();
        let now = now_ms();

        assert!(
            request_open_at(&base, &data, now, None, &proj.to_string_lossy(), None)
                .unwrap_err()
                .contains("no Maleficium window")
        );
        // This process plays the window.
        let me = Presence {
            pid: std::process::id(),
            project: None,
            main_rel: None,
            active_rel: None,
            started_ms: now,
            updated_ms: now,
        };
        write(&base, &me).unwrap();
        let p = proj.to_string_lossy().to_string();
        // Absolute on every OS (a bare `/x` is not absolute on Windows).
        let missing = base.join("no").join("such").join("dir");
        let missing = missing.to_string_lossy().to_string();
        let outside = base.join("passwd").to_string_lossy().to_string();
        for (pid, project, file, says) in [
            (
                Some(1u32),
                p.as_str(),
                None,
                "no open Maleficium window has pid 1",
            ),
            (None, "relative/dir", None, "not an absolute folder"),
            (None, missing.as_str(), None, "does not exist"),
            (
                None,
                p.as_str(),
                Some(outside.as_str()),
                "relative to the project",
            ),
            (None, p.as_str(), Some("../x.tex"), "does not exist"),
            (None, p.as_str(), Some("ch"), "not a file in the project"),
        ] {
            let e = request_open_at(&base, &data, now, pid, project, file).unwrap_err();
            assert!(e.contains(says), "{project} {file:?}: {e}");
        }
        let inside = data.join("x");
        std::fs::create_dir_all(&inside).unwrap();
        assert!(
            request_open_at(&base, &data, now, None, &inside.to_string_lossy(), None)
                .unwrap_err()
                .contains("Maleficium's own data")
        );

        let r = request_open_at(&base, &data, now, None, &p, Some("./ch/intro.tex")).unwrap();
        assert_eq!(r.file.as_deref(), Some("ch/intro.tex"));
        assert_eq!(pending_at(&base, now).as_ref(), Some(&r));
        assert!(request_open_at(&base, &data, now, None, &p, None)
            .unwrap_err()
            .contains("not answered"));
        assert!(answer_at(&base, now, "other", OpenOutcome::Opened).is_err());
        answer_at(&base, now, &r.id, OpenOutcome::Dismissed).unwrap();
        assert!(
            pending_at(&base, now).is_none(),
            "answered: no longer shown"
        );
        let seen = requests_at(&base, now);
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].outcome, Some(OpenOutcome::Dismissed));
        // Answered, a new ask may follow; a stale one is dropped.
        request_open_at(&base, &data, now, None, &p, None).unwrap();
        assert!(requests_at(&base, now + REQUEST_STALE_MS + 1).is_empty());
        assert!(
            pending_at(&base, now).is_none(),
            "the stale ask was removed"
        );
    }
}
