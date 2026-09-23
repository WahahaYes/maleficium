use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// Every file under `dir`, relative, sorted.
fn files(dir: &Path, rel: &Path, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<_> = std::fs::read_dir(dir.join(rel))
        .expect("templates dir readable")
        .flatten()
        .collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let r = rel.join(e.file_name());
        if e.file_type().expect("file type").is_dir() {
            files(dir, &r, out);
        } else {
            out.push(r);
        }
    }
}

/// Embeds `templates/<id>/**` as `TEMPLATES: &[(id, &[(rel path, bytes)])]`,
/// shared by the desktop app and the MCP binary.
fn embed_templates() {
    let root = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("templates");
    println!("cargo:rerun-if-changed={}", root.display());
    let mut ids: Vec<_> = std::fs::read_dir(&root)
        .expect("templates dir readable")
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    ids.sort();
    let mut src = String::from(
        "/// One template's files: (relative path, bytes).\n\
         pub type TemplateFiles = &'static [(&'static str, &'static [u8])];\n\
         pub static TEMPLATES: &[(&str, TemplateFiles)] = &[\n",
    );
    for id in ids {
        let dir = root.join(&id);
        let mut list = Vec::new();
        files(&dir, Path::new(""), &mut list);
        writeln!(src, "    ({id:?}, &[").unwrap();
        for rel in list {
            let abs = dir.join(&rel);
            println!("cargo:rerun-if-changed={}", abs.display());
            let rel = rel.to_string_lossy().replace('\\', "/");
            writeln!(
                src,
                "        ({rel:?}, include_bytes!({:?})),",
                abs.display().to_string()
            )
            .unwrap();
        }
        src.push_str("    ]),\n");
    }
    src.push_str("];\n");
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("templates.rs");
    std::fs::write(out, src).expect("templates table written");
}

fn main() {
    embed_templates();
    tauri_build::build()
}
