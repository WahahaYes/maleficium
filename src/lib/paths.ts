// paths.ts — app-local homes for state that must never litter the project dir.
//
// Trash, the event log, and the main-file association cache root live outside the
// user's folder; compile outputs are the backend's. App-local paths are the only paths: no in-project
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
 * A native dialog's starting path, anchored at `home` unless already absolute:
 * the pickers never start in the process working directory.
 */
export function dialogStart(path: string | undefined, home: string): string {
  if (path && path.startsWith('/')) return path;
  return path ? joinPath(home, path) : home;
}

/**
 * App-local trash home for one project root. Per-delete subdirs are created
 * as needed. Nothing is ever written to or read from the project dir.
 */
export function appTrashDir(appDataDir: string, root: string): string {
  return joinPath(appDataDir, 'maleficium-trash', hashRoot(root));
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
