use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::io::{BufRead, BufReader};
use tauri::{AppHandle, Emitter};

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
pub fn compile_tex(app: AppHandle, input: String, workdir: String) -> Result<String, String> {
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
    let _ = app.emit("compile-line", format!("sidecar compile {} in {}", main_file, dir.to_string_lossy()));

    // 1) Bundled sidecar (externalBin `binaries/tectonic`), resolved without new deps.
    // Its stderr is the most relevant failure (the file was actually processed),
    // so it wins over PATH-missing noise below when everything fails.
    let mut last_err = String::from("no latex engine succeeded");
    let mut sidecar_err: Option<String> = None;
    if let Some(bin) = sidecar_path() {
        match Command::new(&bin)
            .args(["-X", "compile", &main_file, "--outdir", &outdir_str])
            .current_dir(&dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(mut child) => {
                let stdout = child.stdout.take().unwrap();
                let stderr = child.stderr.take().unwrap();
                
                let app_clone = app.clone();
                let stdout_handle = std::thread::spawn(move || {
                    let reader = BufReader::new(stdout);
                    for line in reader.lines() {
                        if let Ok(line) = line {
                            let _ = app_clone.emit("compile-line", line.clone());
                        }
                    }
                });
                
                let mut collected: Vec<String> = Vec::new();
                let stderr_reader = BufReader::new(stderr);
                for line in stderr_reader.lines() {
                    if let Ok(line) = line {
                        let _ = app.emit("compile-line", line.clone());
                        collected.push(line);
                    }
                }
                
                stdout_handle.join().unwrap();
                let status = child.wait();
                
                if let Ok(exit_status) = status {
                    if exit_status.success() {
                        let pdf = out_pdf(&outdir, &main_file);
                        return Ok(pdf.to_string_lossy().to_string());
                    }
                }
                let tail = collected.join("\n");
                let t = &tail[..500.min(tail.len())];
                sidecar_err = Some(format!("bundled tectonic failed: {}", t));
                last_err = sidecar_err.clone().unwrap();
            }
            Err(e) => {
                last_err = format!("bundled tectonic spawn failed (sidecar missing?): {}", e);
            }
        }
    }

    match sidecar_path() {
        None => return Err(String::from("bundled tectonic sidecar missing (src-tauri/binaries/) — no PATH fallback")),
        Some(_) => {}
    }
    if let Some(e) = sidecar_err { return Err(e); }
    Err(last_err)
}