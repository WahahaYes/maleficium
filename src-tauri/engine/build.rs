//! Compiles the vendored SyncTeX parser (see synctex/README.md) and links it
//! with a static zlib. The parser reads `<pdf>.synctex.gz`.

use std::env;
use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;

/// Writes `$OUT_DIR/embedded.rs`: a table of every file in `embedded/` (dotfiles
/// skipped), so the `kpsewhich` mode serves it without a code change. Adding a
/// file to the directory is all it takes.
fn embedded_table() {
    let dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("embedded");
    let mut names: Vec<String> = fs::read_dir(&dir)
        .map(|rd| {
            rd.filter_map(Result::ok)
                .filter(|e| e.path().is_file())
                .filter_map(|e| e.file_name().into_string().ok())
                .filter(|n| !n.starts_with('.'))
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    let mut src = String::from("pub static EMBEDDED: &[(&str, &[u8])] = &[\n");
    for n in &names {
        let path = dir.join(n);
        writeln!(
            src,
            "    ({n:?}, include_bytes!({:?})),",
            path.display().to_string()
        )
        .unwrap();
    }
    src.push_str("];\n");
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("embedded.rs");
    fs::write(out, src).unwrap();
    println!("cargo:rerun-if-changed=embedded");
}

fn main() {
    embedded_table();
    let mut build = cc::Build::new();
    build
        .file("synctex/synctex_parser.c")
        .file("synctex/synctex_parser_utils.c")
        .include("synctex")
        // Tectonic's XeTeX writes SyncTeX with its own `synctex_sheet`; rename the
        // parser's so the two link into one binary.
        .define("synctex_sheet", "maleficium_synctex_sheet")
        .warnings(false);
    // libz-sys publishes its header directory to the crates that depend on it.
    if let Some(inc) = env::var_os("DEP_Z_INCLUDE") {
        build.include(inc);
    }
    build.compile("synctex");
    match env::var("CARGO_CFG_TARGET_OS").as_deref() {
        // The parser's Windows path helpers live in shlwapi.
        Ok("windows") => println!("cargo:rustc-link-lib=shlwapi"),
        _ => println!("cargo:rustc-link-lib=m"),
    }
    println!("cargo:rerun-if-changed=synctex");
}
