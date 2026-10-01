import {
  useCallback,
  useEffect,
  useState,
  type Dispatch,
  type MutableRefObject,
  type SetStateAction,
} from 'react';
import { emit } from '../lib/events';
import { historyStore } from '../lib/history';
import { joinPath } from '../lib/paths';
import { projectIndex, type Query } from '../lib/project-index';
import { updateBuffer, type BufferState } from '../lib/buffers';

// Project-wide search and replace, plus the one-step undo of the last replace.
export function useProjectReplace({
  projectId,
  root,
  mainFile,
  relInProject,
  buffersRef,
  setBuffers,
  setTex,
  fileNameRef,
}: {
  projectId: string | null;
  root: string | null;
  mainFile: string | null;
  relInProject: (abs: string) => string | null;
  buffersRef: MutableRefObject<Map<string, BufferState>>;
  setBuffers: Dispatch<SetStateAction<Map<string, BufferState>>>;
  setTex: (text: string) => void;
  fileNameRef: MutableRefObject<string>;
}) {
  const runSearch = useCallback(
    (q: Query) => {
      if (!projectId) return Promise.reject(new Error('no project open'));
      const main = mainFile ? relInProject(mainFile) : null;
      return projectIndex().search(projectId, q, main);
    },
    [projectId, mainFile, relInProject],
  );
  // The last replace, undoable in one step while it stands.
  const [lastReplace, setLastReplace] = useState<{
    batch: string;
    replacements: number;
    files: number;
  } | null>(null);
  const runPreview = useCallback(
    (q: Query, replacement: string) => {
      if (!projectId) return Promise.reject(new Error('no project open'));
      const main = mainFile ? relInProject(mainFile) : null;
      return projectIndex().replacePreview(projectId, q, replacement, main);
    },
    [projectId, mainFile, relInProject],
  );
  // Open buffers take their new text in place (undoable in the editor too);
  // closed files are written by the core. One history batch covers both.
  const runApply = useCallback(
    async (token: string) => {
      if (!projectId || !root) throw new Error('no project open');
      const keepOpen = [...buffersRef.current.keys()]
        .map((abs) => relInProject(abs))
        .filter((r): r is string => r != null);
      try {
        const res = await projectIndex().replaceApply(projectId, token, keepOpen);
        for (const e of res.edits) {
          const abs = joinPath(root, e.rel);
          setBuffers((b) => updateBuffer(b, abs, e.text));
          if (abs === fileNameRef.current) setTex(e.text);
        }
        const files = res.written.length + res.edits.length;
        setLastReplace({ batch: res.batch, replacements: res.replacements, files });
        emit({
          scope: 'fs',
          kind: 'success',
          actor: 'user',
          message: `replaced ${res.replacements} in ${files} files`,
          event: {
            action: 'replace.apply',
            files,
            replacements: res.replacements,
            batch: res.batch,
          },
        });
        return res;
      } catch (e) {
        emit({
          scope: 'fs',
          kind: 'warn',
          actor: 'user',
          message: 'replace refused: ' + String(e).slice(0, 120),
          event: { action: 'replace.failed', error: String(e).slice(0, 200) },
        });
        throw e;
      }
    },
    [projectId, root, buffersRef, relInProject, setBuffers, setTex, fileNameRef],
  );
  const undoReplace = useCallback(async () => {
    if (!projectId || !root || !lastReplace) return;
    const { batch } = lastReplace;
    const files = await historyStore().batchFiles(projectId, batch);
    let restored = 0;
    for (const f of files) {
      const abs = joinPath(root, f.rel);
      if (buffersRef.current.has(abs)) {
        const bytes = await historyStore().getRevision(projectId, f.rel, f.rev);
        if (!bytes) continue;
        const text = new TextDecoder().decode(bytes);
        setBuffers((b) => updateBuffer(b, abs, text));
        if (abs === fileNameRef.current) setTex(text);
        restored += 1;
      } else if (await historyStore().restoreRevision(projectId, f.rel, f.rev)) {
        restored += 1;
      }
    }
    setLastReplace(null);
    if (restored === files.length && files.length > 0) {
      emit({
        scope: 'fs',
        kind: 'success',
        actor: 'user',
        message: `undid replace in ${restored} files`,
        event: { action: 'replace.undo', batch, files: restored },
      });
    } else {
      emit({
        scope: 'fs',
        kind: 'warn',
        actor: 'user',
        message: `undo replace restored ${restored} of ${files.length} files`,
        event: {
          action: 'replace.failed',
          error: `restored ${restored} of ${files.length} files of ${batch}`,
        },
      });
    }
  }, [projectId, root, lastReplace, buffersRef, setBuffers, setTex, fileNameRef]);
  // A replace belongs to the project it ran in.
  useEffect(() => setLastReplace(null), [projectId]);
  return { lastReplace, runSearch, runPreview, runApply, undoReplace };
}
