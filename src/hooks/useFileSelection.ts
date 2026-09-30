import {
  useCallback,
  useRef,
  type Dispatch,
  type MutableRefObject,
  type SetStateAction,
} from 'react';
import { emit } from '../lib/events';
import { fs } from '../lib/fs-provider';
import { getOrCreateBuffer, enforceBufferCap, markSaved, type BufferState } from '../lib/buffers';
import { hasDir } from '../lib/paths';
import { isPreviewable, LARGE_FILE_BYTES, loadTex, saveTex } from '../lib/files';
import type { RecordOutcome } from '../lib/history';

// Switching the open file: persist the one being left, then show a preview,
// a kept buffer, a large-file placeholder, or the file read from disk.
export function useFileSelection({
  fileName,
  fileNameRef,
  buffers,
  setBuffers,
  setTex,
  setFileName,
  setPreviewFile,
  setLargeFile,
  setLog,
  noteSavedRevision,
  resolveMain,
  rootRef,
  projectIdRef,
}: {
  fileName: string;
  fileNameRef: MutableRefObject<string>;
  buffers: Map<string, BufferState>;
  setBuffers: Dispatch<SetStateAction<Map<string, BufferState>>>;
  setTex: (text: string) => void;
  setFileName: (path: string) => void;
  setPreviewFile: (path: string | null) => void;
  setLargeFile: (path: string | null) => void;
  setLog: (line: string) => void;
  noteSavedRevision: (path: string, outcome: RecordOutcome) => Promise<void>;
  resolveMain: (root: string, rootId: string, opened: string | null) => Promise<string | null>;
  rootRef: MutableRefObject<string | null>;
  projectIdRef: MutableRefObject<string | null>;
}) {
  // Latest tree selection wins: rapid clicks resolve out of order otherwise.
  const selectTokenRef = useRef(0);

  const handleSelect = useCallback(
    async (path: string) => {
      // Persist current buffer before switching (dirty survives switch via map).
      if (hasDir(fileName) && path !== fileName) {
        const cur = buffers.get(fileName);
        if (cur?.dirty) {
          try {
            const outcome = await saveTex(fileName, cur.value, cur.disk);
            setBuffers((b) => markSaved(b, fileName));
            await noteSavedRevision(fileName, outcome);
          } catch (e) {
            // The buffer stays dirty; say the save did not happen.
            emit({
              scope: 'fs',
              kind: 'error',
              actor: 'user',
              message: `save failed: ${fileName} (${String(e).slice(0, 120)})`,
              event: {
                action: 'file.save-failed',
                path: fileName,
                trigger: 'switch',
                error: String(e).slice(0, 200),
              },
            });
          }
        }
      }
      const selectToken = ++selectTokenRef.current;
      // Non-text files never enter the editor: rich preview surface instead.
      if (isPreviewable(path)) {
        if (selectToken !== selectTokenRef.current) return; // stale click lost the race
        setPreviewFile(path);
        setFileName(path);
        setLargeFile(null);
        setLog('previewing ' + path);
        emit({
          scope: 'fs',
          kind: 'info',
          actor: 'user',
          message: 'previewing ' + path,
          event: { action: 'file.preview', path },
        });
        return;
      }
      // Reuse preserved buffer without re-reading.
      const kept = buffers.get(path);
      if (kept) {
        if (selectToken !== selectTokenRef.current) return; // stale click lost the race
        setTex(kept.value);
        setFileName(path);
        setPreviewFile(null);
        setLargeFile(null);
        setLog('switched ' + path + (kept.dirty ? ' (unsaved changes)' : ''));
        emit({
          scope: 'fs',
          kind: 'info',
          actor: 'user',
          message: 'switched ' + path,
          event: { action: 'file.switch', path, dirty: kept.dirty },
        });
        if (rootRef.current && projectIdRef.current && path.endsWith('.tex'))
          void resolveMain(rootRef.current, projectIdRef.current, path);
        return;
      }
      emit({
        scope: 'fs',
        kind: 'progress',
        actor: 'user',
        message: 'loading ' + path,
        event: { action: 'file.load', path },
      });
      setLog('loading ' + path);
      try {
        const info = await fs().stat(path);
        const size = info?.size ?? 0;
        if (size > LARGE_FILE_BYTES) {
          if (selectToken !== selectTokenRef.current) return; // stale click lost the race
          setLargeFile(path);
          setFileName(path);
          setLog(`large file (${Math.round(size / 1024)}KB) — preview only`);
          emit({
            scope: 'fs',
            kind: 'warn',
            actor: 'user',
            message: `large file placeholder ${path} (${size}B)`,
            event: { action: 'file.too-large', path, bytes: size },
          });
          return;
        }
        setLargeFile(null);
        setPreviewFile(null);
        const content = await loadTex(path);
        if (selectToken !== selectTokenRef.current) return; // stale load: drop, keep newest
        setBuffers((b) => {
          const n = new Map(b);
          getOrCreateBuffer(n, path, content);
          return enforceBufferCap(n, fileNameRef.current);
        });
        setTex(content);
        setFileName(path);
        setLog('loaded ' + path);
        emit({
          scope: 'fs',
          kind: 'success',
          actor: 'user',
          message: 'loaded ' + path,
          event: { action: 'file.open', path, chars: content.length },
        });
        if (rootRef.current && projectIdRef.current && path.endsWith('.tex'))
          void resolveMain(rootRef.current, projectIdRef.current, path);
      } catch (e) {
        setLog('load failed: ' + String(e).slice(0, 120));
        emit({
          scope: 'fs',
          kind: 'error',
          actor: 'user',
          message: 'load failed ' + path,
          event: { action: 'file.load-failed', path, error: String(e).slice(0, 200) },
        });
      }
    },
    [
      buffers,
      setBuffers,
      fileName,
      noteSavedRevision,
      resolveMain,
      setTex,
      setFileName,
      setPreviewFile,
      setLargeFile,
      setLog,
      fileNameRef,
      rootRef,
      projectIdRef,
    ],
  );
  return handleSelect;
}
