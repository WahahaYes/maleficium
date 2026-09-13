// paths.ts — app-local homes for state that must never litter the project dir.
//
// 05-versioning: trash, main-file association cache root, and compile `out/`
// live OUTSIDE the user's folder. Resolution order per item:
// explicit app-local path → legacy in-project fallback (read-only, V-2…V-4).
//
// Growth cap: path strings + one djb2 hex only; no file contents, no payload.

/** djb2 hex (8 chars) — per-project dir sharding, no new dep. */
export function hashRoot(root: string): string {
  let h = 5381;
  for (let i = 0; i < root.length; i++) {
    h = ((h << 5) + h + root.charCodeAt(i)) | 0;
  }
  return (h >>> 0).toString(16).padStart(8, '0');
}

/** Join segments with `/` (frontend-side; Rust mirrors with its own join). */
export function joinPath(...parts: string[]): string {
  return parts
    .filter((p) => p !== '')
    .join('/')
    .replace(/\/{2,}/g, '/');
}

/**
 * App-local trash home for one project root. Caller creates per-delete
 * subdirs as needed (`mkdir recursive`). Legacy `.maleficium-trash/` inside
 * the project is NEVER written here — it keeps its filter + scan skip.
 */
export function appTrashDir(appDataDir: string, root: string): string {
  return joinPath(appDataDir, 'maleficium-trash', hashRoot(root));
}

/**
 * App-local compile-output home for one project root. Mirrors the Rust
 * derivation in `commands/compile.rs` (`outDirFor`): same hash, same layout,
 * so the frontend log-read + Clean target the dir the engine wrote.
 */
export function appOutDir(tmpDir: string, root: string): string {
  return joinPath(tmpDir, 'maleficium-out', hashRoot(root));
}
