//! The interactive-widget package: the embedded `maleficium-interactive.sty`
//! bytes, and the explicit install that materializes them into a project.
//! Nothing writes into a project without the user's action; the compile
//! itself only reads the file, and the sidecar lands in the app-cache
//! outdir, never beside the sources.

use crate::Core;

pub use maleficium_events::InstallOutcome;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

include!(concat!(env!("OUT_DIR"), "/interactive.rs"));

/// The package file name inside a project.
pub const PACKAGE_NAME: &str = "maleficium-interactive.sty";

/// The embedded package bytes.
pub fn package_bytes() -> &'static [u8] {
    INTERACTIVE_STY
}

/// What an install did, and the file it concerns.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct InstallResult {
    pub file: String,
    pub outcome: InstallOutcome,
}

/// Write the embedded package into the session root. Explicit user action:
/// the only writer of project sources on this path. A modified existing
/// copy is replaced only when `overwrite` is set (the user confirmed in the
/// app), a copy already identical is left alone, and a link at the
/// destination is refused rather than written through. The MCP tool has no
/// way to pass `overwrite`.
pub fn install_checked(cx: &Core, root_id: &str, overwrite: bool) -> Result<InstallResult, String> {
    let root = super::fs::session_root(cx, root_id)?;
    let dest = root.join(PACKAGE_NAME);
    let result = |outcome| InstallResult {
        file: PACKAGE_NAME.to_string(),
        outcome,
    };
    match std::fs::symlink_metadata(&dest) {
        Ok(m) if m.file_type().is_symlink() || !m.is_file() => {
            return Err(format!("{PACKAGE_NAME} exists and is not a regular file"));
        }
        Ok(_) => {
            let have =
                std::fs::read(&dest).map_err(|e| format!("cannot read {PACKAGE_NAME}: {e}"))?;
            if have == INTERACTIVE_STY {
                return Ok(result(InstallOutcome::AlreadyCurrent));
            }
            if !overwrite {
                return Ok(result(InstallOutcome::NeedsConfirmation));
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(format!("cannot read {PACKAGE_NAME}: {e}")),
    }
    std::fs::write(&dest, INTERACTIVE_STY)
        .map_err(|e| format!("cannot write {PACKAGE_NAME}: {e}"))?;
    Ok(result(InstallOutcome::Installed))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_embedded_package_is_the_shipped_source() {
        let bytes = package_bytes();
        assert!(!bytes.is_empty());
        let text = std::str::from_utf8(bytes).unwrap();
        for macro_name in [
            "\\interactivemodel",
            "\\interactivevideo",
            "\\interactivetable",
            "\\interactivechart",
            "\\interactive",
            "\\maleficiumtheme",
        ] {
            assert!(text.contains(macro_name), "{macro_name} missing");
        }
    }

    fn granted(name: &str) -> (Core, std::path::PathBuf) {
        let cx = Core::default();
        let dir = crate::test_scratch::dir(name);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let canon = dunce::canonicalize(&dir).unwrap();
        super::super::fs::grant_root(&cx, "p", &canon.to_string_lossy()).unwrap();
        (cx, canon)
    }

    #[test]
    fn checked_install_writes_when_missing_and_is_idempotent() {
        let (cx, dir) = granted("interactive-checked-new");
        let r = install_checked(&cx, "p", false).unwrap();
        assert_eq!(r.outcome, InstallOutcome::Installed);
        assert_eq!(
            std::fs::read(dir.join(PACKAGE_NAME)).unwrap(),
            package_bytes()
        );
        let again = install_checked(&cx, "p", false).unwrap();
        assert_eq!(again.outcome, InstallOutcome::AlreadyCurrent);
        assert!(install_checked(&cx, "no-such-root", false).is_err());
    }

    #[test]
    fn checked_install_keeps_a_modified_copy_until_confirmed() {
        let (cx, dir) = granted("interactive-checked-modified");
        let path = dir.join(PACKAGE_NAME);
        std::fs::write(&path, b"% my edits\n").unwrap();
        let r = install_checked(&cx, "p", false).unwrap();
        assert_eq!(r.outcome, InstallOutcome::NeedsConfirmation);
        assert_eq!(std::fs::read(&path).unwrap(), b"% my edits\n");
        let r = install_checked(&cx, "p", true).unwrap();
        assert_eq!(r.outcome, InstallOutcome::Installed);
        assert_eq!(std::fs::read(&path).unwrap(), package_bytes());
    }

    #[cfg(unix)]
    #[test]
    fn checked_install_refuses_to_write_through_a_link() {
        let (cx, dir) = granted("interactive-checked-link");
        let outside = dir.join("outside.txt");
        std::fs::write(&outside, b"keep").unwrap();
        std::os::unix::fs::symlink(&outside, dir.join(PACKAGE_NAME)).unwrap();
        assert!(install_checked(&cx, "p", true).is_err());
        assert_eq!(std::fs::read(&outside).unwrap(), b"keep");
    }
}
