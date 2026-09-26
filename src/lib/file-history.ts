// file-history.ts — file-op undo (separate from text undo).
//
// 50-entry memory stack of path references only, never file bytes.
// Inverse of every op is a move: delete moves to the app-local trash home.

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
 * Trash entry name: `<base>__<rel with / as __>__<ms>`, rel relative to
 * `root`. Same format as core `trash_name` (src-tauri/src/core/fs.rs), whose
 * stateless undo reads rel back out of the name.
 */
export function trashName(root: string, originalPath: string, at = Date.now()): string {
  const prefix = root.endsWith('/') ? root : root + '/';
  if (!originalPath.startsWith(prefix)) {
    throw new Error(`not in project: ${originalPath}`);
  }
  const rel = originalPath.slice(prefix.length);
  const base = rel.slice(rel.lastIndexOf('/') + 1) || 'file';
  return `${base}__${rel.split('/').join('__')}__${at}`;
}
