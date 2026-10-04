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
/// The interactive-widget package: one canonical file outside the template
/// tree, put into every template so a new project already holds it (after
/// that it is an ordinary project file, like any package the author adds).
const INTERACTIVE_DIR: &str = "../interactive";
const INTERACTIVE_STY: &str = "maleficium-interactive.sty";
/// Shared files every template embeds.
const SHARED_ALL: &[&str] = &["maleficium-footer.sty", "maleficium-mark.pdf"];
/// Shared files only some templates embed: file, template ids.
const SHARED_SOME: &[(&str, &[&str])] = &[
    (
        "maleficium-doc.sty",
        &[
            "article",
            "assignment",
            "book",
            "letter",
            "report",
            "welcome",
        ],
    ),
    ("maleficium-cv.sty", &["cv", "resume"]),
    ("maleficium-slides.sty", &["beamer"]),
    ("maleficium-links.sty", &["journal"]),
];

/// Embeds `templates/<id>/**` as `TEMPLATES: &[(id, &[(rel path, bytes)])]`,
/// shared by the desktop app and the MCP binary.
fn embed_templates() {
    let root = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("../templates");
    println!("cargo:rerun-if-changed={}", root.display());
    let shared = root.join(SHARED_DIR);
    println!("cargo:rerun-if-changed={}", shared.display());
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
        let shared_for = |name: &str| {
            SHARED_ALL.contains(&name)
                || SHARED_SOME
                    .iter()
                    .any(|(f, tids)| *f == name && tids.contains(&id.as_str()))
        };
        for name in SHARED_ALL.iter().chain(SHARED_SOME.iter().map(|(f, _)| f)) {
            if shared_for(name) {
                let dup = rels.iter().any(|(r, _)| r == name);
                assert!(
                    !dup,
                    "template {id} carries its own {name}; the copy in {SHARED_DIR}/ is the source"
                );
                rels.push((name.to_string(), shared.join(name)));
            }
        }
        let interactive = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap())
            .join(INTERACTIVE_DIR)
            .join(INTERACTIVE_STY);
        assert!(
            !rels.iter().any(|(r, _)| r == INTERACTIVE_STY),
            "template {id} carries its own {INTERACTIVE_STY}; interactive/ is the source"
        );
        rels.push((INTERACTIVE_STY.to_string(), interactive));
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
