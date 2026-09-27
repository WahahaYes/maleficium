// useFileOps.ts — create, rename, reload, delete, clean, and undo-delete.
//
// Every mutation marks an own-write first so the project watcher does not
// echo it back as an external change. Deletes go to the app-local trash and
// stay undoable; nothing is removed in place.

import { emit } from '../lib/events';
import { createFile, renamePath } from '../lib/files';
import { moveToTrash, undoTrash } from '../lib/trash';
import { dropBuffer, renameBuffer, type BufferState } from '../lib/buffers';
import { cleanOutputs } from '../lib/compile';
import { sourceFor, type SessionRoot } from '../lib/preview-bus';
import type { FileHistory } from '../lib/file-history';
import type { OwnWrites } from '../lib/own-writes';
import { hasDir } from '../lib/paths';

export interface UseFileOpsDeps {
  root: string | null;
  projectId: string | null;
  scratch: SessionRoot | null;
  fileName: string;
  previewFile: string | null;
  setFileName: (v: string) => void;
  mainFile: string | null;
  setPreviewFile: (v: string | null) => void;
  setBuffers: React.Dispatch<React.SetStateAction<Map<string, BufferState>>>;
  trash: FileHistory;
  ownWrites: OwnWrites;
  reloadTree: (r: string, deep?: boolean) => Promise<void>;
  handleSelect: (path: string) => Promise<void>;
}

export function useFileOps(deps: UseFileOpsDeps) {
  const {
    root,
    projectId,
    scratch,
    fileName,
    previewFile,
    setFileName,
    mainFile,
    setPreviewFile,
    setBuffers,
    trash,
    ownWrites,
    reloadTree,
    handleSelect,
  } = deps;

  async function handleCreate(dirPath: string, name: string) {
    try {
      const full = await createFile(dirPath, name);
      ownWrites.wrote(full, '');
      emit({
        scope: 'fs',
        kind: 'success',
        actor: 'user',
        message: 'created ' + full,
        event: { action: 'file.create', path: full },
      });
      if (root) await reloadTree(root, false);
      await handleSelect(full);
    } catch (e) {
      emit({
        scope: 'fs',
        kind: 'error',
        actor: 'user',
        message: 'create failed: ' + String(e).slice(0, 120),
        event: { action: 'file.create-failed', dir: dirPath, name, error: String(e).slice(0, 200) },
      });
    }
  }

  async function handleRename(oldPath: string, newName: string) {
    try {
      const full = await renamePath(oldPath, newName);
      ownWrites.wrote(oldPath, null);
      ownWrites.settled(full);
      setBuffers((b) => renameBuffer(b, oldPath, full));
      if (fileName === oldPath) {
        setFileName(full);
      }
      emit({
        scope: 'fs',
        kind: 'success',
        actor: 'user',
        message: `renamed to ${full}`,
        event: { action: 'file.rename', from: oldPath, to: full },
      });
      if (root) await reloadTree(root, false);
    } catch (e) {
      emit({
        scope: 'fs',
        kind: 'error',
        actor: 'user',
        message: 'rename failed: ' + String(e).slice(0, 120),
        event: { action: 'file.rename-failed', from: oldPath, error: String(e).slice(0, 200) },
      });
    }
  }

  async function handleDelete(path: string) {
    if (!root || !projectId) return;
    const r = await moveToTrash(trash, projectId, root, path);
    if (r.ok) {
      emit({
        scope: 'fs',
        kind: 'success',
        actor: 'user',
        message: `deleted ${path} (Edit → Undo Delete restores it)`,
        event: { action: 'file.delete', path },
      });
      setBuffers((b) => dropBuffer(b, path));
      if (previewFile === path) setPreviewFile(null);
      await reloadTree(root);
    } else {
      emit({
        scope: 'fs',
        kind: 'error',
        actor: 'user',
        message: 'delete failed: ' + (r.error ?? '').slice(0, 120),
        event: { action: 'file.delete-failed', path, error: (r.error ?? '').slice(0, 200) },
      });
    }
  }

  async function handleClean() {
    const target = mainFile ?? (hasDir(fileName) ? fileName : null);
    const project = root && projectId ? [{ rootId: projectId, path: root }] : [];
    const src = target ? sourceFor(target, scratch ? [...project, scratch] : project) : null;
    if (!src) {
      emit({
        scope: 'compile',
        kind: 'warn',
        actor: 'user',
        message: 'Clean: nothing to clean (no project file)',
        event: { action: 'compile.clean', removed: 0, reason: 'no-project-file' },
      });
      return;
    }
    // The backend owns the outdir: Clean names the main file, never a path.
    const r = await cleanOutputs(src.rootId, src.mainRel);
    if (r.ok) {
      emit({
        scope: 'compile',
        kind: r.removed > 0 ? 'success' : 'info',
        actor: 'user',
        message:
          r.removed > 0 ? `Cleaned ${src.mainRel} (${r.removed} files)` : 'Clean: already clean',
        event: { action: 'compile.clean', target: src.mainRel, removed: r.removed },
      });
      if (root) await reloadTree(root, false);
    } else {
      emit({
        scope: 'compile',
        kind: 'error',
        actor: 'user',
        message: 'Clean failed: ' + (r.error ?? '').slice(0, 120),
        event: {
          action: 'compile.clean-failed',
          target: src.mainRel,
          error: (r.error ?? '').slice(0, 200),
        },
      });
    }
  }

  async function handleUndo() {
    const entry = trash.list().at(-1);
    const r = await undoTrash(trash);
    if (r.ok) {
      emit({
        scope: 'fs',
        kind: 'success',
        actor: 'user',
        message: 'restored ' + (entry?.originalPath ?? 'from trash'),
        event: { action: 'file.undo-delete', path: entry?.originalPath ?? null },
      });
      if (root) {
        await reloadTree(root);
      }
    } else {
      emit({
        scope: 'fs',
        kind: 'error',
        actor: 'user',
        message: 'undo failed: ' + (r.error ?? '').slice(0, 120),
        event: { action: 'file.undo-delete-failed', error: (r.error ?? '').slice(0, 200) },
      });
    }
  }

  return {
    handleCreate,
    handleRename,
    handleDelete,
    handleClean,
    handleUndo,
  };
}
