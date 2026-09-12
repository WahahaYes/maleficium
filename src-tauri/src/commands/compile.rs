use std::process::Command;
use std::fs;
use std::path::Path;

#[tauri::command]
pub fn compile_tex(input: String, workdir: String) -> Result<String, String> {
    let (main_file, dir) = if Path::new(&input).is_absolute() {
        (Path::new(&input).file_name().unwrap().to_str().unwrap().to_string(), Path::new(&input).parent().unwrap().to_str().unwrap().to_string())
    } else { (input.clone(), workdir.clone()) };
    let outdir = Path::new(&dir).join("out");
    fs::create_dir_all(&outdir).map_err(|e| e.to_string())?;
    let outdir_str = outdir.to_str().unwrap();
    let file = &main_file;
    let result = Command::new("latexmk").args(["-cd", "-interaction=nonstopmode", "-file-line-error", "-synctex=1", "-output-directory", outdir_str, file]).current_dir(&dir).output().ok().filter(|o| o.status.success())
        .or_else(|| Command::new("pdflatex").args(["-interaction=nonstopmode", "-file-line-error", "-synctex=1", "-output-directory", outdir_str, file]).current_dir(&dir).output().ok().filter(|o| o.status.success()))
        .or_else(|| Command::new("tectonic").args(["-X", "compile", file, "--outdir", outdir_str]).current_dir(&dir).output().ok().filter(|o| o.status.success()));
    match result {
        Some(_) => { let pdf = outdir.join(file.strip_suffix(".tex").unwrap_or(file).to_string() + ".pdf"); if pdf.exists() { Ok(pdf.to_str().unwrap().to_string()) } else { Ok(outdir.join(file).with_extension("pdf").to_str().unwrap().to_string()) } }
        None => Err("no latex binary available (tried latexmk, pdflatex, tectonic)".to_string())
    }
}