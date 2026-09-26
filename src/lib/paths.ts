// paths.ts — app-local homes for state that must never litter the project dir.
//
// Trash, the event log, and the main-file association live outside the
// user's folder, with no in-project fallback; compile outputs are the
// backend's. Path strings only: no file contents.

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
 * App-local trash home for one project, sharded by the backend's root id
 * (`ProjectGrant.rootId`; the frontend never hashes paths itself). Nothing
 * is ever written to or read from the project dir.
 */
export function appTrashDir(appDataDir: string, rootId: string): string {
  return joinPath(appDataDir, 'maleficium-trash', rootId);
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
