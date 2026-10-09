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

/// The single source for style files several templates load: overlaid
/// into each template's embedded files at build time, so the repo holds
/// one copy while instantiated projects stay self-contained.
const SHARED_DIR: &str = "shared";
/// Which shared file each template embeds (see the file's header); also
/// read by e2e/templates.py.
const OVERLAY: &str = "overlay.txt";

/// The overlay map: (path relative to shared/, template ids or None for all).
fn overlay(shared: &Path) -> Vec<(String, Option<Vec<String>>)> {
    let path = shared.join(OVERLAY);
    println!("cargo:rerun-if-changed={}", path.display());
    let text = std::fs::read_to_string(&path).expect("templates/shared/overlay.txt readable");
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| {
            let (file, ids) = l
                .split_once(':')
                .unwrap_or_else(|| panic!("overlay.txt: no `:` in `{l}`"));
            let ids = ids.trim();
            let ids = (ids != "*").then(|| ids.split_whitespace().map(String::from).collect());
            (file.trim().to_string(), ids)
        })
        .collect()
}

/// Embeds `templates/<id>/**` as `TEMPLATES: &[(id, &[(rel path, bytes)])]`,
/// shared by the desktop app and the MCP binary.
fn embed_templates() {
    let root = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("../templates");
    println!("cargo:rerun-if-changed={}", root.display());
    let shared = root.join(SHARED_DIR);
    println!("cargo:rerun-if-changed={}", shared.display());
    let map = overlay(&shared);
    let mut ids: Vec<_> = std::fs::read_dir(&root)
        .expect("templates dir readable")
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|id| id != SHARED_DIR)
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
        let mut rels: Vec<(String, PathBuf)> = list
            .into_iter()
            .map(|rel| {
                let s = rel.to_string_lossy().replace('\\', "/");
                (s, dir.join(&rel))
            })
            .collect();
        for (file, ids) in &map {
            if !ids.as_ref().is_none_or(|ids| ids.contains(&id)) {
                continue;
            }
            let src = shared.join(file);
            let name = Path::new(file)
                .file_name()
                .expect("an overlay entry names a file")
                .to_string_lossy()
                .to_string();
            assert!(
                !rels.iter().any(|(r, _)| *r == name),
                "template {id} carries its own {name}; {SHARED_DIR}/{OVERLAY} names its source"
            );
            assert!(src.is_file(), "overlay.txt names {file}, which is missing");
            rels.push((name, src));
        }
        rels.sort_by(|a, b| a.0.cmp(&b.0));
        writeln!(src, "    ({id:?}, &[").unwrap();
        for (rel, abs) in rels {
            println!("cargo:rerun-if-changed={}", abs.display());
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
}
