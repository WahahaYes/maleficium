//! The examples under `examples/` are projects as the app makes them: each
//! carries `maleficium-interactive.sty` at its root, and that copy must be
//! the package the app ships, or the example documents an older one.

use std::path::PathBuf;

fn repo() -> PathBuf {
    dunce::canonicalize(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")).unwrap()
}

#[test]
fn every_example_carries_the_shipped_interactive_package() {
    let canon = std::fs::read(repo().join("embed-runtime/tex/maleficium-interactive.sty")).unwrap();
    let mut seen = 0;
    for e in std::fs::read_dir(repo().join("examples")).unwrap() {
        let dir = e.unwrap().path();
        if !dir.join("fixture.json").is_file() {
            continue;
        }
        let name = dir.file_name().unwrap().to_string_lossy().into_owned();
        let sty = std::fs::read(dir.join("maleficium-interactive.sty"))
            .unwrap_or_else(|_| panic!("examples/{name} has no maleficium-interactive.sty"));
        assert!(
            sty == canon,
            "examples/{name}/maleficium-interactive.sty differs from embed-runtime/tex/: copy it over"
        );
        seen += 1;
    }
    assert!(seen > 0, "no example with a fixture.json under examples/");
}
