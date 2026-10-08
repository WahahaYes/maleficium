//! The licence boundary: everything Maleficium copies into an exported paper
//! or a user's project comes from `embed-runtime/` (MIT-0), never from the
//! AGPL app. This pins the Rust side: what the exporter and the runtime
//! scaffold embed, and that they write no script or style of their own.

use regex::Regex;
use std::path::PathBuf;

/// Every module that writes a bundle or scaffolds files into a project.
const OUTPUT_MODULES: &[&str] = &[
    "src/bundle.rs",
    "src/bundle/flags.rs",
    "src/bundle/fold.rs",
    "src/bundle/reader.rs",
    "src/runtime_author.rs",
];

/// Embedded but never copied out, from the repo root: read by the app itself.
const READ_ONLY: &[(&str, &str)] = &[
    (
        "src-tauri/core/schemas/paper-bundle-1.schema.json",
        "validates the manifest",
    ),
    ("package.json", "the vendored three.js version"),
];

fn core() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn repo() -> PathBuf {
    dunce::canonicalize(core().join("../..")).unwrap()
}

fn embed_runtime() -> PathBuf {
    repo().join("embed-runtime")
}

/// A module's source without its trailing test module.
fn shipped_source(rel: &str) -> String {
    let src = std::fs::read_to_string(core().join(rel)).unwrap();
    match src.find("#[cfg(test)]\nmod tests") {
        Some(at) => src[..at].to_string(),
        None => src,
    }
}

#[test]
fn every_module_that_writes_output_is_listed() {
    // A new file under bundle/ joins OUTPUT_MODULES, or this fails.
    for e in std::fs::read_dir(core().join("src/bundle")).unwrap() {
        let p = e.unwrap().path();
        if p.extension().is_some_and(|x| x == "rs") && !p.ends_with("tests.rs") {
            let rel = format!("src/bundle/{}", p.file_name().unwrap().to_string_lossy());
            assert!(
                OUTPUT_MODULES.contains(&rel.as_str()),
                "{rel} is not in OUTPUT_MODULES"
            );
        }
    }
}

#[test]
fn what_the_exporter_and_scaffold_embed_comes_from_embed_runtime() {
    let include = Regex::new(r#"include_(?:str|bytes)!\(\s*"([^"]+)"\s*\)"#).unwrap();
    let root = embed_runtime();
    let mut seen = 0;
    for rel in OUTPUT_MODULES {
        let dir = core().join(rel).parent().unwrap().to_path_buf();
        for c in include.captures_iter(&shipped_source(rel)) {
            let path = dunce::canonicalize(dir.join(&c[1])).unwrap();
            if READ_ONLY.iter().any(|(r, _)| path == repo().join(r)) {
                continue;
            }
            assert!(
                path.starts_with(&root),
                "{rel} embeds {}, outside embed-runtime/: move it there (MIT-0) or, if it is never \
                 copied into output, add it to READ_ONLY",
                &c[1]
            );
            seen += 1;
        }
    }
    assert!(
        seen >= 15,
        "the scan found only {seen} embeds; did the include form change?"
    );
}

#[test]
fn the_exporter_writes_no_script_or_style_of_its_own() {
    // A script or style body written as a Rust literal would ship AGPL code
    // inside the paper; it belongs in embed-runtime/ and an include_str!.
    // Allowed: an opening tag that ends the literal, takes a format
    // argument, or carries attributes (json islands, src=).
    let body = Regex::new(r#"<(script|style)>([^"{\\]|\\[^"])"#).unwrap();
    for rel in OUTPUT_MODULES {
        for (n, line) in shipped_source(rel).lines().enumerate() {
            if line.trim_start().starts_with("//") {
                continue;
            }
            if let Some(m) = body.find(line) {
                panic!(
                    "{rel}:{} writes a <script> or <style> body inline: {}",
                    n + 1,
                    m.as_str()
                );
            }
        }
    }
}

#[test]
fn the_interactive_package_templates_carry_is_the_embed_runtime_copy() {
    let overlay = core().join("../templates/shared");
    let text = std::fs::read_to_string(overlay.join("overlay.txt")).unwrap();
    let entry = text
        .lines()
        .filter_map(|l| l.split_once(':').map(|(p, _)| p.trim()))
        .find(|p| p.ends_with("maleficium-interactive.sty"))
        .expect("overlay.txt carries the interactive package");
    let path = dunce::canonicalize(overlay.join(entry)).unwrap();
    assert_eq!(path, embed_runtime().join("tex/maleficium-interactive.sty"));
}

#[test]
fn embed_runtime_carries_one_mit0_licence() {
    let licence = std::fs::read_to_string(embed_runtime().join("LICENSE")).unwrap();
    assert!(licence.starts_with("MIT No Attribution\n"), "{licence}");
    let sty =
        std::fs::read_to_string(embed_runtime().join("tex/maleficium-interactive.sty")).unwrap();
    assert!(
        sty.lines().take(4).any(|l| l.contains("MIT-0")),
        "the package header names MIT-0"
    );
}
