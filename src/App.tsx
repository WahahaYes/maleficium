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
import LogStream, { type ProblemRef } from './components/LogStream';
import { baseName, dirName, hasDir, joinPath, relUnder } from './lib/paths';
import { newMoves, type CameraMove } from './lib/devCamera';
import { createOwnWrites } from './lib/own-writes';
import OutlineView from './components/OutlineView';
import SearchPanel from './components/SearchPanel';
import PaletteDialog from './components/PaletteDialog';
import { paletteCommands } from './lib/palette';
import { hoverText } from './lib/definition.view';
import { projectIndex, type Query } from './lib/project-index';
import { historyStore } from './lib/history';
import type { Hit } from './lib/generated/index';
import HistoryDialog from './components/HistoryDialog';
import ShortcutsDialog from './components/ShortcutsDialog';
import SettingsDialog from './components/SettingsDialog';
import type { AppearancePrefs } from './lib/appearance';
import StatusBar from './components/StatusBar';
import PrecheckPanel from './components/PrecheckPanel';
import ExternalChangeDialog from './components/ExternalChangeDialog';
import { useExternalChanges } from './hooks/useExternalChanges';
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
import { cancelCompile, compileLogTitle } from './lib/compile';
import { onPdf, sourceFor, type PreviewDoc } from './lib/preview-bus';
import { emit } from './lib/events';
import { startEventLog } from './lib/eventlog';
import { historyAvailability } from './lib/history.view';
import { structure } from './lib/structure';
import type { OutlineEntry } from './lib/generated/structure';
import {
  matchesCompile,
  matchesForwardSync,
  matchesGoToDefinition,
  matchesMenuChord,
  menuChordId,
  zoomChord,
} from './lib/keymap';
import type { ZoomAction } from './lib/zoom';
import { useExport } from './hooks/useExport';
import TemplateDialogs, { type TemplateDialogMode } from './components/TemplateDialogs';
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
  const [tex, setTex] = useState(HELLO);
  const [root, setRoot] = useState<string | null>(null);
  const [projectId, setProjectId] = useState<string | null>(null);
  const [tree, setTree] = useState<TreeEntry[]>([]);
  const [fileName, setFileName] = useState('hello.tex');
  const [mainFile, setMainFileState] = useState<string | null>(null);
  const [mainSource, setMainSource] = useState('');
  const [mainCandidates, setMainCandidates] = useState<string[]>([]);
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
  // Latest tree selection wins: rapid clicks resolve out of order otherwise.
  const selectTokenRef = useRef(0);
  // Paths WE just wrote (save/autosave/compile persist/undo): watcher echoes of
  // our own writes are not external changes.
  const [ownWrites] = useState(() => createOwnWrites());
  const { buffers, setBuffers, buffersRef, handleCloseBuffer, handleCloseOthers, handleCloseAll } =
    useBufferManager({
      fileName,
      setFileName,
      previewFile,
      setPreviewFile,
      setTex,
      emptyTex: HELLO,
      setLargeFile,
      ownWrites,
    });
  const { conflicts, checkExternal, resolveExternal } = useExternalChanges({
    buffers,
    setBuffers,
    fileName,
    setTex,
    ownWrites,
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
    ownWrites,
  });

  useIndexOverlays(projectId, buffers, relInProject);

  // The bus is recorded to an app-local JSONL file for the length of the run.
  useEffect(() => {
    const log = startEventLog();
    return () => log.stop();
  }, []);

  useEffect(() => onPdf(setPreviewDoc), []);

  const resolveMain = useCallback(async (r: string, rootId: string, opened: string | null) => {
    const res = await resolveMainFileTauri(rootId, opened);
    // A resolve for a root that is no longer open never touches the open
    // project's main file.
    if (r !== rootRef.current) return null;
    setMainFileState(res.mainFile);
    setMainSource(res.source);
    setMainCandidates(res.candidates);
    return res.mainFile;
  }, []);

  const handleSelect = useCallback(
    async (path: string) => {
      // Persist current buffer before switching (dirty survives switch via map).
      if (hasDir(fileName) && path !== fileName) {
        const cur = buffers.get(fileName);
        if (cur?.dirty) {
          try {
            const outcome = await saveTex(fileName, cur.value, cur.disk);
            ownWrites.wrote(fileName, cur.value);
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
    [buffers, setBuffers, fileName, ownWrites, noteSavedRevision, resolveMain],
  );
  // Callers defined before handleSelect (tab cycling, go to definition,
  // search hits) switch files through this ref.
  handleSelectRef.current = handleSelect;

  // UI tree is 1 level + expand-on-demand. The recursive walk runs only
  // for main-file scan + watcher baseline, never on the open path.
  // Tree CRUD: create/rename via plugin-fs; own-write marks suppress echoes.

  async function handleSetMain() {
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

  // Main-file tie-break: the scan found >1 `\documentclass` and picked the
  // first. Choosing here writes the explicit association, so the tie never
  // reappears for this project.
  async function handlePickMain(path: string) {
    if (!root || !projectId) return;
    await setMainFile(projectId, root, path);
    await resolveMain(root, projectId, path);
    setMainAnchor(null);
    emit({
      scope: 'fs',
      kind: 'success',
      actor: 'user',
      message: 'main file set: ' + path,
      event: { action: 'main.set', mainFile: path },
    });
  }

  // Tree-driven main association (double-click / context menu on a .tex row).
  async function handleSetMainPath(path: string) {
    if (!root || !projectId) return;
    await handleSelect(path);
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

  // One-off compile of a tree-selected file. This is EXPECTED to fail for
  // fragments (a chapter without \documentclass cannot build alone) — the
  // engine error is the honest answer, surfaced through the normal failure
  // path (phase + stream + click-to-jump rows).

  // Keymap listener subscribes once and reads via refs. Menu chords
  // dispatch through the same command registry refs — one path, no
  // duplicates. Double-click in the editor = forward SyncTeX from the
  // caret line (complements single-click inverse on the PDF canvas).
  const menuActionRef = useRef<(id: string) => void>(() => {});
  const zoomActionRef = useRef<((a: ZoomAction) => void) | null>(null);
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const mod = e.ctrlKey || e.metaKey;
      if (matchesCompile(e as unknown as KeyboardEvent)) {
        e.preventDefault();
        void compileRef.current();
      } else if (matchesGoToDefinition(e as unknown as KeyboardEvent)) {
        e.preventDefault();
        goToDefinitionRef.current();
      } else if (matchesForwardSync(e as unknown as KeyboardEvent)) {
        e.preventDefault();
        void forwardSyncRef.current();
      } else if (zoomChord(e as unknown as KeyboardEvent)) {
        e.preventDefault();
        zoomActionRef.current?.(zoomChord(e as unknown as KeyboardEvent)!);
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
        ownWrites.wrote(fileName, text);
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
  }, [fileName, tex, buffers, setBuffers, largeFile, ownWrites, noteSavedRevision]);

  useEffect(() => {
    if (!hasDir(fileName)) return;
    const t = setTimeout(() => {
      const cur = buffers.get(fileName);
      if (cur?.dirty) {
        ownWrites.wrote(fileName, cur.value);
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
  }, [tex, fileName, buffers, setBuffers, ownWrites, noteSavedRevision]);

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
    ownWrites,
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
      ownWrites,
      checkExternal,
      setLog,
      trash,
      resolveMain,
      clearMainFile: () => {
        setMainFileState(null);
        setMainSource('');
        setMainCandidates([]);
      },
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
    ownWrites,
    reloadTree,
    handleSelect,
  });
  const [previewCollapsed, setPreviewCollapsed] = useState(false);
  const [shortcutsOpen, setShortcutsOpen] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const fileTreeVisible = treeVisible;
  const [outlineVisible, setOutlineVisible] = useState(true);
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
  const runSearch = useCallback(
    (q: Query) => {
      if (!projectId) return Promise.reject(new Error('no project open'));
      const main = mainFile ? relInProject(mainFile) : null;
      return projectIndex().search(projectId, q, main);
    },
    [projectId, mainFile, relInProject],
  );
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
    [projectId, root, buffersRef, relInProject, setBuffers],
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
  }, [projectId, root, lastReplace, buffersRef, setBuffers]);
  // A replace belongs to the project it ran in.
  useEffect(() => setLastReplace(null), [projectId]);
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
      setTreeVisible(true);
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
      void handleSetMain();
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
      setRenameDraft(baseName(fileName));
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
  // Dev builds: the video harness's camera moves (src/lib/devCamera.ts).
  const cameraRef = useRef<(m: CameraMove) => void>(() => {});
  cameraRef.current = (m: CameraMove) => {
    if (m.op === 'open') {
      if (root) void handleSelectRef.current(joinPath(root, m.rel));
    } else if (m.op === 'line') viewportRef.current?.goToLine(m.line);
    else if (m.op === 'page') setPageNumber(m.page);
    else menuActionRef.current(m.id);
  };
  useEffect(() => {
    if (!import.meta.env.DEV) return;
    let last = 0;
    let busy = false;
    const t = setInterval(() => {
      if (busy) return;
      busy = true;
      fetch('/__camera', { cache: 'no-store' })
        .then((r) => (r.ok ? r.text() : ''))
        .then((text) => {
          for (const m of newMoves(text, last)) {
            last = m.seq;
            cameraRef.current(m);
          }
        })
        .catch(() => {})
        .finally(() => {
          busy = false;
        });
    }, 250);
    return () => clearInterval(t);
  }, []);

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
            select={hitSelect && hitSelect.path === fileName ? hitSelect : undefined}
            viewportRef={viewportRef}
            onDoubleClickRef={forwardSyncLineRef}
            filePath={fileName}
            prefs={prefs}
            definitionRef={definitionRef}
          />
        </Box>
      )}
      <Typography
        variant="caption"
        title={compileLogTitle(log, compiled)}
        sx={{ display: 'block', mt: 1 }}
      >
        {log}
      </Typography>
    </Box>
  );

  const mainDoc =
    mainFile && root && projectId ? sourceFor(mainFile, [{ rootId: projectId, path: root }]) : null;
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
        mainSource={mainDoc}
        pageNumber={pageNumber}
        onPage={setPageNumber}
        onSync={handleForwardSync}
        onInverse={(page, x, y) => {
          void handleInverseSync(page, x, y);
        }}
        syncDisabled={compilePhase === 'compiling'}
        zoomActionRef={zoomActionRef}
        onZoom={(mode, percent) =>
          emit({
            scope: 'preview',
            kind: 'info',
            actor: 'user',
            message: `preview zoom ${mode.kind === 'percent' ? '' : mode.kind + ' '}${Math.round(percent)}%`,
            event: { action: 'preview.zoom', mode: mode.kind, percent: Math.round(percent) },
          })
        }
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
            {root && searchOpen ? (
              <SearchPanel
                runSearch={runSearch}
                onOpenHit={(rel, hit) => void openHit(rel, hit)}
                runPreview={runPreview}
                runApply={runApply}
                lastReplace={lastReplace}
                onUndoReplace={() => void undoReplace()}
                onClose={() => setSearchOpen(false)}
                focusKey={searchFocus}
              />
            ) : root ? (
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
                    onExpandDir={expandDir}
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
            ratio={previewVisible ? layout.editorRatio : 1}
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
            ratio={editorVisible ? layout.previewRatio : 1}
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
      <Dialog open={renameOpen} onClose={() => setRenameOpen(false)} maxWidth="xs" fullWidth>
        <DialogTitle>Rename {baseName(fileName) || fileName}</DialogTitle>
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
            Version {__APP_VERSION__} · offline-first.
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
