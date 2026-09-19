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

/// Derive forward-sync args: `pdf_path` is the absolute outdir pdf (the
/// engine output the query runs against — outside any project root by
/// design); `tex_rel` resolves inside the root (absolute visible source —
/// the gz stores absolute Input paths).
pub fn forward_args(
    root_id: &str,
    pdf_path: &str,
    tex_rel: &str,
    line: u32,
) -> Result<ForwardArgs, String> {
    crate::commands::guard::reject_empty_nul(pdf_path)?;
    let pdf_canon = PathBuf::from(pdf_path)
        .canonicalize()
        .map_err(|e| format!("forbidden path (unresolvable): {}: {}", pdf_path, e))?;
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

/// Derive inverse-sync args: `dir_path` is the absolute outdir (engine
/// output, outside any project root by design); the pdf name is
/// interpolated into the `page:x:y:name` tag, so it must be bare.
pub fn inverse_args(root_id: &str, dir_path: &str, pdf_name: &str) -> Result<InverseArgs, String> {
    crate::commands::guard::reject_empty_nul(dir_path)?;
    let dir_canon = PathBuf::from(dir_path)
        .canonicalize()
        .map_err(|e| format!("forbidden path (unresolvable): {}: {}", dir_path, e))?;
    if !dir_canon.is_dir() {
        return Err(format!("not a directory: {}", dir_path));
    }
    let _ = root_id;
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
    fn forward_rejects_bad_paths() {
        let id = grant_tmp("escape");
        assert!(forward_args(&id, "/nonexistent/out.pdf", "a.tex", 1).is_err());
        assert!(forward_args(&id, "/tmp/maleficium-out", "/etc/hostname", 1).is_err());
    }

    #[test]
    fn inverse_rejects_non_bare_name() {
        let id = grant_tmp("bare");
        assert!(inverse_args(&id, "/tmp", "a/b.pdf").is_err());
        assert!(inverse_args(&id, "/tmp", "../x.pdf").is_err());
        assert!(inverse_args(&id, "/nonexistent-dir", "a.pdf").is_err());
    }
}
