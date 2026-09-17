// trash.ts — Tauri-backed trash moves + undo via FileHistory references.
//
// Trash lives APP-LOCAL (`appDataDir/maleficium-trash/<hash>/`), never inside
// the project. No in-project fallback (RULES §8: no legacy).

import { mkdir, rename, readFile, writeFile, remove } from '@tauri-apps/plugin-fs';
import { appDataDir } from '@tauri-apps/api/path';
import { FileHistory, trashName } from './file-history';
import { appTrashDir } from './paths';

/** App-local trash home for this project. */
export async function trashDir(root: string): Promise<string> {
  const base = await appDataDir();
  return appTrashDir(base, root);
}

export async function moveToTrash(
  history: FileHistory,
  root: string,
  absPath: string,
): Promise<{ ok: boolean; error?: string }> {
  let dir: string;
  try {
    dir = await trashDir(root);
  } catch (e) {
    return { ok: false, error: String(e) };
  }
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
      const bytes = await readFile(absPath);
      await writeFile(dest, bytes);
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
