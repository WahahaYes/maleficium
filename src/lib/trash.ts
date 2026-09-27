// trash.ts — Tauri-backed trash moves + undo via FileHistory references.
//
// Trash lives app-local, never inside the project. No in-project fallback.

import { fs } from './fs-provider';
import { appDataDir } from '@tauri-apps/api/path';
import { FileHistory, trashName } from './file-history';
import { appTrashDir, joinPath } from './paths';

/**
 * `rootId` is the project's grant id (`ProjectGrant.rootId`); `root` is the
 * granted project path, used to name the entry by its project-relative path.
 */
export async function moveToTrash(
  history: FileHistory,
  rootId: string,
  root: string,
  absPath: string,
): Promise<{ ok: boolean; error?: string }> {
  let dir: string;
  let name: string;
  try {
    dir = appTrashDir(await appDataDir(), rootId);
    name = trashName(root, absPath);
  } catch (e) {
    return { ok: false, error: String(e) };
  }
  try {
    await fs().mkdir(dir, { recursive: true });
  } catch (e) {
    return { ok: false, error: String(e) };
  }
  const dest = joinPath(dir, name);
  try {
    await fs().rename(absPath, dest);
  } catch {
    // Cross-device fallback: copy bytes then delete.
    try {
      const bytes = await fs().readBytes(absPath);
      await fs().writeBytes(dest, bytes);
      await fs().remove(absPath);
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
    await fs().rename(entry.trashPath, entry.originalPath);
    return { ok: true };
  } catch {
    // Rename across filesystems fails; copy the bytes back instead.
    try {
      const bytes = await fs().readBytes(entry.trashPath);
      await fs().writeBytes(entry.originalPath, bytes);
      await fs().remove(entry.trashPath);
      return { ok: true };
    } catch (e) {
      history.record({
        originalPath: entry.originalPath,
        trashPath: entry.trashPath,
        at: entry.at,
      });
      return { ok: false, error: String(e) };
    }
  }
}
