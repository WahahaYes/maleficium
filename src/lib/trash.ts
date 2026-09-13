// trash.ts — Tauri-backed trash moves + undo via FileHistory references.

import { mkdir, rename, readTextFile, writeTextFile, remove } from '@tauri-apps/plugin-fs';
import { FileHistory, trashName } from './file-history';

export function trashDir(root: string): string {
  return (root.endsWith('/') ? root : root + '/') + '.maleficium-trash';
}

export async function moveToTrash(
  history: FileHistory,
  root: string,
  absPath: string,
): Promise<{ ok: boolean; error?: string }> {
  const dir = trashDir(root);
  try {
    await mkdir(dir, { recursive: true });
  } catch (e) {
    return { ok: false, error: String(e) };
  }
  const dest = dir + '/' + trashName(absPath);
  try {
    await rename(absPath, dest);
  } catch {
    // Cross-device fallback: copy bytes then delete.
    try {
      const bytes = await readTextFile(absPath);
      await writeTextFile(dest, bytes);
      await remove(absPath);
    } catch (e) {
      return { ok: false, error: String(e) };
    }
  }
  history.record({ originalPath: absPath, trashPath: dest });
  return { ok: true };
}

export async function undoTrash(history: FileHistory): Promise<{ ok: boolean; error?: string }> {
  const entry = history.pop();
  if (!entry) return { ok: false, error: 'nothing to undo' };
  try {
    await rename(entry.trashPath, entry.originalPath);
    return { ok: true };
  } catch (e) {
    // Restore the entry so a retry is possible.
    history.record({ originalPath: entry.originalPath, trashPath: entry.trashPath, at: entry.at });
    return { ok: false, error: String(e) };
  }
}
