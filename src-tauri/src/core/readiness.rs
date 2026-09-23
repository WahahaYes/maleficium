//! Per-project offline readiness: a record in app data (never in the
//! project), rewritten after each compile that shows something about the
//! project's dependencies, and read back as the state the badge shows.

use std::path::{Path, PathBuf};

use maleficium_events::{OfflineReadiness, OfflineState};
use maleficium_structure::{ExternalNeeds, MissingDependency, MissingReason};
use serde::{Deserialize, Serialize};

use super::engine::{DigestCheck, BUNDLE_DIGEST};
use super::JobStatus;

/// What the project's compiles have shown about its dependencies.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Record {
    /// The pinned digest the record was made against.
    pub digest: String,
    /// Root-relative main file of the last compile.
    pub main: String,
    /// The last compile succeeded from the cache alone.
    pub offline_ok: bool,
    /// Content revision of the document the last offline success compiled.
    pub revision: Option<String>,
    /// The dependency the last compile lacked.
    pub missing: Option<MissingDependency>,
    /// Programs the engine ran.
    pub tools: Vec<String>,
    /// System files (fonts) the engine read by absolute path.
    pub files: Vec<String>,
}

/// Where one project root's record lives.
pub fn record_path(root: &Path) -> PathBuf {
    super::data_base_dir()
        .join("maleficium-readiness")
        .join(format!(
            "{}.json",
            super::hash_root(&root.to_string_lossy())
        ))
}

pub fn load(root: &Path) -> Option<Record> {
    serde_json::from_str(&std::fs::read_to_string(record_path(root)).ok()?).ok()
}

pub fn store(root: &Path, r: &Record) -> Result<(), String> {
    let path = record_path(root);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("readiness dir unreachable: {}", e))?;
    }
    let json = serde_json::to_string_pretty(r).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| format!("readiness record unwritable: {}", e))
}

/// One finished compile, as the record sees it.
pub struct Outcome<'a> {
    pub status: &'a JobStatus,
    pub missing: &'a Option<MissingDependency>,
    pub cached_only: bool,
    pub needs: ExternalNeeds,
    pub main: &'a str,
    pub revision: Option<String>,
}

/// The record after one compile; `None` keeps the previous one (a document
/// error, a timeout or a cancel says nothing about dependencies).
pub fn next(prev: Option<Record>, o: Outcome) -> Option<Record> {
    let prev = prev.unwrap_or_default();
    match (o.status, o.missing) {
        (JobStatus::Success, missing) => {
            let offline_ok = o.cached_only && missing.is_none();
            Some(Record {
                digest: BUNDLE_DIGEST.into(),
                main: o.main.into(),
                offline_ok,
                revision: if offline_ok {
                    o.revision
                } else {
                    prev.revision
                },
                missing: missing.clone(),
                tools: o.needs.tools,
                files: o.needs.files,
            })
        }
        (JobStatus::Failed, Some(m)) => {
            let mut tools = prev.tools;
            if m.reason == MissingReason::ExternalTool {
                if let Some(t) = m.file.as_ref().filter(|t| !tools.contains(t)) {
                    tools.push(t.clone());
                }
            }
            Some(Record {
                digest: BUNDLE_DIGEST.into(),
                main: o.main.into(),
                offline_ok: false,
                revision: prev.revision,
                missing: Some(m.clone()),
                tools,
                files: prev.files,
            })
        }
        _ => None,
    }
}

/// The badge state of a record against the cache and this machine.
pub fn assess(
    record: Option<&Record>,
    cache: &DigestCheck,
    on_path: &dyn Fn(&str) -> bool,
    exists: &dyn Fn(&str) -> bool,
) -> OfflineReadiness {
    let view = |state, needs: Vec<String>, missing: Option<MissingDependency>| OfflineReadiness {
        state,
        needs,
        missing,
    };
    let Some(r) = record else {
        return view(OfflineState::Unverified, Vec::new(), None);
    };
    let missing = r.missing.clone();
    let changed = matches!(cache, DigestCheck::Changed(_))
        || r.digest != BUNDLE_DIGEST
        || missing
            .as_ref()
            .is_some_and(|m| m.reason == MissingReason::BundleChanged);
    if changed {
        return view(
            OfflineState::Unverified,
            vec![String::from("TeX bundle changed")],
            missing,
        );
    }
    let support = || vec![String::from("TeX support files")];
    if *cache == DigestCheck::Unresolved {
        return view(OfflineState::NeedsNetwork, support(), missing);
    }
    let named = |m: &MissingDependency, or: &str| vec![m.file.clone().unwrap_or_else(|| or.into())];
    if let Some(m) = &missing {
        match m.reason {
            MissingReason::NotCached | MissingReason::FetchFailed => {
                return view(
                    OfflineState::NeedsNetwork,
                    named(m, "TeX support files"),
                    missing,
                )
            }
            MissingReason::CacheEmpty | MissingReason::BundleUnreachable => {
                return view(OfflineState::NeedsNetwork, support(), missing)
            }
            MissingReason::SystemFont => {
                return view(OfflineState::NeedsFont, named(m, "a font"), missing)
            }
            MissingReason::NotInBundle
            | MissingReason::ShellEscapeRequired
            | MissingReason::BundleInvalid => {
                return view(OfflineState::Blocked, named(m, "TeX bundle"), missing)
            }
            MissingReason::ExternalTool | MissingReason::BundleChanged => {}
        }
    }
    let tools: Vec<String> = r.tools.iter().filter(|t| !on_path(t)).cloned().collect();
    if !tools.is_empty() {
        return view(OfflineState::NeedsTool, tools, missing);
    }
    let fonts: Vec<String> = r
        .files
        .iter()
        .filter(|f| !exists(f))
        .map(|f| {
            Path::new(f)
                .file_name()
                .map_or_else(|| f.clone(), |n| n.to_string_lossy().to_string())
        })
        .collect();
    if !fonts.is_empty() {
        return view(OfflineState::NeedsFont, fonts, missing);
    }
    // A tool reported missing that is now installed: the next compile decides.
    let state = if r.offline_ok && missing.is_none() {
        OfflineState::Ready
    } else {
        OfflineState::Unverified
    };
    view(state, Vec::new(), missing)
}

/// Whether an executable of this name is on `PATH`.
pub fn on_path(name: &str) -> bool {
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path).any(|dir| {
        let p = dir.join(name);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::metadata(&p).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        }
        #[cfg(not(unix))]
        {
            p.is_file() || p.with_extension("exe").is_file()
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn outcome<'a>(
        status: &'a JobStatus,
        missing: &'a Option<MissingDependency>,
        cached_only: bool,
    ) -> Outcome<'a> {
        Outcome {
            status,
            missing,
            cached_only,
            needs: ExternalNeeds::default(),
            main: "main.tex",
            revision: Some("r1".into()),
        }
    }

    fn miss(file: Option<&str>, reason: MissingReason) -> Option<MissingDependency> {
        Some(MissingDependency {
            file: file.map(str::to_string),
            reason,
        })
    }

    fn state(r: Option<&Record>, cache: DigestCheck) -> (OfflineState, Vec<String>) {
        let v = assess(r, &cache, &|t| t == "sh", &|f| f.starts_with("/present"));
        (v.state, v.needs)
    }

    #[test]
    fn only_a_cached_only_success_is_offline_ok() {
        let ok = next(None, outcome(&JobStatus::Success, &None, true)).unwrap();
        assert!(ok.offline_ok);
        assert_eq!(ok.revision.as_deref(), Some("r1"));
        assert_eq!(state(Some(&ok), DigestCheck::Pinned).0, OfflineState::Ready);

        let online = next(Some(ok.clone()), outcome(&JobStatus::Success, &None, false)).unwrap();
        assert!(!online.offline_ok);
        assert_eq!(online.revision.as_deref(), Some("r1"));
        assert_eq!(
            state(Some(&online), DigestCheck::Pinned).0,
            OfflineState::Unverified
        );
    }

    #[test]
    fn document_errors_cancels_and_timeouts_keep_the_record() {
        for s in [JobStatus::Failed, JobStatus::Cancelled, JobStatus::TimedOut] {
            assert_eq!(next(None, outcome(&s, &None, true)), None);
        }
    }

    #[test]
    fn missing_dependencies_map_to_what_would_fix_them() {
        let cases = [
            (
                miss(Some("booktabs.sty"), MissingReason::NotCached),
                OfflineState::NeedsNetwork,
                "booktabs.sty",
            ),
            (
                miss(Some("x.sty"), MissingReason::FetchFailed),
                OfflineState::NeedsNetwork,
                "x.sty",
            ),
            (
                miss(None, MissingReason::CacheEmpty),
                OfflineState::NeedsNetwork,
                "TeX support files",
            ),
            (
                miss(Some("biber"), MissingReason::ExternalTool),
                OfflineState::NeedsTool,
                "biber",
            ),
            (
                miss(Some("NoSuch"), MissingReason::SystemFont),
                OfflineState::NeedsFont,
                "NoSuch",
            ),
            (
                miss(Some("nope.sty"), MissingReason::NotInBundle),
                OfflineState::Blocked,
                "nope.sty",
            ),
            (
                miss(Some("minted"), MissingReason::ShellEscapeRequired),
                OfflineState::Blocked,
                "minted",
            ),
        ];
        for (m, want, need) in cases {
            let r = next(None, outcome(&JobStatus::Failed, &m, true)).unwrap();
            assert_eq!(
                state(Some(&r), DigestCheck::Pinned),
                (want, vec![need.to_string()]),
                "{m:?}"
            );
        }
    }

    #[test]
    fn a_changed_bundle_distrusts_the_record() {
        let ok = next(None, outcome(&JobStatus::Success, &None, true)).unwrap();
        let (s, needs) = state(Some(&ok), DigestCheck::Changed("0".repeat(64)));
        assert_eq!(s, OfflineState::Unverified);
        assert_eq!(needs, vec!["TeX bundle changed".to_string()]);
        let drift = miss(None, MissingReason::BundleChanged);
        let r = next(None, outcome(&JobStatus::Success, &drift, true)).unwrap();
        assert_eq!(
            state(Some(&r), DigestCheck::Pinned).0,
            OfflineState::Unverified
        );
        let old = Record {
            digest: "0".repeat(64),
            ..ok
        };
        assert_eq!(
            state(Some(&old), DigestCheck::Pinned).0,
            OfflineState::Unverified
        );
    }

    #[test]
    fn a_cleared_cache_needs_network() {
        let ok = next(None, outcome(&JobStatus::Success, &None, true)).unwrap();
        assert_eq!(
            state(Some(&ok), DigestCheck::Unresolved),
            (
                OfflineState::NeedsNetwork,
                vec!["TeX support files".to_string()]
            )
        );
    }

    #[test]
    fn absent_tools_and_fonts_block_readiness() {
        let mut o = outcome(&JobStatus::Success, &None, true);
        o.needs = ExternalNeeds {
            tools: vec!["sh".into(), "biber".into()],
            files: vec!["/present/a.ttf".into()],
        };
        let r = next(None, o).unwrap();
        assert_eq!(
            state(Some(&r), DigestCheck::Pinned),
            (OfflineState::NeedsTool, vec!["biber".to_string()])
        );
        let r = Record {
            tools: vec!["sh".into()],
            files: vec!["/gone/DejaVuSans.ttf".into()],
            ..r
        };
        assert_eq!(
            state(Some(&r), DigestCheck::Pinned),
            (OfflineState::NeedsFont, vec!["DejaVuSans.ttf".to_string()])
        );
        let r = Record {
            files: vec!["/present/a.ttf".into()],
            ..r
        };
        assert_eq!(state(Some(&r), DigestCheck::Pinned).0, OfflineState::Ready);
    }

    #[test]
    fn no_record_is_unverified() {
        assert_eq!(
            state(None, DigestCheck::Pinned),
            (OfflineState::Unverified, vec![])
        );
    }

    #[test]
    fn records_live_in_app_data_keyed_by_root() {
        let p = record_path(Path::new("/home/u/paper"));
        assert!(p.starts_with(super::super::data_base_dir()));
        assert_ne!(p, record_path(Path::new("/home/u/other")));
        assert!(!p.starts_with("/home/u/paper"));
    }

    #[test]
    fn on_path_finds_executables_only() {
        assert!(on_path("sh"));
        assert!(!on_path("no-such-tool-maleficium"));
    }
}
