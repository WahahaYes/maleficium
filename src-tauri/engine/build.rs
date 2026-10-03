//! Compiles the vendored SyncTeX parser (see synctex/README.md) and links it
//! with a static zlib. The parser reads `<pdf>.synctex.gz`.

use std::env;

fn main() {
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
