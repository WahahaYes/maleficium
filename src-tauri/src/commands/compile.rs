use std::path::{Path, PathBuf};
use std::process::Command;

fn out_pdf(outdir: &Path, main_file: &str) -> PathBuf {
    let stem = main_file.strip_suffix(".tex").unwrap_or(main_file);
    outdir.join(format!("{}.pdf", stem))
}

/// Triple suffix matching `src-tauri/binaries/tectonic-<triple>` (Tauri externalBin).
fn sidecar_triple() -> &'static str {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => "x86_64-unknown-linux-gnu",
        ("linux", "aarch64") => "aarch64-unknown-linux-musl",
        ("macos", "aarch64") => "aarch64-apple-darwin",
        ("macos", "x86_64") => "x86_64-apple-darwin",
        ("windows", "x86_64") => "x86_64-pc-windows-msvc",
        _ => "x86_64-unknown-linux-gnu",
    }
}

/// Locate the bundled Tectonic sidecar:
/// 1) next to the running exe (release bundle via externalBin),
/// 2) `src-tauri/binaries/` in dev (CARGO_MANIFEST_DIR).
fn sidecar_path() -> Option<PathBuf> {
    let triple = sidecar_triple();
    let exe_name = if cfg!(windows) {
        format!("tectonic-{}.exe", triple)
    } else {
        format!("tectonic-{}", triple)
    };
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let p = dir.join(&exe_name);
            if p.exists() {
                return Some(p);
            }
        }
    }
    let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("binaries").join(&exe_name);
    if dev.exists() {
        return Some(dev);
    }
    None
}
#[tauri::command]
pub fn compile_tex(input: String, workdir: String) -> Result<String, String> {
    let (dir, main_file) = if Path::new(&input).is_absolute() {
        let p = Path::new(&input);
        let d = p.parent().map(|d| d.to_path_buf()).unwrap_or_else(|| PathBuf::from(&workdir));
        let f = p.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or(input.clone());
        (d, f)
    } else {
        (PathBuf::from(&workdir), input.clone())
    };
    let outdir = dir.join("out");
    let _ = std::fs::create_dir_all(&outdir);
    let outdir_str = outdir.to_string_lossy().to_string();

    // 1) Bundled sidecar (externalBin `binaries/tectonic`), resolved without new deps.
    // Its stderr is the most relevant failure (the file was actually processed),
    // so it wins over PATH-missing noise below when everything fails.
    let mut last_err = String::from("no latex engine succeeded");
    let mut sidecar_err: Option<String> = None;
    if let Some(bin) = sidecar_path() {
        match Command::new(&bin)
            .args(["-X", "compile", &main_file, "--outdir", &outdir_str])
            .current_dir(&dir)
            .output()
        {
            Ok(o) if o.status.success() => {
                let pdf = out_pdf(&outdir, &main_file);
                return Ok(pdf.to_string_lossy().to_string());
            }
            Ok(o) => {
                let tail = String::from_utf8_lossy(&o.stderr);
                let t = &tail[..500.min(tail.len())];
                sidecar_err = Some(format!("bundled tectonic failed: {}", t));
                last_err = sidecar_err.clone().unwrap();
            }
            Err(e) => {
                last_err = format!("bundled tectonic spawn failed (sidecar missing?): {}", e);
            }
        }
    }

    // 2) PATH fallback chain for dev machines without the sidecar built in.
    let attempts: Vec<(&str, Vec<String>)> = vec![
        ("latexmk", vec!["-cd".into(), "-interaction=nonstopmode".into(), "-file-line-error".into(), "-synctex=1".into(), "-output-directory".into(), outdir_str.clone(), main_file.clone()]),
        ("pdflatex", vec!["-interaction=nonstopmode".into(), "-file-line-error".into(), "-synctex=1".into(), "-output-directory".into(), outdir_str.clone(), main_file.clone()]),
        ("tectonic", vec!["-X".into(), "compile".into(), main_file.clone(), "--outdir".into(), outdir_str.clone()]),
    ];
    for (bin, args) in attempts {
        match Command::new(bin).args(&args).current_dir(&dir).output() {
            Ok(o) if o.status.success() => {
                let pdf = out_pdf(&outdir, &main_file);
                // latexmk may place the pdf beside the source instead of outdir; probe both.
                if pdf.exists() {
                    return Ok(pdf.to_string_lossy().to_string());
                }
                let alt = dir.join(format!("{}.pdf", main_file.strip_suffix(".tex").unwrap_or(&main_file)));
                if alt.exists() {
                    return Ok(alt.to_string_lossy().to_string());
                }
                return Ok(pdf.to_string_lossy().to_string());
            }
            Ok(o) => {
                let tail = String::from_utf8_lossy(&o.stderr);
                let t = &tail[..500.min(tail.len())];
                last_err = format!("{} failed: {}", bin, t);
            }
            Err(e) => {
                // Only PATH noise; keep any real sidecar stderr as the final error.
                if sidecar_err.is_none() {
                    last_err = format!("{} spawn failed: {} (tried sidecar first)", bin, e);
                }
            }
        }
    }
    if let Some(e) = sidecar_err {
        // Sidecar actually processed the file but failed: its message beats "not on PATH".
        if last_err.contains("spawn failed") || last_err == "no latex engine succeeded" {
            return Err(e);
        }
    }
    Err(last_err)
}
