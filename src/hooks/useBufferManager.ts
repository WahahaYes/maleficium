// useBufferManager.ts — open text buffers and their close lifecycle.
//
// Persist-then-evict: closing a dirty buffer writes it first, and a file that
// fails to persist stays open and aborts a batch close. Buffer contents are
// App-wide state, so the map and its setter are returned rather than hidden.

import { useRef, useState } from 'react';
import type { BufferState } from '../lib/buffers';
import { emit } from '../lib/events';
import { saveTex } from '../lib/files';

export interface UseBufferManagerDeps {
  fileName: string;
  setFileName: (v: string) => void;
  previewFile: string | null;
  setPreviewFile: (v: string | null) => void;
  setTex: (v: string) => void;
  setLargeFile: (v: string | null) => void;
  setReloadPath: (v: string | null) => void;
  markOwnWrite: (p: string) => void;
}

export function useBufferManager(deps: UseBufferManagerDeps) {
  const {
    fileName,
    setFileName,
    previewFile,
    setPreviewFile,
    setTex,
    setLargeFile,
    setReloadPath,
    markOwnWrite,
  } = deps;

  const [buffers, setBuffers] = useState<Map<string, BufferState>>(new Map());
  const buffersRef = useRef(buffers);
  buffersRef.current = buffers;

  async function handleCloseBuffer(path: string) {
    // Persist-then-evict: close never loses work silently.
    if (path === fileName && fileName.includes('/')) {
      const cur = buffers.get(fileName);
      if (cur?.dirty) {
        try {
          await saveTex(fileName, cur.value);
          markOwnWrite(fileName);
        } catch {
          /* keep dirty, still evict? no — stay */ return;
        }
      }
    }
    await closeBufferQuiet(path);
    emit({
      scope: 'fs',
      kind: 'info',
      actor: 'user',
      message: 'closed ' + path,
      event: { action: 'file.close', path },
    });
  }

  // Close without emitting (batch callers emit once for the batch).
  async function closeBufferQuiet(path: string): Promise<boolean> {
    if (path === fileName && fileName.includes('/')) {
      const cur = buffers.get(fileName);
      if (cur?.dirty) {
        try {
          await saveTex(fileName, cur.value);
          markOwnWrite(fileName);
        } catch {
          /* persist failed — stay open */ return false;
        }
      }
    }
    setBuffers((b) => {
      const n = new Map(b);
      n.delete(path);
      return n;
    });
    if (path === fileName) {
      // Fall through to nearest remaining buffer (keeps editor populated).
      const rest = [...buffers.keys()].filter((k) => k !== path);
      if (rest.length > 0) {
        const next = buffers.get(rest[rest.length - 1]);
        if (next) {
          setTex(next.value);
          setFileName(rest[rest.length - 1]);
          setPreviewFile(null);
          setLargeFile(null);
          setReloadPath(null);
        }
      } else if (previewFile === path) {
        setPreviewFile(null);
      }
    } else if (previewFile === path) {
      setPreviewFile(null);
    }
    return true;
  }

  // Close-all / close-others (persist-then-evict per file; dirty-never-lost:
  // a file that fails to persist stays open and aborts the batch).
  async function handleCloseOthers(keep: string) {
    const paths = [...buffers.keys()].filter((k) => k !== keep);
    let n = 0;
    for (const p of paths) {
      if (await closeBufferQuiet(p)) n++;
      else break;
    }
    emit({
      scope: 'fs',
      kind: 'info',
      actor: 'user',
      message: `closed ${n} other file${n === 1 ? '' : 's'}`,
      event: { action: 'file.close-many', count: n, kept: fileName },
    });
  }

  async function handleCloseAll() {
    const paths = [...buffers.keys()];
    let n = 0;
    for (const p of paths) {
      if (await closeBufferQuiet(p)) n++;
      else break;
    }
    emit({
      scope: 'fs',
      kind: 'info',
      actor: 'user',
      message: `closed ${n} file${n === 1 ? '' : 's'}`,
      event: { action: 'file.close-many', count: n },
    });
  }

  return {
    buffers,
    setBuffers,
    buffersRef,
    handleCloseBuffer,
    handleCloseOthers,
    handleCloseAll,
  };
}
