// Generated from src-tauri/index (maleficium-index). Do not edit:
// change the Rust types, then run
//   MALEFICIUM_WRITE_TS=1 cargo test --manifest-path src-tauri/Cargo.toml --workspace

export type Source = "disk" | "buffer";

export type Unindexed = "not-text" | "too-large" | "not-utf8" | "over-budget";

export type Loc = { rel: string, line: number, };

export type MacroDef = { rel: string, line: number, command: string, params: number | null, body: string, };
