// fs-provider.ts — the filesystem seam.
//
// Primitives only: higher-level helpers (tree walks, mime sniffing, trash
// naming) build on these in their own modules. Absolute path strings are the
// interface currency. No `watch` member — live tracking crosses
// EventTransport, not this seam.

import { isWindowsPath, relUnder } from './paths';

function pathsEqual(a: string, b: string): boolean {
  const norm = (p: string): string => p.replace(/\\/g, '/').replace(/\/+$/, '');
  const [na, nb] = [norm(a), norm(b)];
  if (isWindowsPath(a) || isWindowsPath(b)) return na.toLowerCase() === nb.toLowerCase();
  return na === nb;
}

export interface FileStat {
  size: number;
  isDirectory: boolean;
  isFile: boolean;
}

export interface DirEntry {
  name: string;
  isDirectory: boolean;
  isFile: boolean;
}

export interface FsProvider {
  readText(path: string): Promise<string>;
  writeText(path: string, contents: string): Promise<void>;
  readBytes(path: string): Promise<Uint8Array>;
  writeBytes(path: string, contents: Uint8Array, opts?: { append?: boolean }): Promise<void>;
  listDir(path: string): Promise<DirEntry[]>;
  rename(from: string, to: string): Promise<void>;
  remove(path: string, opts?: { recursive?: boolean }): Promise<void>;
  mkdir(path: string, opts?: { recursive?: boolean }): Promise<void>;
  /** Null when the path does not exist. */
  stat(path: string): Promise<FileStat | null>;
}

export interface DialogFilter {
  name: string;
  extensions: string[];
}

export interface DialogProvider {
  /** `recursive` grants scope to the whole subtree, matching the open-time grant. */
  openDirectory(opts?: {
    title?: string;
    defaultPath?: string;
    recursive?: boolean;
  }): Promise<string | null>;
  saveFile(opts?: {
    title?: string;
    defaultPath?: string;
    filters?: DialogFilter[];
  }): Promise<string | null>;
}

let fsImpl: FsProvider | null = null;
let dialogImpl: DialogProvider | null = null;

/** Register the platform implementations. Called once at boot. */
export function setProviders(next: { fs: FsProvider; dialog: DialogProvider }) {
  fsImpl = next.fs;
  dialogImpl = next.dialog;
}

export function fs(): FsProvider {
  if (!fsImpl) throw new Error('fs provider not configured');
  return fsImpl;
}

export function dialog(): DialogProvider {
  if (!dialogImpl) throw new Error('dialog provider not configured');
  return dialogImpl;
}

// Bound project roots: canonical root path → grant root id. The desktop
// implementation routes paths under a bound root to the core file service;
// everything else (app data, temp, dialog destinations) stays on the
// platform filesystem. Bound at grant time (`projectAccess`); grants live
// for the session, so roots are never unbound.
const projectRoots = new Map<string, string>();

/** Register a granted root after `grantProjectAccess` / `grantUntitledAccess`. */
export function bindProjectRoot(rootId: string, root: string): void {
  projectRoots.set(root, rootId);
}

/** Forget bound roots (tests only). */
export function clearProjectRoots(): void {
  projectRoots.clear();
}

export interface ProjectPath {
  rootId: string;
  /** `/`-rel under the root; `''` for the root itself. */
  rel: string;
}

/**
 * The bound project holding `abs`, or null. Equality is separator- and
 * (Windows) case-insensitive; a root never claims its siblings (`/p` never
 * matches `/px/a`).
 */
export function lookupProjectRoot(abs: string): ProjectPath | null {
  for (const [root, rootId] of projectRoots) {
    if (pathsEqual(root, abs)) return { rootId, rel: '' };
    const rel = relUnder(root, abs);
    if (rel !== null) return { rootId, rel };
  }
  return null;
}
