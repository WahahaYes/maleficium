//! SyncTeX query arg-building against explicit session roots.
//!
//! The Tauri commands and the MCP tools share these builders: same CWD +
//! argv derivation, same output tail. Only the transport differs.

use std::path::PathBuf;

/// Resolved forward-sync invocation: run INSIDE `dir` with `pdf_name`.
pub struct ForwardArgs {
    pub dir: PathBuf,
    pub pdf_name: String,
    pub tex_abs: String,
    pub line: u32,
}

/// Derive forward-sync args: `pdf_rel` resolves inside the root (absolute
/// outdir path); `tex_rel` resolves inside the root (absolute visible
/// source — the gz stores absolute Input paths).
pub fn forward_args(
    root_id: &str,
    pdf_rel: &str,
    tex_rel: &str,
    line: u32,
) -> Result<ForwardArgs, String> {
    let pdf_canon = super::fs::resolve_in(root_id, pdf_rel)?;
    let tex_canon = super::fs::resolve_in(root_id, tex_rel)?;
    let (dir, name) = match (pdf_canon.parent(), pdf_canon.file_name()) {
        (Some(d), Some(n)) if !d.as_os_str().is_empty() => {
            (d.to_path_buf(), n.to_string_lossy().to_string())
        }
        _ => return Err("forbidden path (pdf has no parent)".to_string()),
    };
    Ok(ForwardArgs {
        dir,
        pdf_name: name,
        tex_abs: tex_canon.to_string_lossy().to_string(),
        line,
    })
}

/// Resolved inverse-sync invocation: run INSIDE `dir` with the bare name.
pub struct InverseArgs {
    pub dir: PathBuf,
    pub pdf_name: String,
}

/// Derive inverse-sync args: the outdir resolves inside the root; the pdf
/// name is interpolated into the `page:x:y:name` tag, so it must be bare.
pub fn inverse_args(root_id: &str, dir_rel: &str, pdf_name: &str) -> Result<InverseArgs, String> {
    let dir_canon = super::fs::resolve_in(root_id, dir_rel)?;
    let name = crate::commands::guard::require_bare_filename(pdf_name)?.to_string();
    Ok(InverseArgs {
        dir: dir_canon,
        pdf_name: name,
    })
}

fn run_sidecar(dir: &PathBuf, args: &[String]) -> Result<String, String> {
    let bin = super::sidecar_path_for("synctex")
        .ok_or_else(|| String::from("bundled synctex sidecar missing (src-tauri/binaries/)"))?;
    let output = std::process::Command::new(&bin)
        .current_dir(dir)
        .args(args)
        .output()
        .map_err(|e| format!("synctex sidecar failed: {}", e))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout
        .lines()
        .rev()
        .take(500)
        .collect::<Vec<_>>()
        .join("\n"))
}

/// Run a forward query through the bundled sidecar.
pub fn forward_query(
    root_id: &str,
    pdf_rel: &str,
    tex_rel: &str,
    line: u32,
) -> Result<String, String> {
    let a = forward_args(root_id, pdf_rel, tex_rel, line)?;
    run_sidecar(
        &a.dir,
        &[
            "view".to_string(),
            "-i".to_string(),
            format!("{}:1:{}", a.line, a.tex_abs),
            "-o".to_string(),
            a.pdf_name,
        ],
    )
}

/// Run an inverse query through the bundled sidecar.
pub fn inverse_query(
    root_id: &str,
    dir_rel: &str,
    pdf_name: &str,
    page: u32,
    x: f32,
    y: f32,
) -> Result<String, String> {
    let a = inverse_args(root_id, dir_rel, pdf_name)?;
    run_sidecar(
        &a.dir,
        &[
            "edit".to_string(),
            "-o".to_string(),
            format!("{}:{}:{}:{}", page, x, y, a.pdf_name),
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grant_tmp(name: &str) -> String {
        let dir =
            std::env::temp_dir().join(format!("maleficium-sync-{}-{}", std::process::id(), name));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let canon = dir.canonicalize().unwrap();
        let id = format!("sync-{}", name);
        crate::core::fs::grant_root(&id, &canon.to_string_lossy()).unwrap();
        id
    }

    #[test]
    fn forward_rejects_outside_root() {
        let id = grant_tmp("escape");
        assert!(forward_args(&id, "../outside.pdf", "a.tex", 1).is_err());
        assert!(forward_args(&id, "a.pdf", "/etc/hostname", 1).is_err());
    }

    #[test]
    fn inverse_rejects_non_bare_name() {
        let id = grant_tmp("bare");
        assert!(inverse_args(&id, ".", "a/b.pdf").is_err());
        assert!(inverse_args(&id, ".", "../x.pdf").is_err());
        assert!(inverse_args(&id, "../out", "a.pdf").is_err());
    }
}
