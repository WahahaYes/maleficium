// useFileOps.ts — create, rename, reload, delete, clean, and undo-delete.
//
// Every mutation marks an own-write first so the project watcher does not
// echo it back as an external change. Deletes go to the app-local trash and
// stay undoable; nothing is removed in place.

import { emit } from '../lib/events';
import { createFile, loadTex, renamePath } from '../lib/files';
import { moveToTrash, undoTrash } from '../lib/trash';
import type { BufferState } from '../lib/buffers';
import { fs } from '../lib/fs-provider';
import { appOutDir } from '../lib/paths';
import { appCacheDir } from '@tauri-apps/api/path';
import type { FileHistory } from '../lib/file-history';

export interface UseFileOpsDeps {
  root: string | null;
  fileName: string;
  reloadPath: string | null;
  previewFile: string | null;
  setFileName: (v: string) => void;
  mainFile: string | null;
  setTex: (v: string) => void;
  setPreviewFile: (v: string | null) => void;
  setReloadPath: (v: string | null) => void;
  setBuffers: React.Dispatch<React.SetStateAction<Map<string, BufferState>>>;
  trash: FileHistory;
  markOwnWrite: (p: string) => void;
  reloadTree: (r: string, deep?: boolean) => Promise<void>;
  handleSelect: (path: string) => Promise<void>;
}

export function useFileOps(deps: UseFileOpsDeps) {
  const {
    root,
    fileName,
    reloadPath,
    previewFile,
    setFileName,
    mainFile,
    setTex,
    setPreviewFile,
    setReloadPath,
    setBuffers,
    trash,
    markOwnWrite,
    reloadTree,
    handleSelect,
  } = deps;

  async function handleCreate(dirPath: string, name: string) {
    try {
      const full = await createFile(dirPath, name);
      markOwnWrite(full);
      emit({
        scope: 'fs',
        kind: 'success',
        message: 'created ' + full,
        data: { action: 'file.create', path: full },
      });
      if (root) await reloadTree(root, false);
      await handleSelect(full);
    } catch (e) {
      emit({
        scope: 'fs',
        kind: 'error',
        message: 'create failed: ' + String(e).slice(0, 120),
        data: { action: 'file.create-failed', dir: dirPath, name, error: String(e).slice(0, 200) },
      });
    }
  }

  async function handleRename(oldPath: string, newName: string) {
    try {
      const full = await renamePath(oldPath, newName);
      markOwnWrite(oldPath);
      markOwnWrite(full);
      setBuffers((b) => {
        const prev = b.get(oldPath);
        if (!prev) return b;
        const n = new Map(b);
        n.delete(oldPath);
        n.set(full, prev);
        return n;
      });
      if (fileName === oldPath) {
        setFileName(full);
        setReloadPath(null);
      }
      emit({
        scope: 'fs',
        kind: 'success',
        message: `renamed to ${full}`,
        data: { action: 'file.rename', from: oldPath, to: full },
      });
      if (root) await reloadTree(root, false);
    } catch (e) {
      emit({
        scope: 'fs',
        kind: 'error',
        message: 'rename failed: ' + String(e).slice(0, 120),
        data: { action: 'file.rename-failed', from: oldPath, error: String(e).slice(0, 200) },
      });
    }
  }

  async function handleReload() {
    if (!reloadPath) return;
    try {
      const content = await loadTex(reloadPath);
      setBuffers((b) => {
        const n = new Map(b);
        n.set(reloadPath, {
          value: content,
          dirty: false,
          version: (n.get(reloadPath)?.version ?? 0) + 1,
        });
        return n;
      });
      if (reloadPath === fileName) setTex(content);
      setReloadPath(null);
      emit({
        scope: 'fs',
        kind: 'success',
        message: 'reloaded ' + reloadPath,
        data: { action: 'file.reload', path: reloadPath, chars: content.length },
      });
    } catch (e) {
      emit({
        scope: 'fs',
        kind: 'error',
        message: 'reload failed: ' + String(e).slice(0, 120),
        data: { action: 'file.reload-failed', path: reloadPath, error: String(e).slice(0, 200) },
      });
    }
  }

  async function handleDelete(path: string) {
    if (!root) return;
    const r = await moveToTrash(trash, root, path);
    if (r.ok) {
      emit({
        scope: 'fs',
        kind: 'success',
        message: `deleted ${path} (Edit → Undo Delete restores it)`,
        data: { action: 'file.delete', path },
      });
      setBuffers((b) => {
        const n = new Map(b);
        n.delete(path);
        return n;
      });
      if (previewFile === path) setPreviewFile(null);
      await reloadTree(root);
    } else {
      emit({
        scope: 'fs',
        kind: 'error',
        message: 'delete failed: ' + (r.error ?? '').slice(0, 120),
        data: { action: 'file.delete-failed', path, error: (r.error ?? '').slice(0, 200) },
      });
    }
  }

  async function handleClean() {
    const target = mainFile ?? (fileName.includes('/') ? fileName : null);
    if (!target || !target.includes('/')) {
      emit({
        scope: 'compile',
        kind: 'warn',
        message: 'Clean: nothing to clean (no project file)',
        data: { action: 'compile.clean', removed: 0, reason: 'no-project-file' },
      });
      return;
    }
    // App-local outdir over the app-cache dir: clean never touches the
    // project dir.
    const dir = target.slice(0, target.lastIndexOf('/')) || '/tmp';
    const out = appOutDir(await appCacheDir(), dir);
    try {
      // Per-entry removal: build artifacts only, never sources. Missing
      // dir = already clean.
      let entries = [];
      try {
        entries = await fs().listDir(out);
      } catch {
        emit({
          scope: 'compile',
          kind: 'info',
          message: 'Clean: already clean',
          data: { action: 'compile.clean', out, removed: 0 },
        });
        return;
      }
      let n = 0;
      for (const e of entries) {
        try {
          await fs().remove(out + '/' + e.name);
          n++;
        } catch {
          /* keep going — report count at end */
        }
      }
      markOwnWrite(out);
      emit({
        scope: 'compile',
        kind: 'success',
        message: `Cleaned ${out} (${n} files)`,
        data: { action: 'compile.clean', out, removed: n },
      });
      if (root) await reloadTree(root, false);
    } catch (e) {
      emit({
        scope: 'compile',
        kind: 'error',
        message: 'Clean failed: ' + String(e).slice(0, 120),
        data: { action: 'compile.clean-failed', out, error: String(e).slice(0, 200) },
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
        message: 'restored ' + (entry?.originalPath ?? 'from trash'),
        data: { action: 'file.undo-delete', path: entry?.originalPath ?? null },
      });
      if (root) {
        await reloadTree(root);
      }
    } else {
      emit({
        scope: 'fs',
        kind: 'error',
        message: 'undo failed: ' + (r.error ?? '').slice(0, 120),
        data: { action: 'file.undo-delete-failed', error: (r.error ?? '').slice(0, 200) },
      });
    }
  }

  return {
    handleCreate,
    handleRename,
    handleReload,
    handleDelete,
    handleClean,
    handleUndo,
  };
}
