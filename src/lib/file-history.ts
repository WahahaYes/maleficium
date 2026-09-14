// file-history.ts — file-op undo (separate from text undo).
//
// Growth cap: 50-entry memory stack of path references only, never file bytes.
// Inverse of every op is a move: delete MOVES to the app-local trash home
// (`lib/paths.ts appTrashDir`; RULES §8: no legacy).

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

export function trashName(originalPath: string, at = Date.now()): string {
  const base = originalPath.slice(originalPath.lastIndexOf('/') + 1) || 'file';
  return `${base}.${at}`;
}
