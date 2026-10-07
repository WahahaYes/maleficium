//! Authoring drafts for custom widget runtimes: scaffold a new package
//! into the user library and validate a package wherever it sits.
//!
//! The scaffold writes a caption-overlay-shaped draft (its files are the
//! documented sample, embedded): `runtime.json`, a classic `index.html`
//! that loads the built `bridge.js`, `samples/` for the required role,
//! and `LICENSE`. `--from model@1` starts from the built-in model viewer
//! instead: its built entry plus its sources as reference. Validation
//! runs the package check and the static scan over a folder. Neither
//! approves anything: approval stays a user action in the app.

use crate::runtimes::{self, RuntimeManifest};
use crate::Core;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// The user library of runtime drafts, under the app-data dir.
pub const LIBRARY_DIR: &str = "maleficium-runtimes";
/// The only `--from` source: the built-in model viewer.
pub const MODEL_REF: &str = "model@1";
/// New drafts start at major 1.
const DRAFT_VERSION: &str = "1.0.0";

const DRAFT_MANIFEST: &str =
    include_str!("../../../docs/runtimes/samples/caption-overlay@1/runtime.json");
const DRAFT_ENTRY: &str =
    include_str!("../../../docs/runtimes/samples/caption-overlay@1/index.html");
const DRAFT_SAMPLE: &[u8] =
    include_bytes!("../../../docs/runtimes/samples/caption-overlay@1/samples/photo.svg");
const DRAFT_LICENSE: &str =
    include_str!("../../../docs/runtimes/samples/caption-overlay@1/LICENSE");
const BRIDGE_JS: &[u8] = include_bytes!("../../../src-tauri/widget-runtimes/bridge/bridge.js");

const MODEL_ENTRY: &[u8] = include_bytes!("../../../src-tauri/widget-runtimes/model/index.html");
const MODEL_MAIN: &str = include_str!("../../../src/widget-runtimes/model/main.ts");
const MODEL_CORE: &str = include_str!("../../../src/widget-runtimes/model/core.ts");
const MODEL_HTML: &str = include_str!("../../../src/widget-runtimes/model/index.html");
const MODEL_SAMPLE: &[u8] = include_bytes!("../../../e2e/fixtures/interactive/models/mesh.glb");

/// The user library dir: every draft sits at `<it>/<name>@1/`.
pub fn library_dir() -> PathBuf {
    crate::data_base_dir().join(LIBRARY_DIR)
}

/// One draft: its `<name>@1` reference and its files by package path.
pub struct Draft {
    pub reference: String,
    pub files: BTreeMap<String, Vec<u8>>,
}

/// The draft's files for `name`, from the documented sample or from the
/// built-in model viewer. Only the name (and the title) is parameterized.
pub fn draft_files(name: &str, from: Option<&str>) -> Result<Draft, String> {
    if !runtimes::valid_name(name) {
        return Err(format!(
            "`{name}` is not a runtime name: a lowercase letter, 1 to 39 letters, digits or dashes, not a built-in"
        ));
    }
    match from {
        None => Ok(Draft {
            reference: format!("{name}@1"),
            files: plain_files(name)?,
        }),
        Some(MODEL_REF) => Ok(Draft {
            reference: format!("{name}@1"),
            files: model_files(name),
        }),
        Some(f) => Err(format!("unknown --from `{f}` (only {MODEL_REF})")),
    }
}

fn manifest_text(name: &str) -> Result<String, String> {
    let mut v: serde_json::Value = serde_json::from_str(DRAFT_MANIFEST)
        .map_err(|e| format!("the sample manifest broke: {e}"))?;
    v["name"] = serde_json::Value::String(name.to_string());
    v["title"] = serde_json::Value::String(name.to_string());
    let mut text =
        serde_json::to_string_pretty(&v).map_err(|e| format!("the sample manifest broke: {e}"))?;
    text.push('\n');
    Ok(text)
}

/// The caption-overlay-shaped draft: the sample's manifest (named), entry
/// (loading the built bridge), sample and licence.
fn plain_files(name: &str) -> Result<BTreeMap<String, Vec<u8>>, String> {
    let (head, tail) = DRAFT_ENTRY
        .split_once("  </body>")
        .ok_or_else(|| "the sample entry has no </body> (update the scaffold)".to_string())?;
    let mut entry = String::with_capacity(DRAFT_ENTRY.len() + 64);
    entry.push_str(head);
    entry.push_str("    <script src=\"bridge.js\"></script>\n");
    entry.push_str("  </body>");
    entry.push_str(tail);
    Ok(BTreeMap::from([
        (
            "runtime.json".to_string(),
            manifest_text(name)?.into_bytes(),
        ),
        ("index.html".to_string(), entry.into_bytes()),
        ("bridge.js".to_string(), BRIDGE_JS.to_vec()),
        ("samples/photo.svg".to_string(), DRAFT_SAMPLE.to_vec()),
        ("LICENSE".to_string(), DRAFT_LICENSE.as_bytes().to_vec()),
    ]))
}

/// A model manifest for a `model@1` fork: the glb role and WebGL on. The
/// viewer's camera, size and background stay sidecar-level knobs the fork
/// author declares once they rename them: the package reserves those keys.
fn model_manifest(name: &str) -> Vec<u8> {
    let v = serde_json::json!({
        "contract": 1,
        "name": name,
        "version": DRAFT_VERSION,
        "title": name,
        "description": "A viewer for glb models, forked from the built-in model runtime.",
        "authors": ["Author"],
        "license": "MIT",
        "sources": {
            "model": {
                "primary": true,
                "required": true,
                "extensions": ["glb"],
                "description": "The model to show."
            }
        },
        "capabilities": {"webgl": true},
        "vendored": []
    });
    let mut text = serde_json::to_string_pretty(&v).expect("a literal manifest serializes");
    text.push('\n');
    text.into_bytes()
}

/// The `model@1` fork: the built viewer as the entry, its sources as
/// reference, a glb sample for the required role.
fn model_files(name: &str) -> BTreeMap<String, Vec<u8>> {
    BTreeMap::from([
        ("runtime.json".to_string(), model_manifest(name)),
        ("index.html".to_string(), MODEL_ENTRY.to_vec()),
        ("bridge.js".to_string(), BRIDGE_JS.to_vec()),
        ("src/main.ts".to_string(), MODEL_MAIN.as_bytes().to_vec()),
        ("src/core.ts".to_string(), MODEL_CORE.as_bytes().to_vec()),
        ("src/index.html".to_string(), MODEL_HTML.as_bytes().to_vec()),
        ("samples/mesh.glb".to_string(), MODEL_SAMPLE.to_vec()),
        ("LICENSE".to_string(), DRAFT_LICENSE.as_bytes().to_vec()),
    ])
}

/// Write the draft for `name` into `base/<name>@1/`, refused when that
/// folder exists and is not empty. Returns the draft dir.
pub fn scaffold_into(base: &Path, name: &str, from: Option<&str>) -> Result<PathBuf, String> {
    let draft = draft_files(name, from)?;
    let dest = base.join(&draft.reference);
    if dest.exists() {
        let empty = dest.is_dir()
            && dest
                .read_dir()
                .map(|mut d| d.next().is_none())
                .unwrap_or(false);
        if !empty {
            return Err(format!(
                "{} already exists and is not empty",
                dest.display()
            ));
        }
    }
    for (rel, bytes) in &draft.files {
        let p = dest.join(rel);
        if let Some(dir) = p.parent() {
            std::fs::create_dir_all(dir)
                .map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
        }
        std::fs::write(&p, bytes).map_err(|e| format!("cannot write {rel}: {e}"))?;
    }
    Ok(dest)
}

/// Write the draft for `name` into the user library. See [`scaffold_into`].
pub fn scaffold(name: &str, from: Option<&str>) -> Result<PathBuf, String> {
    scaffold_into(&library_dir(), name, from)
}

/// A package judged on its files: the manifest when the package check
/// passed, every check and scan error, and the scan warnings.
#[derive(Debug)]
pub struct Validation {
    pub reference: String,
    pub path: String,
    pub manifest: Option<RuntimeManifest>,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

impl Validation {
    /// No errors: the package check passed and the scan is silent.
    pub fn valid(&self) -> bool {
        self.errors.is_empty()
    }
}

/// Check the package files under `reference`: the manifest, the entry,
/// the vendored files and licences, a sample per required role, then the
/// static scan (errors refuse, warnings are carried).
pub fn validate_files(reference: &str, files: &BTreeMap<String, Vec<u8>>) -> Validation {
    let path = format!("{}/{reference}", runtimes::RUNTIMES_DIR);
    if !runtimes::valid_ref(reference) {
        return Validation {
            reference: reference.to_string(),
            path,
            manifest: None,
            errors: vec![format!(
                "runtime `{reference}` is not <name>@<major> (or names a reserved runtime)"
            )],
            warnings: Vec::new(),
        };
    }
    match runtimes::check_package(reference, files) {
        Ok(m) => {
            let vendored: BTreeSet<String> = m.vendored_files();
            let report = runtimes::scan::scan(files, &vendored, m.capabilities.wasm);
            let line = |f: &runtimes::scan::Finding| {
                format!("{}:{}: [{}] {}", f.file, f.line, f.rule, f.message)
            };
            Validation {
                reference: reference.to_string(),
                path,
                manifest: Some(m),
                errors: report.errors.iter().map(line).collect(),
                warnings: report.warnings.iter().map(line).collect(),
            }
        }
        Err(e) => Validation {
            reference: reference.to_string(),
            path,
            manifest: None,
            errors: vec![e],
            warnings: Vec::new(),
        },
    }
}

const MAX_PACKAGE_FILES: usize = 4096;
const MAX_PACKAGE_BYTES: u64 = 64 * 1024 * 1024;

/// Read a package folder into the files map the checks take: regular
/// files only, no symlinks, the walk capped like an installed package.
fn read_package(dir: &Path) -> Result<BTreeMap<String, Vec<u8>>, String> {
    let mut files = BTreeMap::new();
    let mut total: u64 = 0;
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let entries =
            std::fs::read_dir(&d).map_err(|e| format!("cannot read {}: {e}", d.display()))?;
        for e in entries {
            let p = e
                .map_err(|e| format!("cannot read {}: {e}", d.display()))?
                .path();
            let meta = std::fs::symlink_metadata(&p)
                .map_err(|e| format!("cannot stat {}: {e}", p.display()))?;
            if meta.is_symlink() {
                return Err(format!("{} is a link: packages carry copies", p.display()));
            }
            if meta.is_dir() {
                stack.push(p);
            } else if meta.is_file() {
                let rel = p
                    .strip_prefix(dir)
                    .map_err(|_| format!("{} left its folder", p.display()))?
                    .to_string_lossy()
                    .replace('\\', "/");
                total += meta.len();
                if total > MAX_PACKAGE_BYTES {
                    return Err("the package is larger than 64 MiB".to_string());
                }
                files.insert(
                    rel,
                    std::fs::read(&p).map_err(|e| format!("cannot read {}: {e}", p.display()))?,
                );
                if files.len() > MAX_PACKAGE_FILES {
                    return Err("the package holds more than 4096 files".to_string());
                }
            }
        }
    }
    Ok(files)
}

/// Validate the package folder `dir` as `reference` (never approves).
pub fn validate_dir(reference: &str, dir: &Path) -> Result<Validation, String> {
    if !runtimes::valid_ref(reference) {
        return Ok(validate_files(reference, &BTreeMap::new()));
    }
    if !dir.is_dir() {
        return Err(format!(
            "no package at {}: nothing to validate",
            dir.display()
        ));
    }
    let files = read_package(dir)?;
    let mut v = validate_files(reference, &files);
    v.path = dir.to_string_lossy().to_string();
    Ok(v)
}

/// Validate the user library's copy of `reference` (never approves).
pub fn validate_library(reference: &str) -> Result<Validation, String> {
    let dir = library_dir().join(reference);
    if !dir.is_dir() {
        return Err(format!(
            "runtime {reference}: not installed in the library (no {LIBRARY_DIR}/{reference}/)"
        ));
    }
    validate_dir(reference, &dir)
}

/// Validate the project's installed copy of `reference` (never approves).
pub fn validate_project(cx: &Core, root_id: &str, reference: &str) -> Result<Validation, String> {
    let root = crate::fs::session_root(cx, root_id)?;
    if !runtimes::valid_ref(reference) {
        return Ok(validate_files(reference, &BTreeMap::new()));
    }
    let dir = root.join(runtimes::RUNTIMES_DIR).join(reference);
    if !dir.is_dir() {
        return Err(crate::widget_approval::not_installed(reference));
    }
    validate_dir(reference, &dir)
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

    fn sample_dir(reference: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../docs/runtimes/samples")
            .join(reference)
    }

    #[test]
    fn a_fresh_draft_passes_the_package_check_with_a_silent_scan() {
        let base = scratch("author-plain");
        let dir = scaffold_into(&base, "orbit-view", None).unwrap();
        assert_eq!(dir, base.join("orbit-view@1"));
        let files = read_package(&dir).unwrap();
        assert_eq!(
            files.keys().cloned().collect::<Vec<_>>(),
            [
                "LICENSE",
                "bridge.js",
                "index.html",
                "runtime.json",
                "samples/photo.svg"
            ]
        );
        let m = runtimes::check_package("orbit-view@1", &files).unwrap();
        assert_eq!(m.name, "orbit-view");
        assert_eq!(m.title, "orbit-view");
        let report = runtimes::scan::scan(&files, &m.vendored_files(), m.capabilities.wasm);
        let shown = |fs: &[runtimes::scan::Finding]| {
            fs.iter()
                .map(|f| format!("{}:{}: {}", f.file, f.line, f.message))
                .collect::<Vec<_>>()
        };
        assert!(report.errors.is_empty(), "{:?}", shown(&report.errors));
        assert!(report.warnings.is_empty(), "{:?}", shown(&report.warnings));
        let v = validate_dir("orbit-view@1", &dir).unwrap();
        assert!(v.valid());
        assert!(v.manifest.is_some() && v.errors.is_empty() && v.warnings.is_empty());
    }

    #[test]
    fn scaffold_names_follow_the_name_rules() {
        let base = scratch("author-names");
        for bad in [
            "Model",
            "m-x",
            "maleficium-x",
            "x",
            "x@1",
            "has space",
            "UPPER",
        ] {
            assert!(scaffold_into(&base, bad, None).is_err(), "{bad}");
        }
        for bad in ["nope@0", "chart@1"] {
            assert!(
                scaffold_into(&base, "fine-name", Some(bad)).is_err(),
                "{bad}"
            );
        }
        assert!(scaffold_into(&base, "fine-name", Some(MODEL_REF)).is_ok());
    }

    #[test]
    fn scaffold_refuses_a_non_empty_folder() {
        let base = scratch("author-exists");
        scaffold_into(&base, "taken", None).unwrap();
        let again = scaffold_into(&base, "taken", None).unwrap_err();
        assert!(again.contains("not empty"), "{again}");
    }

    #[test]
    fn a_model_fork_copies_the_built_entry_and_its_sources() {
        let base = scratch("author-model");
        let dir = scaffold_into(&base, "spin-view", Some(MODEL_REF)).unwrap();
        let files = read_package(&dir).unwrap();
        for rel in [
            "runtime.json",
            "index.html",
            "bridge.js",
            "src/main.ts",
            "src/core.ts",
            "src/index.html",
            "samples/mesh.glb",
            "LICENSE",
        ] {
            assert!(files.contains_key(rel), "{rel}");
        }
        let entry = files["index.html"].clone();
        assert!(entry.starts_with(b"<!doctype html>"));
        assert!(String::from_utf8(files["src/main.ts"].clone())
            .unwrap()
            .contains("startBridge"));
        let text = String::from_utf8(files["runtime.json"].clone()).unwrap();
        let m = runtimes::parse_manifest("spin-view@1", &text).unwrap();
        assert_eq!(m.name, "spin-view");
        assert!(m.capabilities.webgl);
        assert!(m.primary_role() == "model" && m.sources["model"].required);
        runtimes::check_package("spin-view@1", &files).unwrap();
    }

    #[test]
    fn validate_reads_a_project_copy_and_flags_a_bad_ref() {
        let cx = &Core::default();
        let dir = scratch("author-project");
        let root = dunce::canonicalize(&dir).unwrap();
        crate::grant_root(cx, "author", &root.to_string_lossy()).unwrap();
        scaffold_into(&dir.join("runtimes"), "orbit-view", None).unwrap();
        let v = validate_project(cx, "author", "orbit-view@1").unwrap();
        assert!(v.valid(), "{:?}", v.errors);
        assert!(v.path.ends_with("orbit-view@1"));
        let missing = validate_project(cx, "author", "nope@1").unwrap_err();
        assert!(missing.contains("not installed"), "{missing}");
        let bad = validate_project(cx, "author", "Model@1").unwrap();
        assert!(!bad.valid() && bad.manifest.is_none());
    }

    #[test]
    fn validate_reports_scan_errors_and_never_approves() {
        let v = validate_dir("bad-cdn@1", &sample_dir("bad-cdn@1")).unwrap();
        assert!(!v.valid());
        assert!(
            v.manifest.is_some(),
            "the package check passes; the scan refuses"
        );
        assert!(
            v.errors.iter().any(|e| e.contains("url-load")),
            "{:?}",
            v.errors
        );
        let v = validate_dir("caption-overlay@1", &sample_dir("caption-overlay@1")).unwrap();
        assert!(v.valid(), "{:?}", v.errors);
        let missing = validate_dir("gone@1", &sample_dir("gone@1")).unwrap_err();
        assert!(missing.contains("nothing to validate"), "{missing}");
        let src = include_str!("runtime_author.rs");
        let head = src.split("#[cfg(test)]").next().unwrap_or(src);
        for f in ["approve(", "revoke(", "decide_runtime(", "review_runtime("] {
            assert!(!head.contains(f), "validate must never call {f}");
        }
    }
}
