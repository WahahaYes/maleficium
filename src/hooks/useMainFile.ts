import { useCallback, useState, type MutableRefObject } from 'react';
import { emit } from '../lib/events';
import { hasDir } from '../lib/paths';
import { resolveMainFileTauri, setMainFile } from '../lib/mainFile.tauri';

// The open project's main file: what resolved it, the tie-break candidates,
// and the commands that change it.
export function useMainFile({
  root,
  projectId,
  rootRef,
  setLog,
}: {
  root: string | null;
  projectId: string | null;
  rootRef: MutableRefObject<string | null>;
  setLog: (line: string) => void;
}) {
  const [mainFile, setMainFileState] = useState<string | null>(null);
  const [mainSource, setMainSource] = useState('');
  const [mainCandidates, setMainCandidates] = useState<string[]>([]);

  const clearMain = useCallback(() => {
    setMainFileState(null);
    setMainSource('');
    setMainCandidates([]);
  }, []);

  const resolveMain = useCallback(
    async (r: string, rootId: string, opened: string | null) => {
      const res = await resolveMainFileTauri(rootId, opened);
      // A resolve for a root that is no longer open never touches the open
      // project's main file.
      if (r !== rootRef.current) return null;
      setMainFileState(res.mainFile);
      setMainSource(res.source);
      setMainCandidates(res.candidates);
      return res.mainFile;
    },
    [rootRef],
  );

  /** Make the open file the main file. */
  async function setMainToOpenFile(fileName: string) {
    if (!root || !projectId || !hasDir(fileName)) return;
    await setMainFile(projectId, root, fileName);
    const m = await resolveMain(root, projectId, fileName);
    setLog('main file: ' + (m ?? '(none)'));
    emit({
      scope: 'fs',
      kind: 'success',
      actor: 'user',
      message: 'main file set: ' + (m ?? '(none)'),
      event: { action: 'main.set', mainFile: m },
    });
  }

  /** Make `path` the main file (tree double-click or context menu). */
  async function setMainToPath(path: string) {
    if (!root || !projectId) return;
    await setMainFile(projectId, root, path);
    const m = await resolveMain(root, projectId, path);
    setLog('main file: ' + (m ?? '(none)'));
    emit({
      scope: 'fs',
      kind: 'success',
      actor: 'user',
      message: 'main file set: ' + (m ?? '(none)'),
      event: { action: 'main.set', mainFile: m },
    });
  }

  // Main-file tie-break: the scan found >1 `\documentclass` and picked the
  // first. Choosing here writes the explicit association, so the tie never
  // reappears for this project.
  async function pickMain(path: string) {
    if (!root || !projectId) return;
    await setMainFile(projectId, root, path);
    await resolveMain(root, projectId, path);
    emit({
      scope: 'fs',
      kind: 'success',
      actor: 'user',
      message: 'main file set: ' + path,
      event: { action: 'main.set', mainFile: path },
    });
  }

  return {
    mainFile,
    mainSource,
    mainCandidates,
    setMainFileState,
    clearMain,
    resolveMain,
    setMainToOpenFile,
    setMainToPath,
    pickMain,
  };
}
