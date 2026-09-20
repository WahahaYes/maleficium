// paths.ts — app-local homes for state that must never litter the project dir.
//
// Trash, the main-file association cache root, and compile `out/` live outside
// the user's folder. App-local paths are the only paths: no in-project
// fallbacks.
//
// Path strings + one djb2 hex only; no file contents, no payload.

/** djb2 hex (8 chars) for per-project dir sharding. */
export function hashRoot(root: string): string {
  let h = 5381;
  for (let i = 0; i < root.length; i++) {
    h = ((h << 5) + h + root.charCodeAt(i)) | 0;
  }
  return (h >>> 0).toString(16).padStart(8, '0');
}

/** Join segments with `/`. */
export function joinPath(...parts: string[]): string {
  return parts
    .filter((p) => p !== '')
    .join('/')
    .replace(/\/{2,}/g, '/');
}

/**
 * App-local trash home for one project root. Per-delete subdirs are created
 * as needed. Nothing is ever written to or read from the project dir.
 */
export function appTrashDir(appDataDir: string, root: string): string {
  return joinPath(appDataDir, 'maleficium-trash', hashRoot(root));
}

/**
 * App-local compile-output home for one project root. Same hash and layout
 * as the engine outdir, so log reads + Clean target the dir the engine
 * wrote. The base is the OS app-cache dir.
 */
export function appOutDir(baseDir: string, root: string): string {
  return joinPath(baseDir, 'maleficium-out', hashRoot(root));
}

/**
 * App-local history home for one project id. Holds the revision index and
 * the content-addressed blob tree. Nothing is written to the project dir.
 */
export function appHistoryDir(appDataDir: string, projectId: string): string {
  return joinPath(appDataDir, 'maleficium-history', projectId);
}

/** Blob path for one content hash: two-char fan-out under the history home. */
export function historyBlobPath(historyDir: string, hash: string): string {
  return joinPath(historyDir, 'blobs', hash.slice(0, 2), hash);
}

/**
 * App-local event-log home. The current run's JSONL file lives here; the
 * project dir never holds a log.
 */
export function appEventLogDir(appDataDir: string): string {
  return joinPath(appDataDir, 'maleficium-log');
}

/** The JSONL file the running app records its event stream to. */
export function eventLogPath(appDataDir: string): string {
  return joinPath(appEventLogDir(appDataDir), 'events.jsonl');
}
