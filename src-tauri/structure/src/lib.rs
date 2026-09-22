//! Document structure of LaTeX text: outline, symbols (labels, refs, cites,
//! inputs, bibliographies), bib keys, and engine-log diagnostics.
//!
//! The one implementation behind every surface: the desktop app calls it
//! over a Tauri command, the MCP tools call it through `core::structure`,
//! and a web client compiles it to wasm32. Pure by contract: text in,
//! records out. No fs, no process, no Tauri: resolution against a project
//! lives with the caller.

mod diagnostics;
mod outline;
mod symbols;
mod text;

pub use diagnostics::{diagnostics, Diagnostic, Severity};
pub use outline::{outline, Outline, OutlineEntry, OutlineKind, MAX_OUTLINE_ENTRIES};
pub use symbols::{bib_keys, symbols, InputAt, KeyAt, Symbols};

/// The generated TypeScript module for the types the frontend receives
/// (`src/lib/generated/structure.ts`).
pub fn typescript() -> String {
    use ts_rs::{Config, TS};
    let cfg = Config::new().with_large_int("number");
    let decls = [
        OutlineKind::decl(&cfg),
        OutlineEntry::decl(&cfg),
        Outline::decl(&cfg),
        Severity::decl(&cfg),
        Diagnostic::decl(&cfg),
    ];
    let mut out = String::from(
        "// Generated from src-tauri/structure (maleficium-structure). Do not edit:\n\
         // change the Rust types, then run\n\
         //   MALEFICIUM_WRITE_TS=1 cargo test --manifest-path src-tauri/Cargo.toml --workspace\n",
    );
    for d in decls {
        out.push_str("\nexport ");
        out.push_str(&d);
        out.push('\n');
    }
    out
}

/// Content revision: FNV-1a 64 over `(name, text)` pairs in order, as 16 hex
/// chars. Stable across builds and platforms (unlike `DefaultHasher`).
pub fn revision<'a>(parts: impl IntoIterator<Item = (&'a str, &'a str)>) -> String {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut h = OFFSET;
    let mut feed = |bytes: &[u8]| {
        for b in bytes {
            h ^= u64::from(*b);
            h = h.wrapping_mul(PRIME);
        }
    };
    for (name, text) in parts {
        feed(name.as_bytes());
        feed(&[0]);
        feed(text.as_bytes());
        feed(&[0]);
    }
    format!("{:016x}", h)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typescript_bindings_are_fresh() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../src/lib/generated/structure.ts");
        let want = typescript();
        if std::env::var_os("MALEFICIUM_WRITE_TS").is_some() {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, &want).unwrap();
        }
        let have = std::fs::read_to_string(&path).unwrap_or_default();
        assert!(
            have == want,
            "{} is stale: run MALEFICIUM_WRITE_TS=1 cargo test --manifest-path src-tauri/Cargo.toml --workspace",
            path.display()
        );
    }

    #[test]
    fn revision_is_stable_and_content_sensitive() {
        let a = revision([("main.tex", "x")]);
        assert_eq!(a, revision([("main.tex", "x")]));
        assert_eq!(a.len(), 16);
        assert_ne!(a, revision([("main.tex", "y")]));
        assert_ne!(a, revision([("other.tex", "x")]));
        // The separator keeps name/text boundaries from colliding.
        assert_ne!(revision([("ab", "c")]), revision([("a", "bc")]));
    }
}
