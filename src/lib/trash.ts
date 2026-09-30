// trash.ts — project file deletes to the core trash + undo.
//
// The trash home, entry naming, and the copy fallback all live in core
// (shared with the MCP server); this module keeps the app-local undo stack
// of path references.

import { request } from './core-request.tauri';
import type { FileHistory } from './file-history';
import { relUnder } from './paths';

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
  const rel = relUnder(root, absPath);
  if (rel === null) return { ok: false, error: `not in project: ${absPath}` };
  try {
    const trashPath = await request('fileTrash', { rootId, rel, confirm: absPath });
    history.record({ originalPath: absPath, trashPath });
    return { ok: true };
  } catch (e) {
    return { ok: false, error: String(e) };
  }
}

export async function undoTrash(
  history: FileHistory,
  rootId: string,
): Promise<{ ok: boolean; error?: string }> {
  const entry = history.pop();
  if (!entry) return { ok: false, error: 'nothing to undo' };
  try {
    await request('fileUndoTrash', { rootId, trashPath: entry.trashPath });
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
