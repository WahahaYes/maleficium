//! The runtime project-scope grant. Input validation lives in
//! `maleficium_core::guard`.

use maleficium_core::Core;
use std::path::PathBuf;
use tauri::State;

use tauri::AppHandle;
use tauri_plugin_fs::FsExt;

/// Fetch the live fs scope, or fail closed when unavailable (never panic —
/// `try_fs_scope` returns None outside a managed window context).
pub fn live_scope(app: &AppHandle) -> Result<tauri::fs::Scope, String> {
    app.try_fs_scope()
        .ok_or_else(|| "forbidden path: fs scope unavailable".to_string())
}

/// A granted project: its canonical path and the session-root id that the
/// compile and SyncTeX commands take.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectGrant {
    pub path: String,
    pub root_id: String,
}

/// Open a registered root in the live fs scope, recursively.
fn allow_granted(app: &AppHandle, canon: PathBuf, root_id: String) -> Result<ProjectGrant, String> {
    live_scope(app)?
        .allow_directory(&canon, true)
        .map_err(|e| format!("grant failed for {}: {}", canon.display(), e))?;
    Ok(ProjectGrant {
        path: canon.to_string_lossy().to_string(),
        root_id,
    })
}

/// Mint a recursive runtime fs-scope grant for one validated project root and
/// register it as a session root. Granted roots stay readable until quit. One
/// unconditional code path for every open route (dialog pick, recent,
/// restore, preset).
#[tauri::command]
pub fn grant_project_access(
    cx: State<'_, Core>,
    app: AppHandle,
    root: String,
) -> Result<ProjectGrant, String> {
    let (canon, root_id) = maleficium_core::grant_project(&cx, &root)?;
    allow_granted(&app, canon, root_id)
}

/// Grant the backend-owned scratch root that untitled documents compile in.
#[tauri::command]
pub fn grant_untitled_access(cx: State<'_, Core>, app: AppHandle) -> Result<ProjectGrant, String> {
    let (canon, root_id) = maleficium_core::grant_untitled(&cx)?;
    allow_granted(&app, canon, root_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;

    /// Fresh canonical scratch dir per test (pid + name; cleaned first).
    fn scratch(name: &str) -> PathBuf {
        let base = maleficium_core::test_scratch::dir(&format!("guard-{}", name));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        dunce::canonicalize(&base).unwrap()
    }

    #[test]
    fn grant_root_id_is_the_utf8_hash_for_non_ascii_roots() {
        // The frontend keys main-file associations and trash by this id and
        // never rehashes; a UTF-16 hash gave 6ea6b60c here.
        assert_eq!(maleficium_core::hash_root("/home/josé/thèse"), "53bf67b2");
        let dir = scratch("non-ascii").join("josé").join("thèse");
        fs::create_dir_all(&dir).unwrap();
        let cx = &Core::default();
        let (canon, root_id) = maleficium_core::grant_project(cx, &dir.to_string_lossy()).unwrap();
        assert_eq!(
            root_id,
            maleficium_core::hash_root(&canon.to_string_lossy())
        );
        let grant = ProjectGrant {
            path: canon.to_string_lossy().to_string(),
            root_id: root_id.clone(),
        };
        let json = serde_json::to_value(&grant).unwrap();
        assert_eq!(json["rootId"], root_id);
        let path = json["path"].as_str().unwrap();
        assert!(
            Path::new(path).ends_with(Path::new("josé").join("thèse")),
            "{path}"
        );
    }
}
