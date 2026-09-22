//! Structure commands: thin adapters over `maleficium_structure` for text the
//! frontend holds (the live buffer, the streamed log). Pure: no session root,
//! no fs. A web client runs the same crate as wasm instead of this IPC hop.

use maleficium_structure::{self as ms, Diagnostic, Outline};

#[tauri::command]
pub fn structure_outline(text: String) -> Outline {
    ms::outline(&text)
}

/// `root` and `base` only resolve log paths; results are root-relative.
#[tauri::command]
pub fn structure_diagnostics(log: String, root: String, base: String) -> Vec<Diagnostic> {
    ms::diagnostics(&log, &root, &base)
}
