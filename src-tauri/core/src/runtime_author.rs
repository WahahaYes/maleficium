//! Authoring drafts for custom widget runtimes: scaffold a new package
//! into the user library and validate a package wherever it sits.
//!
//! The scaffold writes a caption-overlay-shaped draft (its files are the
//! documented sample, embedded): `runtime.json`, a classic `index.html`
//! that loads the built `bridge.js`, `samples/` for the required role,
//! and `LICENSE`. `--from model@1` starts from the built-in model viewer
//! instead: the viewer as a readable classic `viewer.js` over a vendored
//! three.js, so the draft passes the scan untouched. Validation
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
    include_str!("../../../embed-runtime/samples/caption-overlay@1/runtime.json");
const DRAFT_ENTRY: &str =
    include_str!("../../../embed-runtime/samples/caption-overlay@1/index.html");
const DRAFT_SAMPLE: &[u8] =
    include_bytes!("../../../embed-runtime/samples/caption-overlay@1/samples/photo.svg");
const DRAFT_LICENSE: &str =
    include_str!("../../../embed-runtime/samples/caption-overlay@1/LICENSE");
const BRIDGE_JS: &[u8] = include_bytes!("../../../embed-runtime/built/bridge/bridge.js");

// The fork (`npm run build:runtimes` writes model-fork/; fresh.test.ts
// keeps it current).
const FORK_ENTRY: &[u8] = include_bytes!("../../../embed-runtime/src/model/fork.html");
const FORK_VIEWER: &[u8] = include_bytes!("../../../embed-runtime/built/model-fork/viewer.js");
const FORK_THREE: &[u8] =
    include_bytes!("../../../embed-runtime/built/model-fork/vendor/three/three.js");
const FORK_THREE_LICENSE: &[u8] =
    include_bytes!("../../../embed-runtime/built/model-fork/vendor/three/LICENSE");
/// Where the vendored three.js version comes from: the app's own pin.
const PACKAGE_JSON: &str = include_str!("../../../package.json");
const MODEL_SAMPLE: &[u8] = include_bytes!("../../../embed-runtime/src/model/fork-sample.glb");

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
        "license": "MIT-0",
        "sources": {
            "model": {
                "primary": true,
                "required": true,
                "extensions": ["glb"],
                "description": "The model to show."
            }
        },
        "capabilities": {"webgl": true, "wasm": false},
        "vendored": [{
            "name": "three",
            "version": three_version(),
            "license": "MIT",
            "source": "https://github.com/mrdoob/three.js",
            "files": ["vendor/three/three.js"],
            "licenseFile": "vendor/three/LICENSE"
        }]
    });
    let mut text = serde_json::to_string_pretty(&v).expect("a literal manifest serializes");
    text.push('\n');
    text.into_bytes()
}

/// The three.js version the app pins (`"three": "X.Y.Z"` in package.json).
fn three_version() -> String {
    let v: serde_json::Value = serde_json::from_str(PACKAGE_JSON).expect("package.json parses");
    v["devDependencies"]["three"]
        .as_str()
        .expect("package.json pins three")
        .to_string()
}

/// The `model@1` fork: the viewer as a classic script the author edits in
/// place, three.js vendored with its licence, a glb sample for the
/// required role.
fn model_files(name: &str) -> BTreeMap<String, Vec<u8>> {
    BTreeMap::from([
        ("runtime.json".to_string(), model_manifest(name)),
        ("index.html".to_string(), FORK_ENTRY.to_vec()),
        ("bridge.js".to_string(), BRIDGE_JS.to_vec()),
        ("viewer.js".to_string(), FORK_VIEWER.to_vec()),
        ("vendor/three/three.js".to_string(), FORK_THREE.to_vec()),
        (
            "vendor/three/LICENSE".to_string(),
            FORK_THREE_LICENSE.to_vec(),
        ),
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

/// What an install did to the project's `runtimes/<ref>/`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallOutcome {
    /// Copied into an empty place.
    Installed,
    /// A different copy was there, and `replace` swapped it out.
    Replaced,
    /// The project already held exactly these files: nothing written.
    Unchanged,
}

impl InstallOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Installed => "installed",
            Self::Replaced => "replaced",
            Self::Unchanged => "unchanged",
        }
    }
}

/// An install's result: the project-relative folder and its file count.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Installed {
    pub reference: String,
    pub rel: String,
    pub files: usize,
    pub outcome: InstallOutcome,
}

/// Copy the user library's draft of `reference` into the project as
/// `runtimes/<ref>/`. Only a draft that validates (no errors) installs,
/// and a different copy already in the project is replaced only with
/// `replace`. Installing is copying reviewed files, not approving them:
/// the runtime runs only once the user allows its digest in the app.
pub fn install(
    cx: &Core,
    root_id: &str,
    reference: &str,
    replace: bool,
) -> Result<Installed, String> {
    install_from(cx, root_id, &library_dir(), reference, replace)
}

pub(crate) fn install_from(
    cx: &Core,
    root_id: &str,
    library: &Path,
    reference: &str,
    replace: bool,
) -> Result<Installed, String> {
    crate::fs::session_root(cx, root_id)?;
    if !runtimes::valid_ref(reference) {
        return Err(format!(
            "`{reference}` is not a runtime reference (<name>@<major>)"
        ));
    }
    let draft = library.join(reference);
    if !draft.is_dir() {
        return Err(format!(
            "runtime {reference}: not in the library (no {LIBRARY_DIR}/{reference}/); scaffold it first"
        ));
    }
    let files = read_package(&draft)?;
    let v = validate_files(reference, &files);
    if !v.valid() {
        return Err(format!(
            "runtime {reference} does not validate, so it is not installed: {}",
            v.errors.join("; ")
        ));
    }
    let rel = format!("{}/{reference}", runtimes::RUNTIMES_DIR);
    let root = crate::fs::session_root(cx, root_id)?;
    let existing = root.join(&rel);
    let mut outcome = InstallOutcome::Installed;
    if existing.exists() {
        if read_package(&existing).is_ok_and(|have| have == files) {
            return Ok(Installed {
                reference: reference.to_string(),
                rel,
                files: files.len(),
                outcome: InstallOutcome::Unchanged,
            });
        }
        if !replace {
            return Err(format!(
                "{rel}/ already holds a different copy of {reference}; pass replace to swap it for the library's"
            ));
        }
        outcome = InstallOutcome::Replaced;
    }
    // Written beside the target, then swapped in, so the project never
    // holds a half-copied runtime.
    let staging = format!("{}/.{reference}.installing", runtimes::RUNTIMES_DIR);
    let write = || -> Result<(), String> {
        crate::fs::make_dir(cx, root_id, runtimes::RUNTIMES_DIR)?;
        if root.join(&staging).exists() {
            crate::fs::remove_path(cx, root_id, &staging, true)?;
        }
        crate::fs::make_dir(cx, root_id, &staging)?;
        let mut made: BTreeSet<String> = BTreeSet::new();
        for (path, bytes) in &files {
            // Each folder level in turn: a confined mkdir needs its parent.
            let mut dir = staging.clone();
            for part in path
                .split('/')
                .rev()
                .skip(1)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
            {
                dir = format!("{dir}/{part}");
                if made.insert(dir.clone()) {
                    crate::fs::make_dir(cx, root_id, &dir)?;
                }
            }
            crate::fs::write_bytes(cx, root_id, &format!("{staging}/{path}"), bytes)?;
        }
        if outcome == InstallOutcome::Replaced {
            crate::fs::remove_path(cx, root_id, &rel, true)?;
        }
        crate::fs::rename_path(cx, root_id, &staging, &rel)
    };
    if let Err(e) = write() {
        let _ = crate::fs::remove_path(cx, root_id, &staging, true);
        return Err(format!("installing {reference} failed: {e}"));
    }
    Ok(Installed {
        reference: reference.to_string(),
        rel,
        files: files.len(),
        outcome,
    })
}

/// What an agent does after an install, from the runtime's approval state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AfterInstall {
    /// It runs on the next compile.
    pub approved: bool,
    /// The project's auto-approval setting.
    pub auto_approve: bool,
    /// The user has to allow it: the agent asks them to.
    pub ask_user: bool,
    /// What to do or relay next.
    pub hint: String,
}

/// The agent's next step once `reference` is installed: nothing when the
/// user allowed these exact files or auto-approval covers the change, else
/// ask the user. Auto-approval covers only an update to a runtime the user
/// already allowed (same licence and vendored libraries), never a first
/// install or a denied runtime, so the setting alone does not decide.
pub fn after_install(
    reference: &str,
    status: &crate::widget_approval::WidgetApprovalStatus,
) -> AfterInstall {
    use crate::widget_approval::{ApprovedVia, WidgetApprovalStatus};
    use maleficium_events::WidgetApprovalCause as Cause;
    let r = reference;
    match status {
        WidgetApprovalStatus::Approved(a) => AfterInstall {
            approved: true,
            auto_approve: a.auto_approve,
            ask_user: false,
            hint: match a.via {
                ApprovedVia::Auto => format!(
                    "The user allowed {r} before and auto-approval is on, so this update runs on the next compile. Nothing to ask the user."
                ),
                ApprovedVia::User => format!(
                    "The user already allowed exactly these files of {r}: it runs on the next compile. Nothing to ask the user."
                ),
            },
        },
        WidgetApprovalStatus::ApprovalRequired(q) => {
            let why = match q.cause {
                Cause::NeverApproved if q.auto_approve => format!(
                    "{r} is new to this project; auto-approval is on but only covers updates to a runtime the user already allowed."
                ),
                Cause::NeverApproved => format!("{r} is new to this project."),
                Cause::Revoked => format!(
                    "The user denied {r} in this project; tell them it is installed, and leave it to them."
                ),
                Cause::LicenseOrVendoredChanged => format!(
                    "{r} changed its licence or vendored libraries since the user allowed it, which always needs the user, even with auto-approval on."
                ),
                Cause::ChangedSinceApproval | Cause::DeclaredOriginsChanged => format!(
                    "{r} changed since the user allowed it, and auto-approval is off for this project."
                ),
            };
            AfterInstall {
                approved: false,
                auto_approve: q.auto_approve,
                ask_user: true,
                hint: format!(
                    "Ask the user to allow {r}. {why} After the next compile, the banner above the article links to {}, where they review and allow it. Do not retry, poll, or rework the runtime to get around this.",
                    q.panel
                ),
            }
        }
    }
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
            .join("../../embed-runtime/samples")
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
    fn a_model_fork_is_a_readable_viewer_over_vendored_three_and_validates() {
        let base = scratch("author-model");
        let dir = scaffold_into(&base, "spin-view", Some(MODEL_REF)).unwrap();
        let files = read_package(&dir).unwrap();
        for rel in [
            "runtime.json",
            "index.html",
            "bridge.js",
            "viewer.js",
            "vendor/three/three.js",
            "vendor/three/LICENSE",
            "samples/mesh.glb",
            "LICENSE",
        ] {
            assert!(files.contains_key(rel), "{rel}");
        }
        let entry = files["index.html"].clone();
        assert!(entry.starts_with(b"<!doctype html>"));
        let viewer = String::from_utf8(files["viewer.js"].clone()).unwrap();
        assert!(viewer.contains("mfwBridge") && viewer.contains("THREE"));
        // Untouched, the fork is approvable: no scan errors.
        let v = validate_dir("spin-view@1", &dir).unwrap();
        assert!(v.valid(), "{:?}", v);
        let text = String::from_utf8(files["runtime.json"].clone()).unwrap();
        let m = runtimes::parse_manifest("spin-view@1", &text).unwrap();
        assert_eq!(m.name, "spin-view");
        assert!(m.capabilities.webgl);
        assert_eq!(m.vendored[0].version, three_version());
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
    fn install_copies_a_valid_draft_and_replaces_only_when_asked() {
        let cx = &Core::default();
        let library = scratch("install-library");
        let dir = scratch("install-project");
        let root = dunce::canonicalize(&dir).unwrap();
        crate::grant_root(cx, "inst", &root.to_string_lossy()).unwrap();
        scaffold_into(&library, "spin-view", Some(MODEL_REF)).unwrap();

        let first = install_from(cx, "inst", &library, "spin-view@1", false).unwrap();
        assert_eq!(first.outcome, InstallOutcome::Installed);
        assert_eq!(first.rel, "runtimes/spin-view@1");
        let installed = read_package(&root.join("runtimes/spin-view@1")).unwrap();
        assert_eq!(
            installed,
            read_package(&library.join("spin-view@1")).unwrap()
        );
        assert_eq!(first.files, installed.len());
        assert!(validate_project(cx, "inst", "spin-view@1").unwrap().valid());
        assert!(!root.join("runtimes/.spin-view@1.installing").exists());

        let again = install_from(cx, "inst", &library, "spin-view@1", false).unwrap();
        assert_eq!(again.outcome, InstallOutcome::Unchanged);

        // The author edits the draft: a different copy is in the project.
        let viewer = library.join("spin-view@1/viewer.js");
        let mut text = std::fs::read_to_string(&viewer).unwrap();
        text.push_str("\n// edited\n");
        std::fs::write(&viewer, &text).unwrap();
        let refused = install_from(cx, "inst", &library, "spin-view@1", false).unwrap_err();
        assert!(refused.contains("pass replace"), "{refused}");
        let swapped = install_from(cx, "inst", &library, "spin-view@1", true).unwrap();
        assert_eq!(swapped.outcome, InstallOutcome::Replaced);
        assert_eq!(
            std::fs::read_to_string(root.join("runtimes/spin-view@1/viewer.js")).unwrap(),
            text
        );
    }

    #[test]
    fn install_refuses_an_invalid_draft_a_missing_one_and_a_bad_ref() {
        let cx = &Core::default();
        let library = scratch("install-refuse-library");
        let dir = scratch("install-refuse-project");
        let root = dunce::canonicalize(&dir).unwrap();
        crate::grant_root(cx, "refuse", &root.to_string_lossy()).unwrap();
        let bad = library.join("bad-cdn@1");
        std::fs::create_dir_all(&bad).unwrap();
        for (rel, bytes) in read_package(&sample_dir("bad-cdn@1")).unwrap() {
            let p = bad.join(&rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, bytes).unwrap();
        }
        let e = install_from(cx, "refuse", &library, "bad-cdn@1", false).unwrap_err();
        assert!(
            e.contains("does not validate") && e.contains("url-load"),
            "{e}"
        );
        assert!(!root.join("runtimes/bad-cdn@1").exists());
        let e = install_from(cx, "refuse", &library, "nope@1", false).unwrap_err();
        assert!(e.contains("not in the library"), "{e}");
        let e = install_from(cx, "refuse", &library, "../x@1", false).unwrap_err();
        assert!(e.contains("not a runtime reference"), "{e}");
        assert!(install_from(cx, "ungranted", &library, "nope@1", false).is_err());
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
