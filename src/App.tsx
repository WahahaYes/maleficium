import { useCallback, useEffect, useRef, useState } from 'react';
import Box from '@mui/material/Box';
import Typography from '@mui/material/Typography';
import CompileButton from './components/CompileButton';
import MenuBar from './components/MenuBar';
import LogStream, { type ProblemRef } from './components/LogStream';
import { baseName, dirName, hasDir, joinPath, relUnder } from './lib/paths';
import { useDevCamera } from './hooks/useDevCamera';
import { type EditorViewportHandle } from './components/EditorViewport';
import EditorPane from './components/EditorPane';
import PreviewPane from './components/PreviewPane';
import SideColumn from './components/SideColumn';
import WorkArea from './components/WorkArea';
import { mainFileTip } from './lib/mainFileTip';
import { AboutDialog, GoToLineDialog, RenameDialog } from './components/SimpleDialogs';
import PaletteDialog from './components/PaletteDialog';
import { paletteCommands } from './lib/palette';
import { hoverText } from './lib/definition.view';
import { projectIndex } from './lib/project-index';
import type { Hit } from './lib/generated/index';
import HistoryDialog from './components/HistoryDialog';
import ShortcutsDialog from './components/ShortcutsDialog';
import SettingsDialog from './components/SettingsDialog';
import type { AppearancePrefs } from './lib/appearance';
import StatusBar from './components/StatusBar';
import PrecheckPanel from './components/PrecheckPanel';
import ExternalChangeDialog from './components/ExternalChangeDialog';
import { useExternalChanges } from './hooks/useExternalChanges';
import { listDir1Level, loadTex, saveTex, saveTexToDisk, TreeEntry } from './lib/files';
import { getOrCreateBuffer, updateBuffer, markSaved, enforceBufferCap } from './lib/buffers';
import { cancelCompile, compileLogTitle } from './lib/compile';
import { onPdf, sourceFor, type PreviewDoc } from './lib/preview-bus';
import { emit } from './lib/events';
import { startEventLog } from './lib/eventlog';
import { historyAvailability } from './lib/history.view';
import { structure } from './lib/structure';
import type { OutlineEntry } from './lib/generated/structure';
import type { ZoomAction } from './lib/zoom';
import { useExport } from './hooks/useExport';
import TemplateDialogs, { type TemplateDialogMode } from './components/TemplateDialogs';
import { buildMenus, type CommandActions, type MenuContext } from './lib/commands';
import { FileHistory } from './lib/file-history';
import { pruneRecentProjects } from './lib/recentProjects';
import { grantUntitledAccess } from './lib/projectAccess';
import { useProjectReplace } from './hooks/useProjectReplace';
import { useFileSelection } from './hooks/useFileSelection';
import { useMainFile } from './hooks/useMainFile';
import { useGlobalKeymap } from './hooks/useGlobalKeymap';
import { useShellLayout } from './hooks/useShellLayout';
import { useBufferManager } from './hooks/useBufferManager';
import { useCompileRunner } from './hooks/useCompileRunner';
import { useProjectTree } from './hooks/useProjectTree';
import { useFileOps } from './hooks/useFileOps';
import { useRevisionHistory } from './hooks/useRevisionHistory';
import { useIndexOverlays } from './hooks/useIndexOverlays';
import { useSynctex } from './hooks/useSynctex';

const HELLO = '\\documentclass{article}\n\\begin{document}\nHello Maleficium\n\\end{document}\n';

export default function App({
  themeMode = 'dark',
  onThemeMode = () => {},
  density = 'comfortable',
  onDensityMode = () => {},
  prefs,
  onPrefs = () => {},
}: {
  themeMode?: 'dark' | 'light';
  onThemeMode?: (m: 'dark' | 'light') => void;
  density?: 'comfortable' | 'compact';
  onDensityMode?: (d: 'comfortable' | 'compact') => void;
  prefs: AppearancePrefs;
  onPrefs?: (p: AppearancePrefs) => void;
}) {
  const shell = useShellLayout();
  const { toggleTree } = shell;
  const [tex, setTex] = useState(HELLO);
  const [root, setRoot] = useState<string | null>(null);
  const [projectId, setProjectId] = useState<string | null>(null);
  const [tree, setTree] = useState<TreeEntry[]>([]);
  const [fileName, setFileName] = useState('hello.tex');
  const [trash] = useState(() => new FileHistory());

  const [log, setLog] = useState('ready');
  const [largeFile, setLargeFile] = useState<string | null>(null);
  // Non-text selection (image/video/pdf/binary): rich preview, never the editor.
  const [previewFile, setPreviewFile] = useState<string | null>(null);
  const [previewDoc, setPreviewDoc] = useState<PreviewDoc | null>(null);
  const pdfUrl = previewDoc?.url ?? null;
  // Ref mirror: the subscribe-once listener reads forward SyncTeX via ref,
  // never state.
  // Bumped on every inverse SyncTeX hit to flash the line amber.
  // Ref mirror for the watcher closure (the effect is root-scoped).
  const fileNameRef = useRef(fileName);
  fileNameRef.current = fileName;
  // Latest closures for the subscribe-once global keymap listener.
  const compileRef = useRef<() => Promise<void>>(async () => {});
  const forwardSyncRef = useRef<() => Promise<void>>(async () => {});
  const forwardSyncLineRef = useRef<(file: string, line: number) => void>(() => {});
  const handleSelectRef = useRef<(path: string) => Promise<void>>(async () => {});
  const { buffers, setBuffers, buffersRef, handleCloseBuffer, handleCloseOthers, handleCloseAll } =
    useBufferManager({
      fileName,
      setFileName,
      previewFile,
      setPreviewFile,
      setTex,
      emptyTex: HELLO,
      setLargeFile,
    });
  const { conflicts, checkExternal, resolveExternal } = useExternalChanges({
    buffers,
    setBuffers,
    fileName,
    setTex,
  });

  // Revision history: app-local, keyed by the backend-minted project id.
  const rootRef = useRef<string | null>(root);
  rootRef.current = root;
  // Project switches move the ref at once, so async work started for the
  // old root can tell it is stale before the next render.
  const setRootNow = useCallback((v: string | null) => {
    rootRef.current = v;
    setRoot(v);
  }, []);
  const projectIdRef = useRef<string | null>(projectId);
  projectIdRef.current = projectId;
  const setProjectIdNow = useCallback((v: string | null) => {
    projectIdRef.current = v;
    setProjectId(v);
  }, []);

  /** Project-relative path for a file inside the open project, else null. */
  const relInProject = useCallback((abs: string): string | null => {
    const r = rootRef.current;
    return r ? relUnder(r, abs) : null;
  }, []);
  const {
    revisionCount,
    historyOpen,
    setHistoryOpen,
    historyRows,
    historyAvail,
    historySummary,
    historyNotice,
    restoringRev,
    noteSavedRevision,
    openHistory,
    restoreRevision,
  } = useRevisionHistory({
    root,
    projectId,
    relInProject,
    fileName,
    setBuffers,
    setTex,
    setLog,
  });

  useIndexOverlays(projectId, buffers, relInProject);

  // The bus is recorded to an app-local JSONL file for the length of the run.
  useEffect(() => {
    const log = startEventLog();
    return () => log.stop();
  }, []);

  useEffect(() => onPdf(setPreviewDoc), []);

  const {
    mainFile,
    mainSource,
    mainCandidates,
    setMainFileState,
    clearMain,
    resolveMain,
    setMainToOpenFile,
    setMainToPath,
    pickMain,
  } = useMainFile({ root, projectId, rootRef, setLog });
  const handleSelect = useFileSelection({
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
  });
  // Callers defined before handleSelect (tab cycling, go to definition,
  // search hits) switch files through this ref.
  handleSelectRef.current = handleSelect;
  // Tree-driven main association (double-click / context menu on a .tex row).
  const selectAndSetMain = async (path: string) => {
    await handleSelect(path);
    await setMainToPath(path);
  };

  // UI tree is 1 level + expand-on-demand. The recursive walk runs only
  // for main-file scan + watcher baseline, never on the open path.
  // Tree CRUD: create/rename via plugin-fs; own-write marks suppress echoes.

  // One-off compile of a tree-selected file. This is EXPECTED to fail for
  // fragments (a chapter without \documentclass cannot build alone) — the
  // engine error is the honest answer, surfaced through the normal failure
  // path (phase + stream + click-to-jump rows).

  const menuActionRef = useRef<(id: string) => void>(() => {});
  const zoomActionRef = useRef<((a: ZoomAction) => void) | null>(null);
  // Double-click in the editor = forward SyncTeX from the caret line
  // (complements single-click inverse on the PDF canvas).
  useGlobalKeymap(
    {
      compile: () => void compileRef.current(),
      goToDefinition: () => goToDefinitionRef.current(),
      forwardSync: () => void forwardSyncRef.current(),
      zoom: (z) => zoomActionRef.current?.(z),
      menuAction: (id) => menuActionRef.current(id),
      showShortcuts: () => setShortcutsOpen(true),
      toggleTree,
      select: (path) => void handleSelectRef.current(path),
    },
    buffersRef,
    fileNameRef,
  );

  // Untitled documents resolve against the backend-owned scratch root.
  const [scratch, setScratch] = useState<{ rootId: string; path: string } | null>(null);
  useEffect(() => {
    void grantUntitledAccess().then((g) => {
      if (g.path && g.rootId) setScratch({ rootId: g.rootId, path: g.path });
    });
  }, []);
  const workdirHint = hasDir(fileName) ? dirName(fileName) : (scratch?.path ?? '');
  const mainDir = mainFile ? dirName(mainFile) : workdirHint;
  // Repo-relative for display (absolute kept in tooltips); plain language.
  const relOf = (abs: string | null): string | null => {
    if (!abs) return null;
    return (root ? relUnder(root, abs) : null) ?? abs;
  };

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
  }, [fileName, tex, buffers, setBuffers, largeFile, noteSavedRevision]);

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
  }, [tex, fileName, buffers, setBuffers, noteSavedRevision]);

  // Publish engine-log problems as first-class stream events (click-to-jump).
  /** One tree level on expand; an unreadable folder says so and lists empty. */
  async function expandDir(dir: string): Promise<TreeEntry[]> {
    try {
      return await listDir1Level(dir);
    } catch (e) {
      emit({
        scope: 'fs',
        kind: 'error',
        actor: 'user',
        message: 'could not list ' + dir,
        event: { action: 'tree.load-failed', dir, error: String(e).slice(0, 200) },
      });
      return [];
    }
  }

  /** Jump to a root-relative problem location inside a granted root. */
  function handleProblemJump(t: ProblemRef) {
    const base = t.rootId === projectId ? root : t.rootId === scratch?.rootId ? scratch.path : null;
    if (base) handleJump(joinPath(base, t.path), t.line);
  }

  function handleJump(absPath: string, line: number) {
    loadTex(absPath).then((content) => {
      setBuffers((b) => {
        const n = new Map(b);
        getOrCreateBuffer(n, absPath, content);
        return enforceBufferCap(n, fileNameRef.current);
      });
      setTex(content);
      setFileName(absPath);
      setPreviewFile(null);
      setLargeFile(null);
      setCurrentLine(line);
    });
  }

  // Latest-closure refs for the subscribe-once keymap listener. The listener
  // never closes over render state: assign every render and call only
  // `*.current()`. Untitled typing updates `tex` alone, so a dep-driven
  // listener would never resubscribe.
  const { logCollapsed, setLogCollapsed, layout } = shell;
  const {
    compilePhase,
    compileTimer,
    offline,
    compiled,
    progress,
    autoCompile,
    setAutoCompile,
    makeOffline,
    warmCompile,
    handleCompileFile,
    precheck,
    precheckOpen,
    openPrecheck,
    closePrecheck,
    precheckPopup,
    setPrecheckPopup,
  } = useCompileRunner({
    tex,
    fileName,
    mainFile,
    setMainFileState,
    mainDir,
    workdirHint,
    root,
    projectId,
    scratch,
    buffers,
    setBuffers,
    largeFile,
    previewFile,
    setLog,
    setLogCollapsed,
    compileRef,
  });
  const { reloadTree, openRoot, open, openWelcome, recentProjects, setRecentProjects } =
    useProjectTree({
      root,
      projectId,
      setRoot: setRootNow,
      setProjectId: setProjectIdNow,
      setTree,
      fileNameRef,
      checkExternal,
      setLog,
      trash,
      resolveMain,
      clearMainFile: clearMain,
      handleSelect,
      warmCompile,
    });
  const { handleCreate, handleRename, handleDelete, handleClean, handleUndo } = useFileOps({
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
    reloadTree,
    handleSelect,
  });
  const [shortcutsOpen, setShortcutsOpen] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  // Project search replaces the tree + outline in the side column while open.
  const [searchOpen, setSearchOpen] = useState(false);
  const [searchFocus, setSearchFocus] = useState(0);
  const [hitSelect, setHitSelect] = useState<{
    path: string;
    line: number;
    col: number;
    len: number;
    key: number;
  } | null>(null);
  // Go to definition (F12 / Ctrl+click) and its hover, over the project index.
  const lookupAt = useCallback(
    (line: string, col: number) => {
      if (!projectId) return Promise.resolve(null);
      const main = mainFile ? relInProject(mainFile) : null;
      return projectIndex().definitionAt(projectId, line, col, main);
    },
    [projectId, mainFile, relInProject],
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
      emit({
        scope: 'app',
        kind: found > 0 ? 'info' : 'warn',
        actor: 'user',
        message: found > 0 ? `definition of ${l.ref.key}` : `no definition for ${l.ref.key}`,
        event: { action: 'nav.definition', kind: l.ref.kind, key: l.ref.key, found },
      });
      const d = l.definitions[0];
      if (!d) {
        setLog(hoverText(l));
        return;
      }
      const abs = joinPath(root, d.rel);
      if (abs !== fileNameRef.current) await handleSelectRef.current(abs);
      setHitSelect({ path: abs, line: d.line, col: 0, len: 0, key: Date.now() });
      if (found > 1) setLog(`${l.ref.key}: ${found} definitions (duplicate) — showing the first`);
    },
    [root, lookupAt],
  );
  const definitionRef = useRef<{
    hover: (line: string, col: number) => Promise<string | null>;
    go: (line: string, col: number) => void;
  } | null>(null);
  definitionRef.current = {
    hover: (line, col) =>
      lookupAt(line, col).then(
        (l) => (l ? hoverText(l) : null),
        () => null,
      ),
    go: (line, col) => void goToDefinitionAt(line, col),
  };
  const goToDefinitionRef = useRef<() => void>(() => {});
  goToDefinitionRef.current = () => {
    const at = viewportRef.current?.caretAt();
    if (at) void goToDefinitionAt(at.text, at.col);
  };
  // File finder / command palette: one dialog, `>` switches to commands.
  const [paletteOpen, setPaletteOpen] = useState<string | null>(null);
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
      if (root) void handleSelectRef.current(joinPath(root, rel));
    },
    [root],
  );
  const { lastReplace, runSearch, runPreview, runApply, undoReplace } = useProjectReplace({
    projectId,
    root,
    mainFile,
    relInProject,
    buffersRef,
    setBuffers,
    setTex,
    fileNameRef,
  });
  const openHit = useCallback(
    async (rel: string, hit: Hit) => {
      if (!root) return;
      const abs = joinPath(root, rel);
      if (abs !== fileNameRef.current) await handleSelectRef.current(abs);
      setHitSelect({ path: abs, line: hit.line, col: hit.col, len: hit.len, key: Date.now() });
    },
    [root],
  );
  const openFinding = useCallback(async (rootPath: string, rel: string, line: number) => {
    const abs = joinPath(rootPath, rel);
    if (abs !== fileNameRef.current) await handleSelectRef.current(abs);
    setHitSelect({ path: abs, line, col: 0, len: 0, key: Date.now() });
  }, []);
  const [aboutOpen, setAboutOpen] = useState(false);
  const [goToOpen, setGoToOpen] = useState(false);
  // Rename dialog for the active file.
  const [renameOpen, setRenameOpen] = useState(false);
  // Viewport bridge assigned via viewportRef prop. Without it
  // selectAll/expand/shrink/goToLine no-op.
  const viewportRef = useRef<EditorViewportHandle | null>(null);
  const {
    currentLine,
    setCurrentLine,
    synctexFlash,
    pageNumber,
    setPageNumber,
    handleForwardSync,
    handleInverseSync,
  } = useSynctex({
    pdfUrl,
    source: previewDoc?.source ?? null,
    compilePhase,
    fileName,
    workdirHint,
    viewportRef,
    fileNameRef,
    setBuffers,
    setTex,
    setFileName,
    setPreviewFile,
    setLargeFile,
    forwardSyncRef,
    forwardSyncLineRef,
  });
  // Outline: active buffer only, debounced 500ms. Full-fidelity entries
  // (parse cap 1000); the view slices to 100 per filter (filter-first,
  // cap-second). Sections-only rows feed the Selection submenus.
  const [outline, setOutline] = useState<OutlineEntry[]>([]);
  // Multi-pick set for Selection > Pick Sections.
  const [outlinePicks, setOutlinePicks] = useState<number[]>([]);
  useEffect(() => {
    // A reply for a superseded buffer is dropped: parses resolve async.
    let stale = false;
    const t = setTimeout(() => {
      const t0 = performance.now();
      structure()
        .outline(tex)
        .then(({ entries }) => {
          if (stale) return;
          setOutline(entries);
          if (tex.length > 1_000_000) {
            const ms = Math.round(performance.now() - t0);
            emit({
              scope: 'app',
              kind: 'info',
              actor: 'system',
              message: `outline parsed ${entries.length} entries in ${ms}ms`,
              event: { action: 'outline.parse', entries: entries.length, ms },
            });
          }
        })
        .catch(() => {
          /* outline never blocks editing */
        });
    }, 500);
    return () => {
      stale = true;
      clearTimeout(t);
    };
  }, [tex, fileName]);

  const handleTexChange = useCallback(
    (v: string) => {
      const t0 = performance.now();
      setTex(v);
      if (hasDir(fileName)) {
        setBuffers((b) => updateBuffer(b, fileName, v));
      }
      // Keystroke-to-paint probe emission.
      requestAnimationFrame(() => {
        const dt = Math.round(performance.now() - t0);
        if (v.length > 1_000_000) {
          emit({
            scope: 'app',
            kind: 'info',
            actor: 'system',
            message: `editor render ${(v.length / 1_048_576).toFixed(1)}MB file in ${dt}ms`,
            event: { action: 'editor.render', bytes: v.length, ms: dt },
          });
        }
      });
    },
    [fileName, setBuffers],
  );

  // ---- Command registry binding ----
  // Menus, icon buttons, and chords invoke these actions. Rename uses the
  // same project-file predicate the registry gates on.
  const isProjectFile = (p: string) => root != null && hasDir(p) && relUnder(root, p) !== null;
  const compileTarget = mainFile ?? (hasDir(fileName) ? fileName : null);
  const workingLabel = (() => {
    const t = compileTarget ?? largeFile ?? fileName;
    const base = baseName(t) || t;
    return relOf(t) === t ? base : `${relOf(t)}`;
  })();
  const [templateMode, setTemplateMode] = useState<TemplateDialogMode>(null);
  const exporter = useExport({
    pdf: previewDoc?.source ?? null,
    project: root && projectId ? { rootId: projectId, path: root } : null,
  });
  const menuCtx: MenuContext = {
    hasProject: root != null,
    isProjectFile: isProjectFile(fileName),
    dirty: !!buffers.get(fileName)?.dirty,
    compiling: compilePhase === 'compiling',
    autoCompile,
    precheckCount: precheck?.findings.length ?? 0,
    pdfOpen: pdfUrl != null,
    editorReady: viewportRef.current != null && largeFile == null,
    view: shell.view,
    preset: shell.preset,
    logCollapsed,
    outlineVisible: shell.outlineVisible,
    // Selection submenus navigate sections only.
    outlineLines: outline
      .filter((o) => o.kind === 'section')
      .map((o) => ({ line: o.line, title: o.title })),
    outlinePicks,
    canUndoDelete: trash.size > 0,
    canUndoReplace: lastReplace != null,
    historyAvailable:
      historyAvailability({
        hasProject: root != null,
        relPath: relInProject(fileName),
        storeReady: projectId != null,
      }) === 'ready',
    reloadPending: conflicts.length > 0,
    theme: themeMode,
    density,
    recentProjects,
  };
  const menuActions: CommandActions = {
    openProject: () => {
      void open();
    },
    findInFile: () => viewportRef.current?.openFind(),
    undoReplace: () => {
      void undoReplace();
    },
    quickOpen: () => setPaletteOpen(''),
    commandPalette: () => setPaletteOpen('>'),
    findInProject: () => {
      shell.setTreeVisible(true);
      setSearchOpen(true);
      setSearchFocus((k) => k + 1);
    },
    openRecent: (r) => {
      void openRoot(r, { warm: true });
    },
    clearRecents: () => {
      pruneRecentProjects(() => false);
      setRecentProjects([]);
    },
    newFile: () => {
      if (root) void handleCreate(root, 'untitled.tex');
      else
        emit({
          scope: 'fs',
          kind: 'warn',
          actor: 'user',
          message: 'New File needs an open project',
          event: { action: 'command.blocked', command: 'newFile', reason: 'no-project' },
        });
    },
    closeFile: () => {
      void handleCloseBuffer(fileName);
    },
    save: () => {
      void save();
    },
    setMainFile: () => {
      void setMainToOpenFile(fileName);
    },
    reloadFromDisk: () => {
      void resolveExternal('reload');
    },
    keepMine: () => {
      void resolveExternal('keep');
    },
    clean: () => {
      void handleClean();
    },
    undoDelete: () => {
      void handleUndo();
    },
    showHistory: () => {
      void openHistory();
    },
    showSettings: () => {
      setSettingsOpen(true);
    },
    renameActive: () => {
      if (!isProjectFile(fileName)) {
        emit({
          scope: 'fs',
          kind: 'warn',
          actor: 'user',
          message: 'Rename needs a project file (open one first)',
          event: {
            action: 'command.blocked',
            command: 'renameActive',
            reason: 'not-a-project-file',
          },
        });
        return;
      }
      setRenameOpen(true);
    },
    deleteActive: () => {
      void handleDelete(fileName);
    },
    selectAll: () => viewportRef.current?.selectAll(),
    expandSelection: () => viewportRef.current?.expandSelection(),
    shrinkSelection: () => viewportRef.current?.shrinkSelection(),
    goToDefinition: () => goToDefinitionRef.current(),
    goToLine: () => {
      setGoToOpen(true);
    },
    pickOutlineSection: (line: number) => setCurrentLine(line),
    toggleOutlinePick: (line: number) =>
      setOutlinePicks((prev) =>
        prev.includes(line) ? prev.filter((l) => l !== line) : [...prev, line],
      ),
    setPreset: shell.setPreset,
    toggleTree: shell.toggleTree,
    togglePreview: shell.togglePreview,
    toggleLog: shell.toggleLog,
    toggleOutline: shell.toggleOutline,
    setTheme: (m) => onThemeMode(m),
    setDensity: (d) => onDensityMode(d),
    zoomPreview: (a) => zoomActionRef.current?.(a),
    compile: () => {
      void compileRef.current();
    },
    compileFile: () => {
      void handleCompileFile(fileName);
    },
    makeOffline: () => {
      void makeOffline();
    },
    showPrecheck: openPrecheck,
    toggleAutoCompile: () => setAutoCompile(!autoCompile),
    exportPdf: () => void exporter.exportPdfAs(),
    exportZip: () => void exporter.exportZipAs(),
    newFromTemplate: () => setTemplateMode('gallery'),
    saveAsTemplate: () => setTemplateMode('save'),
    importTemplate: () => setTemplateMode('import'),
    showWelcome: () => void openWelcome(),
    cancelCompile: () => {
      void cancelCompile().catch((e) =>
        emit({
          scope: 'compile',
          kind: 'error',
          actor: 'user',
          message: 'cancel failed: ' + String(e).slice(0, 120),
          event: { action: 'compile.cancel-failed', error: String(e).slice(0, 200) },
        }),
      );
    },
    forwardSync: () => {
      void forwardSyncRef.current();
    },
    showShortcuts: () => setShortcutsOpen(true),
    showAbout: () => setAboutOpen(true),
  };
  const menuSections = buildMenus(menuCtx, menuActions);
  useDevCamera((m) => {
    if (m.op === 'open') {
      if (root) void handleSelectRef.current(joinPath(root, m.rel));
    } else if (m.op === 'line') viewportRef.current?.goToLine(m.line);
    else if (m.op === 'page') setPageNumber(m.page);
    else menuActionRef.current(m.id);
  });
  menuActionRef.current = (id: string) => {
    for (const sec of menuSections) {
      const cmd = sec.commands.find((c) => c.id === id);
      if (cmd && cmd.enabled && cmd.visible !== false) {
        void cmd.run?.();
        return;
      }
    }
  };

  const mainDoc =
    mainFile && root && projectId ? sourceFor(mainFile, [{ rootId: projectId, path: root }]) : null;

  return (
    <Box sx={{ display: 'flex', flexDirection: 'column', height: '100vh' }}>
      <MenuBar
        sections={menuSections}
        status={
          <Box sx={{ display: 'flex', alignItems: 'center', gap: 0.5 }}>
            <Typography variant="caption" color="text.secondary">
              {compilePhase === 'compiling' ? `compiling ${compileTimer}s` : compilePhase}
            </Typography>
            <CompileButton
              targetLabel={workingLabel}
              compiling={compilePhase === 'compiling'}
              onCompile={() => menuActionRef.current('tools.compile')}
              onCancel={() => menuActionRef.current('tools.cancel')}
            />
          </Box>
        }
      />
      <WorkArea
        shell={shell}
        side={
          <SideColumn
            root={root}
            searchOpen={searchOpen}
            search={{
              runSearch,
              onOpenHit: (rel, hit) => void openHit(rel, hit),
              runPreview,
              runApply,
              lastReplace,
              onUndoReplace: () => void undoReplace(),
              onClose: () => setSearchOpen(false),
              focusKey: searchFocus,
            }}
            tree={{
              tree,
              selected: fileName,
              onSelect: handleSelect,
              onDoubleClick: (p) => {
                if (p.endsWith('.tex')) void selectAndSetMain(p);
              },
              onDelete: handleDelete,
              onSetMain: (p) => {
                void selectAndSetMain(p);
              },
              onCompileFile: (p) => {
                void handleCompileFile(p);
              },
              onCreate: handleCreate,
              onRename: handleRename,
              onExpandDir: expandDir,
              rootDir: root,
              mainFile,
              lazy: true,
              maxDepth: 2,
              filterHidden: true,
            }}
            outlineVisible={shell.outlineVisible}
            outline={{ entries: outline, onJump: (line) => setCurrentLine(line) }}
          />
        }
        editor={
          <EditorPane
            fileName={fileName}
            fileLabel={relOf(fileName) ?? fileName}
            dirty={!!buffers.get(fileName)?.dirty}
            mainLabel={relOf(mainFile) ?? '(none)'}
            mainTip={mainFileTip(mainFile, mainSource)}
            mainFile={mainFile}
            mainCandidates={mainCandidates}
            onPickMain={(c) => void pickMain(c)}
            labelOf={(c) => relOf(c) ?? c}
            tabs={{
              buffers,
              active: fileName,
              onSelect: (p) => {
                void handleSelect(p);
              },
              onClose: (p) => {
                void handleCloseBuffer(p);
              },
              onCloseOthers: (k) => {
                void handleCloseOthers(k);
              },
              onCloseAll: () => {
                void handleCloseAll();
              },
            }}
            largeFile={largeFile}
            previewFile={previewFile}
            viewport={{
              value: tex,
              onChange: handleTexChange,
              onSave: save,
              line: currentLine,
              flashKey: synctexFlash,
              select: hitSelect && hitSelect.path === fileName ? hitSelect : undefined,
              viewportRef,
              onDoubleClickRef: forwardSyncLineRef,
              filePath: fileName,
              prefs,
              definitionRef,
            }}
            log={log}
            logTitle={compileLogTitle(log, compiled)}
          />
        }
        preview={
          <PreviewPane
            pdfUrl={pdfUrl}
            stamp={previewDoc?.stamp ?? 0}
            mainSource={mainDoc}
            pageNumber={pageNumber}
            onPage={setPageNumber}
            onSync={handleForwardSync}
            onInverse={(page, x, y) => {
              void handleInverseSync(page, x, y);
            }}
            syncDisabled={compilePhase === 'compiling'}
            zoomActionRef={zoomActionRef}
          />
        }
      />
      <LogStream
        height={layout.logHeight}
        onHeight={shell.setLogHeight}
        collapsed={logCollapsed}
        onToggleCollapse={shell.toggleLog}
        onJump={handleProblemJump}
      />
      <ShortcutsDialog open={shortcutsOpen} onClose={() => setShortcutsOpen(false)} />
      <SettingsDialog
        open={settingsOpen}
        onClose={() => setSettingsOpen(false)}
        prefs={prefs}
        onChange={onPrefs}
        precheckPopup={precheckPopup}
        onPrecheckPopup={setPrecheckPopup}
      />
      <RenameDialog
        open={renameOpen}
        title={baseName(fileName) || fileName}
        initial={baseName(fileName)}
        onClose={() => setRenameOpen(false)}
        onRename={(name) => void handleRename(fileName, name)}
      />
      <GoToLineDialog
        open={goToOpen}
        initial={String(currentLine)}
        onClose={() => setGoToOpen(false)}
        onGo={(n) => viewportRef.current?.goToLine(n)}
      />
      <AboutDialog open={aboutOpen} onClose={() => setAboutOpen(false)} />
      <PaletteDialog
        open={paletteOpen != null}
        initial={paletteOpen ?? ''}
        onClose={() => setPaletteOpen(null)}
        findFiles={findFiles}
        rank={rankNames}
        commands={paletteCommands(menuSections, ['file.quick-open', 'view.command-palette'])}
        onOpenFile={openFileRel}
      />
      <HistoryDialog
        open={historyOpen}
        onClose={() => setHistoryOpen(false)}
        fileLabel={relInProject(fileName)}
        availability={historyAvail}
        rows={historyRows}
        summary={historySummary}
        notice={historyNotice}
        restoringRev={restoringRev}
        onRestore={(rev) => void restoreRevision(rev)}
      />
      <TemplateDialogs
        mode={templateMode}
        onClose={() => setTemplateMode(null)}
        project={root && projectId ? { rootId: projectId, path: root } : null}
        mainRel={mainFile ? relInProject(mainFile) : null}
        openRoot={(r, main) => openRoot(r, { warm: true, cold: true, main })}
      />
      <StatusBar
        mainFile={relOf(mainFile)}
        mainFileTitle={mainFile}
        historyCount={trash.size}
        revisionCount={revisionCount}
        revisionTitle={`${revisionCount} revision${revisionCount === 1 ? '' : 's'} of ${
          relInProject(fileName) ?? fileName
        } — click to open History`}
        onOpenHistory={() => void openHistory()}
        phase={compilePhase}
        timer={compileTimer}
        message={compilePhase === 'compiling' ? (progress ?? log) : log}
        messageTitle={compilePhase === 'compiling' ? undefined : compileLogTitle(log, compiled)}
        offline={compilePhase === 'compiling' ? null : offline}
        warnings={precheck?.findings.length ?? 0}
        onOpenWarnings={openPrecheck}
      />
      {conflicts.length > 0 ? (
        <ExternalChangeDialog
          path={relInProject(conflicts[0]) ?? conflicts[0]}
          more={conflicts.length - 1}
          onReload={() => void resolveExternal('reload')}
          onKeep={() => void resolveExternal('keep')}
        />
      ) : null}
      {precheck && precheckOpen ? (
        <PrecheckPanel
          key={`${precheck.target}:${precheck.findings.length}`}
          precheck={precheck}
          popupEnabled={precheckPopup}
          onJump={(r, rel, line) => void openFinding(r, rel, line)}
          onClose={closePrecheck}
        />
      ) : null}
    </Box>
  );
}
