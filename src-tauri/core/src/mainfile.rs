//! Main-file resolution and associations: every adapter resolves the same
//! root document. Order: explicit association → `%!TEX root` magic →
//! `\documentclass` scan (first wins, deterministic) → single-.tex fallback
//! → none. Resolution never throws: unreadable inputs read as "no main
//! file"; only an unknown root is an error.
//!
//! Associations live here keyed by root id (the grant's id, stable for the
//! session), persisted as one JSON map under the app-data dir — never as an
//! in-project file. Joined paths are cleaned lexically, so `sub/../main.tex`
//! resolves to `main.tex` instead of travelling with its `..`.

use crate::Core;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use ts_rs::TS;

/// Where the resolved main file came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum MainSource {
    Config,
    Magic,
    Scan,
    Single,
    None,
}

/// The resolved root document: absolute native paths, as the app state
/// holds today. `candidates` carries the tied scan hits when >1.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct MainResolution {
    pub main: Option<String>,
    pub source: MainSource,
    pub candidates: Vec<String>,
}

impl MainResolution {
    fn none() -> Self {
        MainResolution {
            main: None,
            source: MainSource::None,
            candidates: Vec::new(),
        }
    }
}

/// Collapse `.` and `..` lexically, without touching the filesystem: a
/// leading `..` past the start clamps instead of escaping.
fn clean(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for comp in path.components() {
        match comp {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            c => out.push(c.as_os_str()),
        }
    }
    if out.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        out
    }
}

fn parse_magic_comment(content: &str) -> Option<String> {
    use regex::Regex;
    use std::sync::OnceLock;
    static MAGIC_RE: OnceLock<Regex> = OnceLock::new();
    let re = MAGIC_RE.get_or_init(|| {
        Regex::new(r"(?im)^\s*%\s*!\s*TEX\s+root\s*=\s*(.+?)\s*$").expect("magic comment regex")
    });
    let v = re.captures(content)?.get(1)?.as_str().trim();
    let v = v.strip_prefix('"').unwrap_or(v);
    let v = v.strip_prefix('\'').unwrap_or(v);
    let v = v.strip_suffix('"').unwrap_or(v);
    let v = v.strip_suffix('\'').unwrap_or(v);
    let v = v.trim();
    if v.is_empty() {
        None
    } else {
        Some(v.to_string())
    }
}

fn has_documentclass(content: &str) -> bool {
    use regex::Regex;
    use std::sync::OnceLock;
    static CLASS_RE: OnceLock<Regex> = OnceLock::new();
    let re = CLASS_RE.get_or_init(|| {
        Regex::new(r"\\documentclass(\[[^\]]*\])?\{[^}]*\}").expect("documentclass regex")
    });
    re.is_match(content)
}

/// Folders the scan never descends into: version control, build outputs,
/// and the trash staging dir.
fn walk_tex(root: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name == ".git" || name == "out" || name == ".maleficium-trash" {
                continue;
            }
            walk_tex(&path, out);
        } else if path.is_file() && path.extension().is_some_and(|e| e == "tex") {
            out.push(path);
        }
    }
}

/// Where one app-data JSON map of root id → associated rel lives.
fn assoc_path() -> PathBuf {
    crate::data_base_dir()
        .join("maleficium-mainfile")
        .join("associations.json")
}

fn load_assocs(path: &Path) -> HashMap<String, String> {
    let Ok(raw) = std::fs::read_to_string(path) else {
        return HashMap::new();
    };
    let Ok(json) = serde_json::from_str::<HashMap<String, serde_json::Value>>(&raw) else {
        // Unparseable stored associations: start with none.
        return HashMap::new();
    };
    json.into_iter()
        .filter_map(|(k, v)| {
            v.as_str()
                .filter(|s| !s.trim().is_empty())
                .map(|s| (k, s.to_string()))
        })
        .collect()
}

fn store_assocs(path: &Path, map: &HashMap<String, String>) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("main-file store unreachable: {}", e))?;
    }
    let json = serde_json::to_string_pretty(map).map_err(|e| e.to_string())?;
    std::fs::write(path, json).map_err(|e| format!("main-file store unwritable: {}", e))
}

/// The explicit association for one root (a rel, as stored), or `None`.
pub fn association(cx: &Core, root_id: &str) -> Result<Option<String>, String> {
    super::fs::session_root(cx, root_id)?;
    Ok(load_assocs(&assoc_path()).remove(root_id))
}

/// Persist the explicit association ("Set as Main File") for one root.
pub fn set_association(cx: &Core, root_id: &str, rel: &str) -> Result<(), String> {
    super::fs::session_root(cx, root_id)?;
    if rel.trim().is_empty() {
        return Err(String::from("empty main-file association"));
    }
    let path = assoc_path();
    let mut map = load_assocs(&path);
    map.insert(root_id.to_string(), rel.to_string());
    store_assocs(&path, &map)
}

/// Resolve the main file for a granted root. `opened_abs` is the absolute
/// path of the currently opened file (may be non-main), for the magic
/// comment step.
pub fn resolve(
    cx: &Core,
    root_id: &str,
    opened_abs: Option<&str>,
) -> Result<MainResolution, String> {
    let root = super::fs::session_root(cx, root_id)?;
    let assoc = load_assocs(&assoc_path()).remove(root_id);
    Ok(resolve_with(&root, assoc.as_deref(), opened_abs))
}

fn resolve_with(root: &Path, assoc_rel: Option<&str>, opened_abs: Option<&str>) -> MainResolution {
    // 1) explicit association
    if let Some(rel) = assoc_rel.filter(|r| !r.trim().is_empty()) {
        let joined = clean(&root.join(rel));
        return MainResolution {
            main: Some(joined.to_string_lossy().to_string()),
            source: MainSource::Config,
            candidates: Vec::new(),
        };
    }

    // 2) magic comment in the opened file
    if let Some(opened) = opened_abs {
        if let Ok(content) = std::fs::read_to_string(opened) {
            if let Some(magic) = parse_magic_comment(&content) {
                let base = Path::new(opened)
                    .parent()
                    .filter(|p| !p.as_os_str().is_empty())
                    .map(Path::to_path_buf)
                    .unwrap_or_else(|| root.to_path_buf());
                let resolved = if Path::new(&magic).is_absolute() {
                    clean(Path::new(&magic))
                } else {
                    clean(&base.join(&magic))
                };
                return MainResolution {
                    main: Some(resolved.to_string_lossy().to_string()),
                    source: MainSource::Magic,
                    candidates: Vec::new(),
                };
            }
        }
    }

    // 3) scan for \documentclass (deterministic: sorted, first wins)
    let mut tex_files: Vec<PathBuf> = Vec::new();
    walk_tex(root, &mut tex_files);
    tex_files.sort();
    let mut with_class: Vec<String> = Vec::new();
    for f in &tex_files {
        if std::fs::read_to_string(f)
            .map(|c| has_documentclass(&c))
            .unwrap_or(false)
        {
            with_class.push(f.to_string_lossy().to_string());
        }
    }
    if let Some(first) = with_class.first() {
        return MainResolution {
            main: Some(first.clone()),
            source: MainSource::Scan,
            candidates: if with_class.len() > 1 {
                with_class
            } else {
                Vec::new()
            },
        };
    }

    // 4) single-.tex fallback
    if tex_files.len() == 1 {
        return MainResolution {
            main: Some(tex_files[0].to_string_lossy().to_string()),
            source: MainSource::Single,
            candidates: Vec::new(),
        };
    }
    MainResolution::none()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str, files: &[(&str, &str)]) -> PathBuf {
        let dir = crate::test_scratch::dir(name);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for (rel, content) in files {
            let path = dir.join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, content).unwrap();
        }
        dunce::canonicalize(&dir).unwrap()
    }

    const MAIN: &str = "\\documentclass{article}\n\\begin{document}hi\\end{document}\n";

    #[test]
    fn magic_parses_with_spacing_and_quotes() {
        assert_eq!(
            parse_magic_comment("%!TEX root = main.tex\n\\input{body}"),
            Some(String::from("main.tex"))
        );
        assert_eq!(
            parse_magic_comment("% ! TEX root=  \"ch/main.tex\" "),
            Some(String::from("ch/main.tex"))
        );
        assert_eq!(parse_magic_comment("\\documentclass{article}"), None);
    }

    #[test]
    fn documentclass_needs_the_command() {
        assert!(has_documentclass("\\documentclass[11pt]{article}"));
        assert!(!has_documentclass("\\section{Hi}"));
    }

    #[test]
    fn config_wins() {
        let root = fixture(
            "mainfile-config",
            &[("main.tex", MAIN), ("ch1.tex", "\\section{One}\n")],
        );
        let r = resolve_with(
            &root,
            Some("main.tex"),
            Some(&root.join("ch1.tex").to_string_lossy()),
        );
        assert_eq!(r.source, MainSource::Config);
        assert_eq!(
            r.main,
            Some(root.join("main.tex").to_string_lossy().to_string())
        );
    }

    #[test]
    fn magic_wins_over_scan_and_canonicalizes_dotdot() {
        let root = fixture(
            "mainfile-magic",
            &[
                ("main.tex", MAIN),
                ("sub/ch.tex", "%!TEX root = ../main.tex\n\\section{Two}\n"),
            ],
        );
        let r = resolve_with(
            &root,
            None,
            Some(&root.join("sub/ch.tex").to_string_lossy()),
        );
        assert_eq!(r.source, MainSource::Magic);
        assert_eq!(
            r.main,
            Some(root.join("main.tex").to_string_lossy().to_string())
        );
    }

    #[test]
    fn scan_finds_documentclass_deterministically() {
        let root = fixture(
            "mainfile-scan",
            &[
                ("main.tex", MAIN),
                ("ch1.tex", "\\section{One}\n"),
                ("z.tex", "\\documentclass{book}\n"),
            ],
        );
        let r = resolve_with(&root, None, Some(&root.join("ch1.tex").to_string_lossy()));
        assert_eq!(r.source, MainSource::Scan);
        assert_eq!(
            r.main,
            Some(root.join("main.tex").to_string_lossy().to_string())
        );
        assert_eq!(
            r.candidates,
            vec![
                root.join("main.tex").to_string_lossy().to_string(),
                root.join("z.tex").to_string_lossy().to_string(),
            ]
        );
    }

    #[test]
    fn single_tex_falls_back() {
        let root = fixture("mainfile-single", &[("only.tex", "\\section{only}\n")]);
        let r = resolve_with(&root, None, None);
        assert_eq!(r.source, MainSource::Single);
        assert_eq!(
            r.main,
            Some(root.join("only.tex").to_string_lossy().to_string())
        );
    }

    #[test]
    fn empty_root_resolves_to_none_without_throwing() {
        let root = fixture("mainfile-empty", &[]);
        let r = resolve_with(&root, None, None);
        assert_eq!(r.source, MainSource::None);
        assert!(r.main.is_none());
    }

    #[test]
    fn unlistable_root_resolves_to_none() {
        let r = resolve_with(
            Path::new("/nonexistent/maleficium-mainfile-probe"),
            None,
            None,
        );
        assert_eq!(r.source, MainSource::None);
    }

    #[test]
    fn assoc_store_round_trips_and_tolerates_garbage() {
        let dir = crate::test_scratch::dir("mainfile-assoc");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("associations.json");
        assert!(load_assocs(&path).is_empty());
        let mut map = HashMap::new();
        map.insert("r1".to_string(), "ch/main.tex".to_string());
        store_assocs(&path, &map).unwrap();
        assert_eq!(
            load_assocs(&path).get("r1"),
            Some(&String::from("ch/main.tex"))
        );
        std::fs::write(&path, "{nope").unwrap();
        assert!(load_assocs(&path).is_empty());
    }

    #[test]
    fn resolve_and_set_reject_unknown_roots() {
        let cx = &Core::default();
        assert!(resolve(cx, "nope", None).is_err());
        assert!(association(cx, "nope").is_err());
        assert!(set_association(cx, "nope", "main.tex").is_err());
    }

    #[test]
    fn set_rejects_empty_rels() {
        let cx = &Core::default();
        let dir = crate::test_scratch::dir("mainfile-grant");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let canon = dunce::canonicalize(&dir).unwrap();
        super::super::fs::grant_root(cx, "mainfile-empty-rel", &canon.to_string_lossy()).unwrap();
        assert!(set_association(cx, "mainfile-empty-rel", "  ").is_err());
    }
}
