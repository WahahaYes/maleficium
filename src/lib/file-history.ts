// file-history.ts — file-op undo (separate from text undo).
//
// 50-entry memory stack of path references only, never file bytes.
// Inverse of every op is a move: delete moves to the app-local trash home.

import { relUnder } from './paths';

export interface FileHistoryEntry {
  /** Original absolute path before the op. */
  originalPath: string;
  /** Absolute trash path the file was moved to. */
  trashPath: string;
  at: number;
}

const MAX_ENTRIES = 50;

export class FileHistory {
  private stack: FileHistoryEntry[] = [];

  get size(): number {
    return this.stack.length;
  }

  record(e: Omit<FileHistoryEntry, 'at'> & { at?: number }): void {
    this.stack.push({ ...e, at: e.at ?? Date.now() });
    if (this.stack.length > MAX_ENTRIES) {
      this.stack.splice(0, this.stack.length - MAX_ENTRIES);
    }
  }

  pop(): FileHistoryEntry | undefined {
    return this.stack.pop();
  }

  list(): FileHistoryEntry[] {
    return [...this.stack];
  }

  clear(): void {
    this.stack.length = 0;
  }
}

/**
 * Escape one path component so `__` in a trash name is only ever a separator:
 * `%` -> `%25`, then `_` -> `%5F`. Same as core `escape_component`.
 */
function escapeComponent(s: string): string {
  return s.replace(/%/g, '%25').replace(/_/g, '%5F');
}

/**
 * Trash entry name: `<base>__<c1>__<c2>...__<ms>`, where c1..cn are the path
 * components relative to `root` and every component is escaped. Same format
 * as core `trash_name` (src-tauri/src/core/fs.rs), whose stateless undo reads
 * rel back out of the name.
 */
export function trashName(root: string, originalPath: string, at = Date.now()): string {
  const rel = relUnder(root, originalPath);
  if (rel === null) {
    throw new Error(`not in project: ${originalPath}`);
  }
  const base = rel.slice(rel.lastIndexOf('/') + 1) || 'file';
  const flat = rel.split('/').map(escapeComponent).join('__');
  return `${escapeComponent(base)}__${flat}__${at}`;
}
