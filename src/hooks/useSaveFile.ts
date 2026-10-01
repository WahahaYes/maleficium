import { useCallback, useEffect, type Dispatch, type SetStateAction } from 'react';
import { emit } from '../lib/events';
import { markSaved, type BufferState } from '../lib/buffers';
import { hasDir } from '../lib/paths';
import { saveTex, saveTexToDisk } from '../lib/files';
import type { RecordOutcome } from '../lib/history';

// Saving the open file: Ctrl+S, and the autosave that follows an edit.
// Returns the manual save.
export function useSaveFile({
  fileName,
  tex,
  buffers,
  setBuffers,
  largeFile,
  noteSavedRevision,
  setLog,
}: {
  fileName: string;
  tex: string;
  buffers: Map<string, BufferState>;
  setBuffers: Dispatch<SetStateAction<Map<string, BufferState>>>;
  largeFile: string | null;
  noteSavedRevision: (path: string, outcome: RecordOutcome) => Promise<void>;
  setLog: (line: string) => void;
}) {
  const save = useCallback(async () => {
    if (largeFile) {
      setLog('save blocked: large placeholder file is not loaded');
      emit({
        scope: 'fs',
        kind: 'warn',
        actor: 'user',
        message: 'save blocked for large placeholder ' + largeFile,
        event: { action: 'file.save-blocked', path: largeFile, reason: 'large-placeholder' },
      });
      return;
    }
    try {
      if (hasDir(fileName)) {
        const cur = buffers.get(fileName);
        const text = cur?.value ?? tex;
        const outcome = await saveTex(fileName, text, cur?.disk);
        setBuffers((b) => markSaved(b, fileName));
        await noteSavedRevision(fileName, outcome);
        setLog('saved ' + fileName);
        emit({
          scope: 'fs',
          kind: 'success',
          actor: 'user',
          message: 'saved ' + fileName,
          event: { action: 'file.save', path: fileName, chars: text.length, mode: 'manual' },
        });
      } else {
        await saveTexToDisk(fileName, tex);
        setLog('saved ' + fileName);
        emit({
          scope: 'fs',
          kind: 'success',
          actor: 'user',
          message: 'saved ' + fileName,
          event: { action: 'file.save', path: fileName, chars: tex.length, mode: 'untitled' },
        });
      }
    } catch (e) {
      // Refused (held for a conflict, or the file changed on disk) or failed:
      // the buffer stays dirty, and the user is told instead of nothing happening.
      setLog('save failed: ' + fileName);
      emit({
        scope: 'fs',
        kind: 'error',
        actor: 'user',
        message: `save failed: ${fileName} (${String(e).slice(0, 120)})`,
        event: {
          action: 'file.save-failed',
          path: fileName,
          trigger: 'manual',
          error: String(e).slice(0, 200),
        },
      });
    }
  }, [fileName, tex, buffers, setBuffers, largeFile, noteSavedRevision, setLog]);

  useEffect(() => {
    if (!hasDir(fileName)) return;
    const t = setTimeout(() => {
      const cur = buffers.get(fileName);
      if (cur?.dirty) {
        saveTex(fileName, cur.value, cur.disk)
          .then(async (outcome) => {
            setBuffers((b) => markSaved(b, fileName));
            await noteSavedRevision(fileName, outcome);
            emit({
              scope: 'fs',
              kind: 'info',
              actor: 'system',
              message: 'autosaved ' + fileName,
              event: { action: 'file.save', path: fileName, chars: cur.value.length, mode: 'auto' },
            });
            setLog('autosaved ' + new Date().toTimeString().slice(0, 8));
          })
          .catch((e) => {
            // The dirty flag stays, so the next edit retries.
            emit({
              scope: 'fs',
              kind: 'error',
              actor: 'system',
              message: `save failed: ${fileName} (${String(e).slice(0, 120)})`,
              event: {
                action: 'file.save-failed',
                path: fileName,
                trigger: 'auto',
                error: String(e).slice(0, 200),
              },
            });
          });
      }
    }, 1200);
    return () => clearTimeout(t);
  }, [tex, fileName, buffers, setBuffers, noteSavedRevision, setLog]);

  return save;
}
