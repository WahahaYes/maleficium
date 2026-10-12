import { useCallback, useRef, useState, type MutableRefObject, type RefObject } from 'react';
import type { EditorViewportHandle } from '../components/EditorViewport';
import { hoverText } from '../lib/definition.view';
import { emit } from '../lib/events';
import type { Hit } from '../lib/generated/index';
import { joinPath } from '../lib/paths';
import { projectIndex, type ProjectMacro } from '../lib/project-index';

/** A range to select in the editor once `path` is the active file. */
export interface EditorReveal {
  path: string;
  line: number;
  col: number;
  len: number;
  key: number;
}

// Taking the user to a place in the project: go to definition (F12 /
// Ctrl+click) and its hover over the project index, the file finder,
// search hits, and pre-compile findings. Files switch through the
// handleSelect ref, so this runs before the file-selection hook exists.
export function useNavigation({
  root,
  projectId,
  mainRel,
  fileNameRef,
  selectRef,
  viewportRef,
  setLog,
}: {
  root: string | null;
  projectId: string | null;
  /** The main file, project-relative (resolves \input chains for lookups). */
  mainRel: string | null;
  fileNameRef: MutableRefObject<string>;
  selectRef: MutableRefObject<(path: string) => Promise<void>>;
  viewportRef: RefObject<EditorViewportHandle | null>;
  setLog: (s: string) => void;
}) {
  const [reveal, setReveal] = useState<EditorReveal | null>(null);
  /** Open `abs` (unless active) and select line:col+len there. */
  const revealAt = useCallback(
    async (abs: string, line: number, col = 0, len = 0) => {
      if (abs !== fileNameRef.current) await selectRef.current(abs);
      setReveal({ path: abs, line, col, len, key: Date.now() });
    },
    [fileNameRef, selectRef],
  );

  const lookupAt = useCallback(
    (line: string, col: number) =>
      projectId
        ? projectIndex().definitionAt(projectId, line, col, mainRel)
        : Promise.resolve(null),
    [projectId, mainRel],
  );
  const goToDefinitionAt = useCallback(
    async (line: string, col: number) => {
      if (!root) return;
      const l = await lookupAt(line, col).catch(() => null);
      if (!l) {
        setLog('nothing to go to here');
        return;
      }
      const found = l.definitions.length;
      // A macro with no project definition is built in, not an error.
      const builtin = found === 0 && l.ref.kind === 'macro';
      emit({
        scope: 'app',
        kind: found > 0 || builtin ? 'info' : 'warn',
        actor: 'user',
        message:
          found > 0
            ? `definition of ${l.ref.key}`
            : builtin
              ? `${l.ref.key} is built in or from a package`
              : `no definition for ${l.ref.key}`,
        event: { action: 'nav.definition', kind: l.ref.kind, key: l.ref.key, found },
      });
      const d = l.definitions[0];
      if (!d) {
        setLog(hoverText(l) ?? `${l.ref.key} is built in or from a package`);
        return;
      }
      await revealAt(joinPath(root, d.rel), d.line);
      if (found > 1) setLog(`${l.ref.key}: ${found} definitions (duplicate) — showing the first`);
    },
    [root, lookupAt, revealAt, setLog],
  );
  // The editor's definition bridge (hover, Ctrl+click, macro completion).
  const definitionRef = useRef<{
    hover: (line: string, col: number) => Promise<string | null>;
    go: (line: string, col: number) => void;
    macros: () => Promise<ProjectMacro[]>;
  } | null>(null);
  definitionRef.current = {
    hover: (line, col) =>
      lookupAt(line, col).then(
        (l) => (l ? hoverText(l) : null),
        () => null,
      ),
    go: (line, col) => void goToDefinitionAt(line, col),
    macros: () => (projectId ? projectIndex().macros(projectId) : Promise.resolve([])),
  };
  /** F12 / the menu: go to the definition under the caret. */
  const goToDefinition = () => {
    const at = viewportRef.current?.caretAt();
    if (at) void goToDefinitionAt(at.text, at.col);
  };
  const goToDefinitionRef = useRef(goToDefinition);
  goToDefinitionRef.current = goToDefinition;

  const findFiles = useCallback(
    (query: string) =>
      projectId ? projectIndex().findFiles(projectId, query) : Promise.resolve([]),
    [projectId],
  );
  const rankNames = useCallback(
    (query: string, items: string[]) => projectIndex().rank(query, items),
    [],
  );
  const openFileRel = useCallback(
    (rel: string) => {
      if (root) void selectRef.current(joinPath(root, rel));
    },
    [root, selectRef],
  );
  const openHit = useCallback(
    (rel: string, hit: Hit) => {
      if (root) void revealAt(joinPath(root, rel), hit.line, hit.col, hit.len);
    },
    [root, revealAt],
  );
  const openFinding = useCallback(
    (rootPath: string, rel: string, line: number) => void revealAt(joinPath(rootPath, rel), line),
    [revealAt],
  );

  return {
    reveal,
    definitionRef,
    goToDefinitionRef,
    findFiles,
    rankNames,
    openFileRel,
    openHit,
    openFinding,
  };
}
