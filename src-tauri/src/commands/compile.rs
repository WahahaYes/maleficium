use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::io::{BufRead, BufReader};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, State};

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

pub struct CompileState(pub Mutex<Option<std::process::Child>>);

impl Default for CompileState {
    fn default() -> Self {
        Self(Mutex::new(None))
    }
}

#[tauri::command]
pub fn compile_tex(app: AppHandle, state: State<'_, CompileState>, input: String, workdir: String) -> Result<String, String> {
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
            Ok(child) => {
                let mut child = child;
                let (Some(stdout), Some(stderr)) = (child.stdout.take(), child.stderr.take()) else {
                    let _ = child.kill();
                    return Err(String::from("sidecar pipes unavailable"));
                };
                state.0.lock().unwrap().replace(child);

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
                
                let _ = stdout_handle.join();
                match state.0.lock().unwrap().take() {
                    None => return Err(String::from("compile cancelled")),
                    Some(mut c) => match c.wait() {
                        Ok(s) if s.success() => {
                            let pdf = out_pdf(&outdir, &main_file);
                            return Ok(pdf.to_string_lossy().to_string());
                        }
                        _ => {
                            let tail = collected.join("\n");
                            let t = &tail[..500.min(tail.len())];
                            sidecar_err = Some(format!("bundled tectonic failed: {}", t));
                            last_err = sidecar_err.clone().unwrap();
                        }
                    },
                }
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

#[tauri::command]
pub fn cancel_compile(state: State<'_, CompileState>) -> Result<String, String> {
    match state.0.lock().unwrap().take() {
        Some(mut c) => {
            let _ = c.kill();
            let _ = c.wait();
            Ok(String::from("cancelled"))
        }
        None => Err(String::from("nothing to cancel"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn out_pdf_maps_tex_stem_to_pdf() {
        assert_eq!(out_pdf(Path::new("/t/out"), "hello.tex"), Path::new("/t/out/hello.pdf"));
    }

    #[test]
    fn out_pdf_appends_pdf_when_no_tex_suffix() {
        assert_eq!(out_pdf(Path::new("/t/out"), "hello"), Path::new("/t/out/hello.pdf"));
    }

    #[test]
    fn sidecar_triple_returns_non_empty() {
        assert!(!sidecar_triple().is_empty());
    }
}