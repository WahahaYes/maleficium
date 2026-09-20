// fs-provider.ts — the filesystem seam.
//
// Primitives only: higher-level helpers (tree walks, mime sniffing, trash
// naming) build on these in their own modules. Absolute path strings are the
// interface currency. No `watch` member — live tracking crosses
// EventTransport, not this seam.

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
