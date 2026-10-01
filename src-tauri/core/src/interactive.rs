//! The interactive-widget package: the embedded `maleficium-interactive.sty`
//! bytes, and the explicit install that materializes them into a project.
//! Nothing writes into a project without the user's action; the compile
//! itself only reads the file, and the sidecar lands in the app-cache
//! outdir, never beside the sources.

use crate::Core;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

include!(concat!(env!("OUT_DIR"), "/interactive.rs"));

/// The package file name inside a project.
pub const PACKAGE_NAME: &str = "maleficium-interactive.sty";

/// The embedded package bytes.
pub fn package_bytes() -> &'static [u8] {
    INTERACTIVE_STY
}

/// Where the install wrote the package, root-relative.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct Installed {
    pub file: String,
}

/// Write the embedded package into the session root. Explicit user action:
/// the only writer of project sources on this path.
pub fn install(cx: &Core, root_id: &str) -> Result<Installed, String> {
    let root = super::fs::session_root(cx, root_id)?;
    std::fs::write(root.join(PACKAGE_NAME), INTERACTIVE_STY)
        .map_err(|e| format!("cannot write {PACKAGE_NAME}: {e}"))?;
    Ok(Installed {
        file: PACKAGE_NAME.to_string(),
    })
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

    #[test]
    fn install_writes_the_file_into_the_granted_root() {
        let cx = &Core::default();
        let dir = crate::test_scratch::dir("interactive-install");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let canon = dunce::canonicalize(&dir).unwrap();
        super::super::fs::grant_root(cx, "interactive-proj", &canon.to_string_lossy()).unwrap();
        let out = install(cx, "interactive-proj").unwrap();
        assert_eq!(out.file, PACKAGE_NAME);
        assert_eq!(
            std::fs::read(canon.join(PACKAGE_NAME)).unwrap(),
            package_bytes()
        );
        assert!(install(cx, "no-such-root").is_err());
    }
}
