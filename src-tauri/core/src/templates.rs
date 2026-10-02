//! Project templates: the bundled set embedded in the binary, and the
//! user's own under app data. Instantiating writes one new project folder;
//! saving or importing a template writes only under app data.

use crate::Core;

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

include!(concat!(env!("OUT_DIR"), "/templates.rs"));

const MANIFEST: &str = "template.json";
/// The first-run tour: bundled, instantiable, never listed.
pub const WELCOME: &str = "welcome";

/// A template as the gallery shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct TemplateInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub category: String,
    /// Root-relative main file.
    pub main: String,
    /// Saved or imported by the user (under app data), not bundled.
    #[serde(default)]
    pub user: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct TemplateList {
    pub templates: Vec<TemplateInfo>,
    /// User template folders whose manifest could not be read.
    pub unreadable: Vec<String>,
}

/// A project made from a template.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct Created {
    /// Absolute path of the new project folder.
    pub root: String,
    /// Root-relative main file.
    pub main: String,
}

/// Where the user's own templates live, under an app-data base.
fn user_dir(base: &Path) -> PathBuf {
    base.join("maleficium-templates")
}

/// Where the welcome project is kept, under an app-data base.
fn welcome_dir(base: &Path) -> PathBuf {
    base.join("maleficium-welcome")
}

/// A template's files as loaded: (relative path, bytes).
type Files = Vec<(String, Vec<u8>)>;

fn bundled(id: &str) -> Option<TemplateFiles> {
    TEMPLATES.iter().find(|(i, _)| *i == id).map(|(_, f)| *f)
}

fn manifest(bytes: &[u8], user: bool) -> Result<TemplateInfo, String> {
    let mut t: TemplateInfo =
        serde_json::from_slice(bytes).map_err(|e| format!("bad {MANIFEST}: {e}"))?;
    t.user = user;
    Ok(t)
}

/// Every template but the welcome project: bundled first, then the user's.
pub fn list() -> TemplateList {
    list_in(&super::data_base_dir())
}

fn list_in(base: &Path) -> TemplateList {
    let mut templates: Vec<TemplateInfo> = TEMPLATES
        .iter()
        .filter(|(id, _)| *id != WELCOME)
        .filter_map(|(_, files)| files.iter().find(|(r, _)| *r == MANIFEST))
        .filter_map(|(_, b)| manifest(b, false).ok())
        .collect();
    let mut unreadable = Vec::new();
    if let Ok(entries) = std::fs::read_dir(user_dir(base)) {
        let mut user: Vec<TemplateInfo> = Vec::new();
        for e in entries.flatten() {
            let path = e.path().join(MANIFEST);
            match std::fs::read(&path)
                .map_err(|e| e.to_string())
                .and_then(|b| manifest(&b, true))
            {
                Ok(t) => user.push(t),
                Err(err) => unreadable.push(format!("{}: {err}", e.file_name().to_string_lossy())),
            }
        }
        user.sort_by(|a, b| a.name.cmp(&b.name));
        templates.extend(user);
    }
    TemplateList {
        templates,
        unreadable,
    }
}

/// A template's manifest and files (manifest excluded), bundled or user.
fn load(base: &Path, id: &str) -> Result<(TemplateInfo, Files), String> {
    valid_id(id)?;
    if let Some(files) = bundled(id) {
        let (_, m) = files
            .iter()
            .find(|(r, _)| *r == MANIFEST)
            .ok_or_else(|| format!("template {id} has no {MANIFEST}"))?;
        let info = manifest(m, false)?;
        let body = files
            .iter()
            .filter(|(r, _)| *r != MANIFEST)
            .map(|(r, b)| (r.to_string(), b.to_vec()))
            .collect();
        return Ok((info, body));
    }
    let dir = user_dir(base).join(id);
    let info = manifest(
        &std::fs::read(dir.join(MANIFEST)).map_err(|_| format!("no template named {id}"))?,
        true,
    )?;
    let mut body = Vec::new();
    for rel in super::export::source_files(&dir)? {
        if rel != MANIFEST {
            let b = std::fs::read(dir.join(&rel)).map_err(|e| format!("cannot read {rel}: {e}"))?;
            body.push((rel, b));
        }
    }
    Ok((info, body))
}

fn valid_id(id: &str) -> Result<(), String> {
    let ok = !id.is_empty()
        && id.len() <= 48
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
    ok.then_some(())
        .ok_or_else(|| format!("template id must be lowercase letters, digits and dashes: {id}"))
}

/// A new folder's name: one plain path segment, not hidden.
fn valid_folder(name: &str) -> Result<(), String> {
    let bad = name.is_empty()
        || name.len() > 120
        || name.starts_with('.')
        || name.contains(['/', '\\', '\0']);
    (!bad)
        .then_some(())
        .ok_or_else(|| format!("not a usable folder name: {name:?}"))
}

/// Write `files` into `dest`, which must not exist or be empty.
fn write_into(dest: &Path, files: &[(String, Vec<u8>)]) -> Result<(), String> {
    if dest.exists() {
        let empty = std::fs::read_dir(dest)
            .map_err(|e| format!("cannot read {}: {e}", dest.display()))?
            .next()
            .is_none();
        if !empty {
            return Err(format!(
                "{} already exists and is not empty",
                dest.display()
            ));
        }
    }
    for (rel, bytes) in files {
        let p = dest.join(rel);
        if let Some(dir) = p.parent() {
            std::fs::create_dir_all(dir)
                .map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
        }
        std::fs::write(&p, bytes).map_err(|e| format!("cannot write {rel}: {e}"))?;
    }
    Ok(())
}

/// Make `parent/name` from a template.
pub fn instantiate(template: &str, parent: &str, name: &str) -> Result<Created, String> {
    instantiate_in(&super::data_base_dir(), template, parent, name)
}

fn instantiate_in(
    base: &Path,
    template: &str,
    parent: &str,
    name: &str,
) -> Result<Created, String> {
    valid_folder(name)?;
    let parent = Path::new(parent);
    if !parent.is_absolute() {
        return Err(format!(
            "parent folder must be absolute: {}",
            parent.display()
        ));
    }
    let parent =
        dunce::canonicalize(parent).map_err(|e| format!("parent folder unreachable: {e}"))?;
    if !parent.is_dir() {
        return Err(format!("parent is not a folder: {}", parent.display()));
    }
    let (info, files) = load(base, template)?;
    let dest = parent.join(name);
    crate::widget_approval::refuse_store_overlap(&dest, &crate::widget_approval::store_base())?;
    write_into(&dest, &files)?;
    Ok(Created {
        root: dest.to_string_lossy().to_string(),
        main: info.main,
    })
}

/// The welcome project, written into app data the first time it is asked for.
pub fn welcome() -> Result<Created, String> {
    welcome_in(&super::data_base_dir())
}

fn welcome_in(base: &Path) -> Result<Created, String> {
    let dir = welcome_dir(base);
    let (info, files) = load(base, WELCOME)?;
    if !dir.join(&info.main).is_file() {
        write_into(&dir, &files)?;
    }
    Ok(Created {
        root: dir.to_string_lossy().to_string(),
        main: info.main,
    })
}

/// Copy `src`'s sources into a new user template.
fn store_template(base: &Path, src: &Path, info: TemplateInfo) -> Result<TemplateInfo, String> {
    valid_id(&info.id)?;
    if bundled(&info.id).is_some() || user_dir(base).join(&info.id).exists() {
        return Err(format!("a template named {} already exists", info.id));
    }
    let sources = super::export::source_files(src)?;
    if !sources.contains(&info.main) {
        return Err(format!("{} is not a file of this project", info.main));
    }
    let mut files = Vec::with_capacity(sources.len() + 1);
    for rel in sources {
        let b = std::fs::read(src.join(&rel)).map_err(|e| format!("cannot read {rel}: {e}"))?;
        files.push((rel, b));
    }
    let manifest = serde_json::to_vec_pretty(&TemplateInfo {
        user: false,
        ..info.clone()
    })
    .map_err(|e| e.to_string())?;
    files.push((MANIFEST.to_string(), manifest));
    write_into(&user_dir(base).join(&info.id), &files)?;
    Ok(TemplateInfo { user: true, ..info })
}

/// Save a granted project as a user template.
pub fn save_project(cx: &Core, root_id: &str, info: TemplateInfo) -> Result<TemplateInfo, String> {
    store_template(
        &super::data_base_dir(),
        &super::fs::session_root(cx, root_id)?,
        info,
    )
}

/// Import any folder as a user template.
pub fn import_folder(dir: &str, info: TemplateInfo) -> Result<TemplateInfo, String> {
    import_folder_in(&super::data_base_dir(), dir, info)
}

fn import_folder_in(base: &Path, dir: &str, info: TemplateInfo) -> Result<TemplateInfo, String> {
    let d = Path::new(dir);
    if !d.is_absolute() || !d.is_dir() {
        return Err(format!("not a folder: {dir}"));
    }
    store_template(
        base,
        &dunce::canonicalize(d).map_err(|e| e.to_string())?,
        info,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = crate::test_scratch::dir(&format!("tpl-{name}"));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        dunce::canonicalize(&d).unwrap()
    }

    #[test]
    fn the_bundled_set_is_listed_without_the_welcome_project() {
        let ids: Vec<String> = list_in(&tmp("list"))
            .templates
            .into_iter()
            .filter(|t| !t.user)
            .map(|t| t.id)
            .collect();
        for want in [
            "article",
            "assignment",
            "beamer",
            "book",
            "cv",
            "journal",
            "letter",
            "report",
            "resume",
        ] {
            assert!(
                ids.contains(&want.to_string()),
                "{want} missing from {ids:?}"
            );
        }
        assert!(!ids.contains(&WELCOME.to_string()));
        for t in list_in(&tmp("list2")).templates.iter().filter(|t| !t.user) {
            assert!(!t.name.is_empty() && !t.description.is_empty() && !t.category.is_empty());
            let (_, files) = load(&tmp("load"), &t.id).unwrap();
            assert!(
                files.iter().any(|(r, _)| *r == t.main),
                "{} lacks its main",
                t.id
            );
        }
    }

    #[test]
    fn instantiate_writes_a_fresh_folder_and_refuses_a_used_one() {
        let parent = tmp("inst");
        let base = tmp("inst-base");
        let instantiate = |t: &str, p: &str, n: &str| instantiate_in(&base, t, p, n);
        let c = instantiate("report", &parent.to_string_lossy(), "thesis").unwrap();
        assert_eq!(c.main, "main.tex");
        let root = PathBuf::from(&c.root);
        assert!(root.join("chapters/introduction.tex").is_file());
        assert!(!root.join(MANIFEST).exists());
        let again = instantiate("report", &parent.to_string_lossy(), "thesis").unwrap_err();
        assert!(again.contains("not empty"), "{again}");
        for bad in ["", ".hidden", "a/b", "..", "x\0y"] {
            assert!(
                instantiate("article", &parent.to_string_lossy(), bad).is_err(),
                "{bad:?}"
            );
        }
        let cwd = std::env::current_dir().unwrap();
        for rel in ["relative", "", ".", "..", "~/talks", "./x"] {
            let err = instantiate("beamer", rel, "stray-beamer").unwrap_err();
            assert!(
                err.contains("absolute") || err.contains("unreachable"),
                "{rel:?}: {err}"
            );
        }
        assert!(!cwd.join("stray-beamer").exists());
        std::fs::write(parent.join("file"), "x").unwrap();
        let not_dir = parent.join("file").to_string_lossy().to_string();
        assert!(instantiate("article", &not_dir, "p")
            .unwrap_err()
            .contains("not a folder"));
        assert!(instantiate("no-such", &parent.to_string_lossy(), "p").is_err());
        assert!(instantiate("../etc", &parent.to_string_lossy(), "p").is_err());
    }

    #[test]
    fn saved_templates_list_and_instantiate_and_ids_stay_unique() {
        let base = tmp("save-base");
        let proj = tmp("save-src");
        std::fs::write(proj.join("paper.tex"), "\\documentclass{article}").unwrap();
        std::fs::create_dir_all(proj.join(".git")).unwrap();
        std::fs::write(proj.join("paper.aux"), "x").unwrap();
        let id = format!("mine-{}", std::process::id());
        let info = TemplateInfo {
            id: id.clone(),
            name: "Mine".into(),
            description: "d".into(),
            category: "Your templates".into(),
            main: "paper.tex".into(),
            user: false,
        };
        let saved = import_folder_in(&base, &proj.to_string_lossy(), info.clone()).unwrap();
        assert!(saved.user);
        assert!(list_in(&base)
            .templates
            .iter()
            .any(|t| t.id == id && t.user));
        assert!(
            import_folder_in(&base, &proj.to_string_lossy(), info.clone())
                .unwrap_err()
                .contains("already exists")
        );
        let bundled_id = TemplateInfo {
            id: "article".into(),
            ..info.clone()
        };
        assert!(import_folder_in(&base, &proj.to_string_lossy(), bundled_id).is_err());
        let wrong_main = TemplateInfo {
            id: format!("{id}-b"),
            main: "gone.tex".into(),
            ..info
        };
        assert!(import_folder_in(&base, &proj.to_string_lossy(), wrong_main).is_err());
        let out = tmp("save-out");
        let c = instantiate_in(&base, &id, &out.to_string_lossy(), "copy").unwrap();
        let root = PathBuf::from(c.root);
        assert!(root.join("paper.tex").is_file());
        assert!(!root.join("paper.aux").exists() && !root.join(".git").exists());
    }

    #[test]
    fn welcome_is_written_once_into_app_data() {
        let base = tmp("welcome");
        let c = welcome_in(&base).unwrap();
        assert!(PathBuf::from(&c.root).starts_with(&base));
        let main = PathBuf::from(&c.root).join(&c.main);
        std::fs::write(&main, "edited").unwrap();
        assert_eq!(welcome_in(&base).unwrap(), c);
        assert_eq!(std::fs::read_to_string(&main).unwrap(), "edited");
    }
}
