import { useCallback, useEffect, useRef, useState } from 'react';
import Box from '@mui/material/Box';
import Button from '@mui/material/Button';
import Chip from '@mui/material/Chip';
import Dialog from '@mui/material/Dialog';
import DialogActions from '@mui/material/DialogActions';
import DialogContent from '@mui/material/DialogContent';
import DialogTitle from '@mui/material/DialogTitle';
import Menu from '@mui/material/Menu';
import MenuItem from '@mui/material/MenuItem';
import TextField from '@mui/material/TextField';
import Typography from '@mui/material/Typography';
import EditorViewport, { type EditorViewportHandle } from './components/EditorViewport';
import BufferTabs from './components/BufferTabs';
import CompileButton from './components/CompileButton';
import MenuBar from './components/MenuBar';
import Preview from './components/Preview';
import BinaryPreview from './components/BinaryPreview';
import FileTree from './components/FileTree';
import LogStream from './components/LogStream';
import OutlineView from './components/OutlineView';
import HistoryDialog from './components/HistoryDialog';
import ShortcutsDialog from './components/ShortcutsDialog';
import SettingsDialog from './components/SettingsDialog';
import type { AppearancePrefs } from './lib/appearance';
import StatusBar from './components/StatusBar';
import Pane, { PaneSplitter } from './components/Pane';
import {
  listDir1Level,
  loadTex,
  saveTex,
  saveTexToDisk,
  isPreviewable,
  LARGE_FILE_BYTES,
  TreeEntry,
} from './lib/files';
import { getOrCreateBuffer, updateBuffer, markSaved, enforceBufferCap } from './lib/buffers';
import { cancelCompile } from './lib/compile';
import { onPdf, type PreviewDoc } from './lib/preview-bus';
import { emit } from './lib/events';
import { startEventLog } from './lib/eventlog';
import { historyAvailability } from './lib/history.view';
import { parseOutline, type OutlineEntry } from './lib/outline';
import { matchesCompile, matchesForwardSync, matchesMenuChord, menuChordId } from './lib/keymap';
import { buildMenus, presetOf, type CommandActions, type MenuContext } from './lib/commands';
import { resolveMainFileTauri, setMainFile } from './lib/mainFile.tauri';
import { FileHistory } from './lib/file-history';
import { pruneRecentProjects } from './lib/recentProjects';
import { DEVICE_PREF_KEYS, store } from './lib/app-store';
import { fs } from './lib/fs-provider';
import { grantUntitledAccess } from './lib/projectAccess';
import { useBufferManager } from './hooks/useBufferManager';
import { useCompileRunner } from './hooks/useCompileRunner';
import { useProjectTree } from './hooks/useProjectTree';
import { useFileOps } from './hooks/useFileOps';
import { useRevisionHistory } from './hooks/useRevisionHistory';
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
  const [tex, setTex] = useState(HELLO);
  const [root, setRoot] = useState<string | null>(null);
  const [projectId, setProjectId] = useState<string | null>(null);
  const [tree, setTree] = useState<TreeEntry[]>([]);
  const [fileName, setFileName] = useState('hello.tex');
  const [mainFile, setMainFileState] = useState<string | null>(null);
  const [mainSource, setMainSource] = useState('');
  const [mainCandidates, setMainCandidates] = useState<string[]>([]);
  const [trash] = useState(() => new FileHistory());

  const [reloadPath, setReloadPath] = useState<string | null>(null);
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
  // Latest tree selection wins: rapid clicks resolve out of order otherwise.
  const selectTokenRef = useRef(0);
  // Paths WE just wrote (save/autosave/compile persist/undo): watcher echoes of
  // our own writes must not raise the reload banner. Windowed suppression.
  const ownWritesRef = useRef<Map<string, number>>(new Map());
  const markOwnWrite = useCallback((p: string) => {
    ownWritesRef.current.set(p, Date.now());
  }, []);
  const { buffers, setBuffers, buffersRef, handleCloseBuffer, handleCloseOthers, handleCloseAll } =
    useBufferManager({
      fileName,
      setFileName,
      previewFile,
      setPreviewFile,
      setTex,
      setLargeFile,
      setReloadPath,
      markOwnWrite,
    });

  // Revision history: app-local, keyed by the backend-minted project id. One
  // project is open at a time, so `rootFor` answers for that id alone.
  const rootRef = useRef<string | null>(root);
  rootRef.current = root;
  const projectIdRef = useRef<string | null>(projectId);
  projectIdRef.current = projectId;

  /** Project-relative path for a file inside the open project, else null. */
  const relInProject = useCallback((abs: string): string | null => {
    const r = rootRef.current;
    return r && abs.startsWith(r + '/') ? abs.slice(r.length + 1) : null;
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
    recordRevision,
    openHistory,
    restoreRevision,
  } = useRevisionHistory({
    root,
    rootRef,
    projectId,
    projectIdRef,
    relInProject,
    fileName,
    setBuffers,
    setTex,
    setReloadPath,
    setLog,
    markOwnWrite,
  });

  // The bus is recorded to an app-local JSONL file for the length of the run.
  useEffect(() => {
    const log = startEventLog();
    return () => log.stop();
  }, []);

  useEffect(() => onPdf(setPreviewDoc), []);

  const handleSelect = useCallback(
    async (path: string) => {
      // Persist current buffer before switching (dirty survives switch via map).
      if (fileName.includes('/') && path !== fileName) {
        const cur = buffers.get(fileName);
        if (cur?.dirty) {
          try {
            await saveTex(fileName, cur.value);
            markOwnWrite(fileName);
            setBuffers((b) => markSaved(b, fileName));
            await recordRevision(fileName, cur.value);
          } catch {
            /* keep dirty */
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
        setReloadPath(null);
        setLog('previewing ' + path);
        emit({
          scope: 'fs',
          kind: 'info',
          message: 'previewing ' + path,
          data: { action: 'file.preview', path },
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
        setReloadPath(null);
        setLog('switched ' + path + (kept.dirty ? ' (unsaved changes)' : ''));
        emit({
          scope: 'fs',
          kind: 'info',
          message: 'switched ' + path,
          data: { action: 'file.switch', path, dirty: kept.dirty },
        });
        if (root && path.endsWith('.tex')) void resolveMain(root, path);
        return;
      }
      emit({
        scope: 'fs',
        kind: 'progress',
        message: 'loading ' + path,
        data: { action: 'file.load', path },
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
            message: `large file placeholder ${path} (${size}B)`,
            data: { action: 'file.too-large', path, bytes: size },
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
        setReloadPath(null);
        setLog('loaded ' + path);
        emit({
          scope: 'fs',
          kind: 'success',
          message: 'loaded ' + path,
          data: { action: 'file.open', path, chars: content.length },
        });
        if (root && path.endsWith('.tex')) void resolveMain(root, path);
      } catch (e) {
        setLog('load failed: ' + String(e).slice(0, 120));
        emit({
          scope: 'fs',
          kind: 'error',
          message: 'load failed ' + path,
          data: { action: 'file.load-failed', path, error: String(e).slice(0, 200) },
        });
      }
    },
    [buffers, setBuffers, fileName, root, markOwnWrite, recordRevision],
  );

  const resolveMain = useCallback(async (r: string, opened: string | null) => {
    const res = await resolveMainFileTauri(r, opened);
    setMainFileState(res.mainFile);
    setMainSource(res.source);
    setMainCandidates(res.candidates);
    return res.mainFile;
  }, []);

  // UI tree is 1 level + expand-on-demand. The recursive walk runs only
  // for main-file scan + watcher baseline, never on the open path.
  // Tree CRUD: create/rename via plugin-fs; own-write marks suppress echoes.

  async function handleSetMain() {
    if (!root || !fileName.includes('/')) return;
    await setMainFile(root, fileName);
    const m = await resolveMain(root, fileName);
    setLog('main file: ' + (m ?? '(none)'));
    emit({
      scope: 'fs',
      kind: 'success',
      message: 'main file set: ' + (m ?? '(none)'),
      data: { action: 'main.set', mainFile: m },
    });
  }

  // Main-file tie-break: the scan found >1 `\documentclass` and picked the
  // first. Choosing here writes the explicit association, so the tie never
  // reappears for this project.
  async function handlePickMain(path: string) {
    if (!root) return;
    await setMainFile(root, path);
    await resolveMain(root, path);
    setMainAnchor(null);
    emit({
      scope: 'fs',
      kind: 'success',
      message: 'main file set: ' + path,
      data: { action: 'main.set', mainFile: path },
    });
  }

  // Tree-driven main association (double-click / context menu on a .tex row).
  async function handleSetMainPath(path: string) {
    if (!root) return;
    await handleSelect(path);
    await setMainFile(root, path);
    const m = await resolveMain(root, path);
    setLog('main file: ' + (m ?? '(none)'));
    emit({
      scope: 'fs',
      kind: 'success',
      message: 'main file set: ' + (m ?? '(none)'),
      data: { action: 'main.set', mainFile: m },
    });
  }

  // One-off compile of a tree-selected file. This is EXPECTED to fail for
  // fragments (a chapter without \documentclass cannot build alone) — the
  // engine error is the honest answer, surfaced through the normal failure
  // path (phase + stream + click-to-jump rows).

  // Keymap listener subscribes once and reads via refs. Menu chords
  // dispatch through the same command registry refs — one path, no
  // duplicates. Double-click in the editor = forward SyncTeX from the
  // caret line (complements single-click inverse on the PDF canvas).
  const menuActionRef = useRef<(id: string) => void>(() => {});
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const mod = e.ctrlKey || e.metaKey;
      if (matchesCompile(e as unknown as KeyboardEvent)) {
        e.preventDefault();
        void compileRef.current();
      } else if (matchesForwardSync(e as unknown as KeyboardEvent)) {
        e.preventDefault();
        void forwardSyncRef.current();
      } else if (matchesMenuChord(e as unknown as KeyboardEvent)) {
        const id = menuChordId(e as unknown as KeyboardEvent);
        if (id) {
          e.preventDefault();
          menuActionRef.current(id);
        }
      } else if (!mod && e.key === '?') {
        setShortcutsOpen(true);
      } else if (mod && e.key.toLowerCase() === 'b') {
        e.preventDefault();
        setTreeVisible((v) => !v);
      } else if (mod && e.key === 'Tab') {
        // Tab cycling when the tab strip is not focused; global fallback:
        const keys = [...buffersRef.current.keys()];
        if (keys.length > 1) {
          e.preventDefault();
          const i = keys.indexOf(fileNameRef.current);
          const n = e.shiftKey ? (i - 1 + keys.length) % keys.length : (i + 1) % keys.length;
          void handleSelectRef.current(keys[n]);
        }
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [buffersRef]);

  // Untitled documents resolve against the backend-owned scratch root.
  const [scratch, setScratch] = useState<{ rootId: string; path: string } | null>(null);
  useEffect(() => {
    void grantUntitledAccess().then((g) => {
      if (g.path && g.rootId) setScratch({ rootId: g.rootId, path: g.path });
    });
  }, []);
  const workdirHint = fileName.includes('/')
    ? fileName.slice(0, fileName.lastIndexOf('/'))
    : (scratch?.path ?? '');
  const mainDir = mainFile ? mainFile.slice(0, mainFile.lastIndexOf('/')) : workdirHint;
  // Repo-relative for display (absolute kept in tooltips); plain language.
  const relOf = (abs: string | null): string | null => {
    if (!abs) return null;
    if (root && abs.startsWith(root + '/')) return abs.slice(root.length + 1);
    return abs;
  };

  const save = useCallback(async () => {
    if (largeFile) {
      setLog('save blocked: large placeholder file is not loaded');
      emit({
        scope: 'fs',
        kind: 'warn',
        message: 'save blocked for large placeholder ' + largeFile,
        data: { action: 'file.save-blocked', path: largeFile, reason: 'large-placeholder' },
      });
      return;
    }
    if (fileName.includes('/')) {
      const cur = buffers.get(fileName);
      const text = cur?.value ?? tex;
      await saveTex(fileName, text);
      markOwnWrite(fileName);
      setBuffers((b) => markSaved(b, fileName));
      await recordRevision(fileName, text);
      setLog('saved ' + fileName);
      emit({
        scope: 'fs',
        kind: 'success',
        message: 'saved ' + fileName,
        data: { action: 'file.save', path: fileName, chars: text.length },
      });
    } else {
      await saveTexToDisk(fileName, tex);
      setLog('saved ' + fileName);
      emit({
        scope: 'fs',
        kind: 'success',
        message: 'saved ' + fileName,
        data: { action: 'file.save', path: fileName, chars: tex.length, untitled: true },
      });
    }
  }, [fileName, tex, buffers, setBuffers, largeFile, markOwnWrite, recordRevision]);

  useEffect(() => {
    if (!fileName.includes('/')) return;
    const t = setTimeout(() => {
      const cur = buffers.get(fileName);
      if (cur?.dirty) {
        markOwnWrite(fileName);
        saveTex(fileName, cur.value)
          .then(async () => {
            setBuffers((b) => markSaved(b, fileName));
            await recordRevision(fileName, cur.value);
            emit({
              scope: 'fs',
              kind: 'info',
              message: 'autosaved ' + fileName,
              data: { action: 'file.save', path: fileName, chars: cur.value.length, auto: true },
            });
            setLog('autosaved ' + new Date().toTimeString().slice(0, 8));
          })
          .catch(() => {
            /* autosave best-effort — dirty flag stays */
          });
      }
    }, 1200);
    return () => clearTimeout(t);
  }, [tex, fileName, buffers, setBuffers, markOwnWrite, recordRevision]);

  // Publish engine-log problems as first-class stream events (click-to-jump).
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
  // ---- Shell state: view is explicit booleans (View menu presets own them) ----
  const [treeVisible, setTreeVisible] = useState(true);
  const [editorVisible, setEditorVisible] = useState(true);
  const [previewOpen, setPreviewOpen] = useState(true);
  const [layout, setLayout] = useState(() => {
    try {
      const raw = store().get(DEVICE_PREF_KEYS.layout);
      if (raw) {
        const j = JSON.parse(raw) as Partial<{
          editorRatio: number;
          previewRatio: number;
          logHeight: number;
        }>;
        return {
          editorRatio:
            typeof j.editorRatio === 'number' ? Math.min(0.8, Math.max(0.2, j.editorRatio)) : 0.6,
          previewRatio:
            typeof j.previewRatio === 'number' ? Math.min(0.8, Math.max(0.2, j.previewRatio)) : 0.4,
          logHeight:
            typeof j.logHeight === 'number' ? Math.max(80, Math.min(600, j.logHeight)) : 160,
        };
      }
    } catch {
      /* corrupted prefs — defaults win */
    }
    return { editorRatio: 0.6, previewRatio: 0.4, logHeight: 160 };
  });
  useEffect(() => {
    store().set(DEVICE_PREF_KEYS.layout, JSON.stringify(layout));
  }, [layout]);
  const [logCollapsed, setLogCollapsed] = useState(false);
  const { compilePhase, compileTimer, warmCompile, handleCompileFile } = useCompileRunner({
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
    markOwnWrite,
    setLog,
    setLogCollapsed,
    compileRef,
  });
  const { reloadTree, openRoot, open, recentProjects, setRecentProjects } = useProjectTree({
    root,
    setRoot,
    setProjectId,
    setTree,
    fileNameRef,
    ownWritesRef,
    setReloadPath,
    setLog,
    trash,
    resolveMain,
    handleSelect,
    warmCompile,
  });
  const { handleCreate, handleRename, handleReload, handleDelete, handleClean, handleUndo } =
    useFileOps({
      root,
      projectId,
      scratch,
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
    });
  const [previewCollapsed, setPreviewCollapsed] = useState(false);
  const [shortcutsOpen, setShortcutsOpen] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  void previewCollapsed;
  const fileTreeVisible = treeVisible;
  const [outlineVisible, setOutlineVisible] = useState(true);
  const [aboutOpen, setAboutOpen] = useState(false);
  const [goToOpen, setGoToOpen] = useState(false);
  const [goToDraft, setGoToDraft] = useState('');
  // Anchor for the main-file tie-break menu.
  const [mainAnchor, setMainAnchor] = useState<HTMLElement | null>(null);
  // Rename dialog for the active file.
  const [renameOpen, setRenameOpen] = useState(false);
  const [renameDraft, setRenameDraft] = useState('');
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
    setReloadPath,
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
    const t = setTimeout(() => {
      try {
        const t0 = performance.now();
        const entries = parseOutline(tex);
        setOutline(entries);
        if (tex.length > 1_000_000) {
          emit({
            scope: 'app',
            kind: 'info',
            message: `outline parsed ${entries.length} entries in ${Math.round(performance.now() - t0)}ms`,
          });
        }
      } catch {
        /* outline never blocks editing */
      }
    }, 500);
    return () => clearTimeout(t);
  }, [tex, fileName]);

  const handleTexChange = useCallback(
    (v: string) => {
      const t0 = performance.now();
      setTex(v);
      if (fileName.includes('/')) {
        setBuffers((b) => updateBuffer(b, fileName, v));
      }
      // Keystroke-to-paint probe emission.
      requestAnimationFrame(() => {
        const dt = Math.round(performance.now() - t0);
        if (v.length > 1_000_000) {
          emit({
            scope: 'app',
            kind: 'info',
            message: `editor render ${(v.length / 1_048_576).toFixed(1)}MB file in ${dt}ms`,
          });
        }
      });
    },
    [fileName, setBuffers],
  );

  // ---- Command registry binding ----
  // Menus, icon buttons, and chords invoke these actions. Rename uses the
  // same project-file predicate the registry gates on.
  const isProjectFile = (p: string) => root != null && p.includes('/') && p.startsWith(root + '/');
  const compileTarget = mainFile ?? (fileName.includes('/') ? fileName : null);
  const workingLabel = (() => {
    const t = compileTarget ?? largeFile ?? fileName;
    const base = t.slice(t.lastIndexOf('/') + 1) || t;
    return relOf(t) === t ? base : `${relOf(t)}`;
  })();
  const menuCtx: MenuContext = {
    hasProject: root != null,
    isProjectFile: isProjectFile(fileName),
    dirty: !!buffers.get(fileName)?.dirty,
    compiling: compilePhase === 'compiling',
    pdfOpen: pdfUrl != null,
    editorReady: viewportRef.current != null && largeFile == null,
    view: { tree: treeVisible, editor: editorVisible, preview: previewOpen },
    preset: presetOf({ tree: treeVisible, editor: editorVisible, preview: previewOpen }),
    logCollapsed,
    outlineVisible,
    // Selection submenus navigate sections only.
    outlineLines: outline
      .filter((o) => o.kind === 'section')
      .map((o) => ({ line: o.line, title: o.title })),
    outlinePicks,
    canUndoDelete: trash.size > 0,
    historyAvailable:
      historyAvailability({
        hasProject: root != null,
        relPath: relInProject(fileName),
        storeReady: projectId != null,
      }) === 'ready',
    reloadPending: reloadPath != null,
    theme: themeMode,
    density,
    recentProjects,
  };
  const menuActions: CommandActions = {
    openProject: () => {
      void open();
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
          message: 'New File needs an open project',
          data: { action: 'command.blocked', command: 'newFile', reason: 'no-project' },
        });
    },
    closeFile: () => {
      void handleCloseBuffer(fileName);
    },
    save: () => {
      void save();
    },
    setMainFile: () => {
      void handleSetMain();
    },
    reloadFromDisk: () => {
      void handleReload();
    },
    keepMine: () => setReloadPath(null),
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
          message: 'Rename needs a project file (open one first)',
          data: {
            action: 'command.blocked',
            command: 'renameActive',
            reason: 'not-a-project-file',
          },
        });
        return;
      }
      setRenameDraft(fileName.slice(fileName.lastIndexOf('/') + 1));
      setRenameOpen(true);
    },
    deleteActive: () => {
      void handleDelete(fileName);
    },
    selectAll: () => viewportRef.current?.selectAll(),
    expandSelection: () => viewportRef.current?.expandSelection(),
    shrinkSelection: () => viewportRef.current?.shrinkSelection(),
    goToLine: () => {
      setGoToDraft(String(currentLine));
      setGoToOpen(true);
    },
    pickOutlineSection: (line: number) => setCurrentLine(line),
    toggleOutlinePick: (line: number) =>
      setOutlinePicks((prev) =>
        prev.includes(line) ? prev.filter((l) => l !== line) : [...prev, line],
      ),
    setPreset: (preset) => {
      if (preset === 'both') {
        setTreeVisible(true);
        setEditorVisible(true);
        setPreviewOpen(true);
        setPreviewCollapsed(false);
      } else if (preset === 'editor') {
        setTreeVisible(false);
        setEditorVisible(true);
        setPreviewOpen(false);
      } else {
        setTreeVisible(false);
        setEditorVisible(false);
        setPreviewOpen(true);
        setPreviewCollapsed(false);
      }
    },
    toggleTree: () => setTreeVisible((v) => !v),
    togglePreview: () => {
      setPreviewOpen((v) => !v);
      setPreviewCollapsed(false);
    },
    toggleLog: () => setLogCollapsed((c) => !c),
    toggleOutline: () => setOutlineVisible((v) => !v),
    setTheme: (m) => onThemeMode(m),
    setDensity: (d) => onDensityMode(d),
    compile: () => {
      void compileRef.current();
    },
    compileFile: () => {
      void handleCompileFile(fileName);
    },
    cancelCompile: () => {
      void cancelCompile().catch((e) =>
        emit({
          scope: 'compile',
          kind: 'error',
          message: 'cancel failed: ' + String(e).slice(0, 120),
          data: { action: 'compile.cancel-failed', error: String(e).slice(0, 200) },
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
  menuActionRef.current = (id: string) => {
    for (const sec of menuSections) {
      const cmd = sec.commands.find((c) => c.id === id);
      if (cmd && cmd.enabled && cmd.visible !== false) {
        void cmd.run?.();
        return;
      }
    }
  };

  // Quiet caption: the editor header names the file + its main file and
  // nothing else.
  const mainLabel = relOf(mainFile) ?? '(none)';
  const mainTip =
    mainFile == null
      ? 'No main file detected'
      : mainSource === 'config'
        ? `Main file (your choice): ${mainFile}`
        : mainSource === 'magic'
          ? `Main file (from %!TEX root): ${mainFile}`
          : mainSource === 'scan'
            ? `Main file (auto-detected): ${mainFile}`
            : mainSource === 'single'
              ? `Main file (only .tex file): ${mainFile}`
              : `Main file: ${mainFile}`;
  const editorPane = (
    <Box
      sx={{ p: 2, display: 'flex', flexDirection: 'column', height: '100%', overflow: 'hidden' }}
    >
      <Box sx={{ display: 'flex', alignItems: 'center', gap: 0.5, mb: 1, minWidth: 0 }}>
        <Typography
          variant="caption"
          noWrap
          sx={{ minWidth: 0, overflow: 'hidden', textOverflow: 'ellipsis' }}
          title={fileName}
        >
          {relOf(fileName) ?? fileName}
          {buffers.get(fileName)?.dirty ? ' ●' : ''}
        </Typography>
        <Chip
          size="small"
          label={`main: ${mainLabel}`}
          title={mainTip}
          onClick={mainCandidates.length > 1 ? (e) => setMainAnchor(e.currentTarget) : undefined}
          sx={{ height: 18, maxWidth: 220 }}
        />
        {mainCandidates.length > 1 ? (
          <Menu
            open={mainAnchor != null}
            anchorEl={mainAnchor}
            onClose={() => setMainAnchor(null)}
            slotProps={{ list: { 'aria-label': 'Choose main file' } }}
          >
            {mainCandidates.map((c) => (
              <MenuItem
                key={c}
                selected={c === mainFile}
                onClick={() => {
                  void handlePickMain(c);
                }}
              >
                <Typography variant="body2" noWrap>
                  {relOf(c) ?? c}
                </Typography>
              </MenuItem>
            ))}
          </Menu>
        ) : null}
      </Box>
      <BufferTabs
        buffers={buffers}
        active={fileName}
        onSelect={(p) => {
          void handleSelect(p);
        }}
        onClose={(p) => {
          void handleCloseBuffer(p);
        }}
        onCloseOthers={(k) => {
          void handleCloseOthers(k);
        }}
        onCloseAll={() => {
          void handleCloseAll();
        }}
      />
      {reloadPath ? (
        <Box sx={{ display: 'flex', gap: 1, mb: 1, alignItems: 'center' }}>
          <Typography variant="body2">Changed on disk: {reloadPath}</Typography>
          <Button size="small" variant="outlined" onClick={handleReload}>
            Reload
          </Button>
          <Button size="small" onClick={() => setReloadPath(null)}>
            Keep mine
          </Button>
        </Box>
      ) : null}
      {largeFile ? (
        <Typography variant="body2" sx={{ mt: 1 }}>
          Large file — not loaded into the editor ({largeFile}). Open externally to edit.
        </Typography>
      ) : previewFile ? (
        <Box
          sx={{
            flex: 1,
            minHeight: 0,
            display: 'flex',
            flexDirection: 'column',
            overflow: 'hidden',
          }}
        >
          <BinaryPreview key={previewFile} path={previewFile} />
        </Box>
      ) : (
        <Box sx={{ flex: 1, overflow: 'auto' }}>
          <EditorViewport
            value={tex}
            onChange={handleTexChange}
            onSave={save}
            line={currentLine}
            flashKey={synctexFlash}
            viewportRef={viewportRef}
            onDoubleClickRef={forwardSyncLineRef}
            filePath={fileName}
            prefs={prefs}
          />
        </Box>
      )}
      <Typography variant="caption" sx={{ display: 'block', mt: 1 }}>
        {log}
      </Typography>
    </Box>
  );

  const previewPane = (
    <Box
      sx={{
        display: 'flex',
        flexDirection: 'column',
        height: '100%',
        minHeight: 0,
        overflow: 'hidden',
      }}
    >
      <Preview
        pdfUrl={pdfUrl}
        stamp={previewDoc?.stamp ?? 0}
        pageNumber={pageNumber}
        onPage={setPageNumber}
        onSync={handleForwardSync}
        onInverse={(page, x, y) => {
          void handleInverseSync(page, x, y);
        }}
        syncDisabled={compilePhase === 'compiling'}
      />
    </Box>
  );

  const previewVisible = previewOpen && !previewCollapsed;

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
      <Box sx={{ display: 'flex', flex: 1, minHeight: 0, overflowX: 'auto' }}>
        {fileTreeVisible && (
          <Box
            sx={{
              width: 260,
              flexShrink: 0,
              overflow: 'auto',
              borderRight: 1,
              borderColor: 'divider',
              p: 1,
              display: 'flex',
              flexDirection: 'column',
            }}
          >
            {root ? (
              <>
                <Box sx={{ flexShrink: 0 }}>
                  <FileTree
                    tree={tree}
                    selected={fileName}
                    onSelect={handleSelect}
                    onDoubleClick={(p) => {
                      if (p.endsWith('.tex')) void handleSetMainPath(p);
                    }}
                    onDelete={handleDelete}
                    onSetMain={(p) => {
                      void handleSetMainPath(p);
                    }}
                    onCompileFile={(p) => {
                      void handleCompileFile(p);
                    }}
                    onCreate={handleCreate}
                    onRename={handleRename}
                    onExpandDir={listDir1Level}
                    rootDir={root}
                    mainFile={mainFile}
                    lazy
                    maxDepth={2}
                    filterHidden
                  />
                </Box>
                {outlineVisible ? (
                  <OutlineView entries={outline} onJump={(line) => setCurrentLine(line)} />
                ) : null}
              </>
            ) : (
              <Typography variant="body2" color="text.secondary">
                Open a project to browse files.
              </Typography>
            )}
          </Box>
        )}
        {editorVisible ? (
          <Pane
            label="editor"
            ratio={layout.editorRatio}
            onRatio={(r) => setLayout((l) => ({ ...l, editorRatio: r, previewRatio: 1 - r }))}
          >
            {editorPane}
          </Pane>
        ) : null}
        {editorVisible && previewVisible ? (
          <PaneSplitter
            label="Resize editor and preview"
            onDrag={(dx) =>
              setLayout((l) => {
                const w = window.innerWidth || 1000;
                const r = Math.min(0.8, Math.max(0.2, l.editorRatio + dx / w));
                return { ...l, editorRatio: r, previewRatio: 1 - r };
              })
            }
            onKeyResize={(dir) =>
              setLayout((l) => {
                const r = Math.min(0.8, Math.max(0.2, l.editorRatio + dir * 0.05));
                return { ...l, editorRatio: r, previewRatio: 1 - r };
              })
            }
          />
        ) : null}
        {!previewVisible ? (
          <Box
            sx={{
              width: 48,
              flexShrink: 0,
              display: 'flex',
              alignItems: 'flex-start',
              justifyContent: 'center',
              pt: 1,
            }}
          >
            <Button
              size="small"
              aria-label="Show preview"
              onClick={() => {
                setPreviewOpen(true);
                setPreviewCollapsed(false);
              }}
            >
              show
            </Button>
          </Box>
        ) : (
          <Pane
            label="preview"
            ratio={layout.previewRatio}
            onRatio={(r) => setLayout((l) => ({ ...l, previewRatio: r, editorRatio: 1 - r }))}
          >
            <Box sx={{ display: 'flex', justifyContent: 'flex-end' }}>
              <Button
                size="small"
                aria-label="Hide preview"
                onClick={() => setPreviewCollapsed(true)}
              >
                hide
              </Button>
            </Box>
            {previewPane}
          </Pane>
        )}
      </Box>
      <LogStream
        height={layout.logHeight}
        onHeight={(h) => setLayout((l) => ({ ...l, logHeight: h }))}
        collapsed={logCollapsed}
        onToggleCollapse={() => setLogCollapsed((c) => !c)}
        onJump={handleJump}
      />
      <ShortcutsDialog open={shortcutsOpen} onClose={() => setShortcutsOpen(false)} />
      <SettingsDialog
        open={settingsOpen}
        onClose={() => setSettingsOpen(false)}
        prefs={prefs}
        onChange={onPrefs}
      />
      <Dialog open={renameOpen} onClose={() => setRenameOpen(false)} maxWidth="xs" fullWidth>
        <DialogTitle>
          Rename {fileName.slice(fileName.lastIndexOf('/') + 1) || fileName}
        </DialogTitle>
        <DialogContent>
          <TextField
            autoFocus
            fullWidth
            size="small"
            variant="outlined"
            aria-label="New file name"
            value={renameDraft}
            onChange={(e) => setRenameDraft(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter' && renameDraft.trim()) {
                setRenameOpen(false);
                void handleRename(fileName, renameDraft.trim());
              }
            }}
          />
        </DialogContent>
        <DialogActions>
          <Button onClick={() => setRenameOpen(false)}>Cancel</Button>
          <Button
            variant="contained"
            disabled={!renameDraft.trim()}
            onClick={() => {
              setRenameOpen(false);
              if (renameDraft.trim()) void handleRename(fileName, renameDraft.trim());
            }}
          >
            Rename
          </Button>
        </DialogActions>
      </Dialog>
      <Dialog open={goToOpen} onClose={() => setGoToOpen(false)} maxWidth="xs">
        <DialogTitle>Go to Line</DialogTitle>
        <DialogContent>
          <TextField
            autoFocus
            fullWidth
            size="small"
            variant="outlined"
            aria-label="Line number"
            value={goToDraft}
            onChange={(e) => setGoToDraft(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') {
                const n = parseInt(goToDraft, 10);
                if (Number.isFinite(n)) viewportRef.current?.goToLine(n);
                setGoToOpen(false);
              }
            }}
            slotProps={{ htmlInput: { inputMode: 'numeric' } }}
          />
        </DialogContent>
        <DialogActions>
          <Button onClick={() => setGoToOpen(false)}>Cancel</Button>
          <Button
            variant="contained"
            onClick={() => {
              const n = parseInt(goToDraft, 10);
              if (Number.isFinite(n)) viewportRef.current?.goToLine(n);
              setGoToOpen(false);
            }}
          >
            Go
          </Button>
        </DialogActions>
      </Dialog>
      <Dialog open={aboutOpen} onClose={() => setAboutOpen(false)} maxWidth="xs">
        <DialogTitle>About Maleficium</DialogTitle>
        <DialogContent>
          <Typography variant="body2">
            Maleficium — desktop-native LaTeX editor (Tauri 2 + React + Tectonic sidecar).
          </Typography>
          <Typography variant="caption" color="text.secondary" sx={{ display: 'block', mt: 1 }}>
            Version 0.1.0 · offline-first · Linux-first.
          </Typography>
          <Typography variant="caption" color="text.secondary" sx={{ display: 'block', mt: 1 }}>
            SyncTeX navigation by Jérôme Laurens (MIT) — bundled sidecar.
          </Typography>
        </DialogContent>
        <DialogActions>
          <Button variant="contained" onClick={() => setAboutOpen(false)}>
            Close
          </Button>
        </DialogActions>
      </Dialog>
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
        message={log}
      />
    </Box>
  );
}
