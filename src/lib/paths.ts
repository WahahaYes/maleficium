// paths.ts — app-local homes for state that must never litter the project dir.
//
// Trash, the event log, and the main-file association live outside the
// user's folder, with no in-project fallback; compile outputs are the
// backend's. Path strings only: no file contents.

// Native paths arrive in the host's form: `/home/u/p` or `C:\Users\u\p`
// (also UNC `\\server\share`). A path's own prefix picks its rules, so these
// helpers behave the same on every host and tests cover both forms anywhere.
// Project-relative paths (from the core, or `relUnder`) always use `/`.
// Path semantics are due to move into the Rust core (latex-project-cf).

const WIN_PREFIX = /^(?:[A-Za-z]:[\\/]|\\\\)/;

/** True for a Windows-form absolute path: a drive (`C:\`) or UNC prefix. */
export function isWindowsPath(p: string): boolean {
  return WIN_PREFIX.test(p);
}

/** Absolute in either form. */
export function isAbsolutePath(p: string): boolean {
  return p.startsWith('/') || WIN_PREFIX.test(p);
}

/** True when `p` names a location on disk, not a bare untitled name. */
export function hasDir(p: string): boolean {
  return /[\\/]/.test(p);
}

/** Index of the last separator; Windows paths accept both `\` and `/`. */
function lastSep(p: string): number {
  const slash = p.lastIndexOf('/');
  return isWindowsPath(p) ? Math.max(slash, p.lastIndexOf('\\')) : slash;
}

/** The last segment (a Unix path never splits on `\`, a legal name char). */
export function baseName(p: string): string {
  return p.slice(lastSep(p) + 1);
}

/** Everything before the last segment, or '' for a bare name. */
export function dirName(p: string): string {
  const i = lastSep(p);
  return i < 0 ? '' : p.slice(0, i);
}

/**
 * Join segments in the first one's form: `/` for Unix, `\` for Windows
 * (where `/`-separated rels are converted). Separator runs collapse, except
 * a UNC path's leading `\\`.
 */
export function joinPath(...parts: string[]): string {
  const segs = parts.filter((p) => p !== '');
  if (segs.length === 0) return '';
  if (!isWindowsPath(segs[0])) return segs.join('/').replace(/\/{2,}/g, '/');
  const lead = segs[0].startsWith('\\\\') ? '\\\\' : '';
  segs[0] = segs[0].slice(lead.length);
  return lead + segs.join('\\').replace(/[\\/]+/g, '\\');
}

/**
 * `abs` relative to `root` as a `/`-path, or null when it lies outside.
 * Windows paths match either separator and compare case-insensitively.
 */
export function relUnder(root: string, abs: string): string | null {
  if (!root) return null;
  if (!isWindowsPath(root)) {
    const prefix = root.endsWith('/') ? root : root + '/';
    return abs.startsWith(prefix) ? abs.slice(prefix.length) : null;
  }
  const r = root.replace(/\//g, '\\').replace(/\\+$/, '');
  const a = abs.replace(/\//g, '\\');
  if (a[r.length] !== '\\' || a.slice(0, r.length).toLowerCase() !== r.toLowerCase()) {
    return null;
  }
  return a.slice(r.length + 1).replace(/\\/g, '/');
}

/** A file name typed by the user, with separators of either form made `_`. */
export function safeName(name: string): string {
  return name.trim().replace(/[\\/]/g, '_');
}

/**
 * A native dialog's starting path, anchored at `home` unless already absolute:
 * the pickers never start in the process working directory.
 */
export function dialogStart(path: string | undefined, home: string): string {
  if (path && isAbsolutePath(path)) return path;
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
