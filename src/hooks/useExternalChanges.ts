// useExternalChanges.ts — keep every open buffer honest about its file.
//
// Checked on each watch batch and whenever the window regains focus: a clean
// buffer takes the disk content quietly, a dirty one raises a conflict the
// user resolves (reload, or keep their edits). Writes to a conflicted path
// are held until then.

import { useCallback, useEffect, useRef, useState } from 'react';
import { acknowledgeDisk, reloadBuffer, type BufferState } from '../lib/buffers';
import { emit } from '../lib/events';
import { decideExternal, holdWrites } from '../lib/externalChange';
import { loadTex } from '../lib/files';
import type { OwnWrites } from '../lib/own-writes';

export interface UseExternalChangesDeps {
  buffers: Map<string, BufferState>;
  setBuffers: React.Dispatch<React.SetStateAction<Map<string, BufferState>>>;
  fileName: string;
  setTex: (v: string) => void;
  ownWrites: OwnWrites;
}

export function useExternalChanges(deps: UseExternalChangesDeps) {
  const { buffers, setBuffers, fileName, setTex, ownWrites } = deps;
  const [conflicts, setConflicts] = useState<string[]>([]);
  const live = useRef({ buffers, fileName });
  live.current = { buffers, fileName };

  const check = useCallback(
    async (paths: readonly string[]) => {
      for (const path of paths) {
        const buf = live.current.buffers.get(path);
        if (!buf) continue;
        if (await ownWrites.isEcho(path)) continue;
        let onDisk: string;
        try {
          onDisk = await loadTex(path);
        } catch {
          continue; // gone or unreadable: the delete path reports it
        }
        const now = live.current.buffers.get(path);
        if (!now) continue;
        const action = decideExternal(now, onDisk);
        if (action === 'none') continue;
        if (action === 'sync') {
          setBuffers((b) => acknowledgeDisk(b, path, onDisk));
        } else if (action === 'reload') {
          setBuffers((b) => reloadBuffer(b, path, onDisk));
          if (path === live.current.fileName) setTex(onDisk);
          emit({
            scope: 'fs',
            kind: 'info',
            actor: 'system',
            message: `reloaded ${path} (changed on disk, no unsaved edits)`,
            event: { action: 'file.reload', path, chars: onDisk.length },
          });
        } else {
          holdWrites(path, true);
          setConflicts((c) => (c.includes(path) ? c : [...c, path]));
          emit({
            scope: 'fs',
            kind: 'warn',
            actor: 'system',
            message: `changed on disk with unsaved edits: ${path}`,
            event: { action: 'file.external-conflict', path },
          });
        }
      }
    },
    [ownWrites, setBuffers, setTex],
  );

  // Missed or unavailable watch events: re-check everything open on focus.
  useEffect(() => {
    const onFocus = () => void check([...live.current.buffers.keys()]);
    window.addEventListener('focus', onFocus);
    return () => window.removeEventListener('focus', onFocus);
  }, [check]);

  // A conflict whose buffer closed or no longer differs has nothing to decide.
  useEffect(() => {
    const stale = conflicts.filter((p) => !buffers.get(p)?.dirty);
    if (stale.length === 0) return;
    for (const p of stale) holdWrites(p, false);
    setConflicts((c) => c.filter((p) => !stale.includes(p)));
  }, [buffers, conflicts]);

  /** Resolve the oldest open conflict. */
  const resolve = useCallback(
    async (choice: 'reload' | 'keep') => {
      const path = conflicts[0];
      if (!path) return;
      let onDisk: string;
      try {
        onDisk = await loadTex(path);
      } catch (e) {
        emit({
          scope: 'fs',
          kind: 'error',
          actor: 'user',
          message: 'reload failed: ' + String(e).slice(0, 120),
          event: { action: 'file.reload-failed', path, error: String(e).slice(0, 200) },
        });
        return;
      }
      holdWrites(path, false);
      setConflicts((c) => c.filter((p) => p !== path));
      if (choice === 'reload') {
        setBuffers((b) => reloadBuffer(b, path, onDisk));
        if (path === live.current.fileName) setTex(onDisk);
        emit({
          scope: 'fs',
          kind: 'success',
          actor: 'user',
          message: 'reloaded ' + path,
          event: { action: 'file.reload', path, chars: onDisk.length },
        });
      } else {
        setBuffers((b) => acknowledgeDisk(b, path, onDisk));
        emit({
          scope: 'fs',
          kind: 'info',
          actor: 'user',
          message: `kept your edits to ${path}; saving will replace the disk version`,
          event: { action: 'file.keep-mine', path },
        });
      }
    },
    [conflicts, setBuffers, setTex],
  );

  return { conflicts, checkExternal: check, resolveExternal: resolve };
}
