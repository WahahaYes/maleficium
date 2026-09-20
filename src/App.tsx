import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
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
import StatusBar from './components/StatusBar';
import Pane, { PaneSplitter } from './components/Pane';
import {
  openProject,
  listDir1Level,
  loadTex,
  saveTex,
  saveTexToDisk,
  createFile,
  renamePath,
  isPreviewable,
  LARGE_FILE_BYTES,
  TreeEntry,
} from './lib/files';
import {
  getOrCreateBuffer,
  updateBuffer,
  markSaved,
  enforceBufferCap,
  type BufferState,
} from './lib/buffers';
import { compileTex, onCompileLine, cancelCompile } from './lib/compile';
import { emitPdf, onPdf } from './lib/preview-bus';
import {
  forward_sync,
  inverse_sync,
  isForwardNoMatch,
  parseForwardSync,
  parseInverseSync,
} from './lib/synctex';
import { emit } from './lib/events';
import { parseLog } from './lib/parseLog';
import { parseOutline, type OutlineEntry } from './lib/outline';
import { matchesCompile, matchesForwardSync, matchesMenuChord, menuChordId } from './lib/keymap';
import { buildMenus, presetOf, type CommandActions, type MenuContext } from './lib/commands';
import { listTreeDeep } from './lib/files';
import { resolveMainFileTauri, setMainFile } from './lib/mainFile.tauri';
import { FileHistory } from './lib/file-history';
import { getRecentProjects, touchRecentProject, pruneRecentProjects } from './lib/recentProjects';
import { moveToTrash, undoTrash } from './lib/trash';
import { createHistoryStore } from './lib/history';
import {
  buildRevisionRows,
  historyAvailability,
  retentionSummary,
  truncationNotice,
  type HistoryAvailability,
  type RevisionRow,
} from './lib/history.view';
import { appOutDir, hashRoot } from './lib/paths';
import { grantProjectAccess } from './lib/projectAccess';
import { watch, readTextFile, mkdir, stat } from '@tauri-apps/plugin-fs';
import { coalesceEvents, classifyTauriEvent, debounce } from './lib/watcher';

const HELLO = '\\documentclass{article}\n\\begin{document}\nHello Maleficium\n\\end{document}\n';

export default function App({
  themeMode = 'dark',
  onThemeMode = () => {},
  density = 'comfortable',
  onDensityMode = () => {},
}: {
  themeMode?: 'dark' | 'light';
  onThemeMode?: (m: 'dark' | 'light') => void;
  density?: 'comfortable' | 'compact';
  onDensityMode?: (d: 'comfortable' | 'compact') => void;
}) {
  const [tex, setTex] = useState(HELLO);
  const [root, setRoot] = useState<string | null>(null);
  const [tree, setTree] = useState<TreeEntry[]>([]);
  const [fileName, setFileName] = useState('hello.tex');
  const [mainFile, setMainFileState] = useState<string | null>(null);
  const [mainSource, setMainSource] = useState('');
  const [mainCandidates, setMainCandidates] = useState<string[]>([]);
  const [buffers, setBuffers] = useState<Map<string, BufferState>>(new Map());
  const [trash] = useState(() => new FileHistory());
  const [revisionCount, setRevisionCount] = useState(0);
  const [historyOpen, setHistoryOpen] = useState(false);
  const [historyRows, setHistoryRows] = useState<RevisionRow[]>([]);
  const [historyAvail, setHistoryAvail] = useState<HistoryAvailability>('unavailable');
  const [historySummary, setHistorySummary] = useState<string | null>(null);
  const [historyNotice, setHistoryNotice] = useState<string | null>(null);
  const [restoringRev, setRestoringRev] = useState<string | null>(null);
  const [reloadPath, setReloadPath] = useState<string | null>(null);
  const [log, setLog] = useState('ready');
  const [largeFile, setLargeFile] = useState<string | null>(null);
  // Non-text selection (image/video/pdf/binary): rich preview, never the editor.
  const [previewFile, setPreviewFile] = useState<string | null>(null);
  const [pdfUrl, setPdfUrl] = useState<string | null>(null);
  const [pdfStamp, setPdfStamp] = useState(0);
  const [currentLine, setCurrentLine] = useState(1);
  // Ref mirror: the subscribe-once listener reads forward SyncTeX via ref,
  // never state.
  const currentLineRef = useRef(currentLine);
  currentLineRef.current = currentLine;
  // Bumped on every inverse SyncTeX hit to flash the line amber.
  const [synctexFlash, setSynctexFlash] = useState(0);
  // Ref mirror for the watcher closure (the effect is root-scoped).
  const fileNameRef = useRef(fileName);
  fileNameRef.current = fileName;
  const buffersRef = useRef(buffers);
  buffersRef.current = buffers;
  // Latest closures for the subscribe-once global keymap listener.
  const compileRef = useRef<() => Promise<void>>(async () => {});
  const forwardSyncRef = useRef<() => Promise<void>>(async () => {});
  const handleSelectRef = useRef<(path: string) => Promise<void>>(async () => {});
  // Latest tree selection wins: rapid clicks resolve out of order otherwise.
  const selectTokenRef = useRef(0);
  // Paths WE just wrote (save/autosave/compile persist/undo): watcher echoes of
  // our own writes must not raise the reload banner. Windowed suppression.
  const ownWritesRef = useRef<Map<string, number>>(new Map());
  const markOwnWrite = useCallback((p: string) => {
    ownWritesRef.current.set(p, Date.now());
  }, []);

  // Revision history: app-local, keyed by a project id that never resolves to
  // a path outside the store. One project is open at a time, so `rootFor`
  // answers for that id alone.
  const rootRef = useRef<string | null>(root);
  rootRef.current = root;
  const projectId = root ? hashRoot(root) : null;
  const history = useMemo(
    () =>
      createHistoryStore((id) => {
        const r = rootRef.current;
        return r && hashRoot(r) === id ? r : null;
      }),
    [],
  );
  /** Project-relative path for a file inside the open project, else null. */
  const relInProject = useCallback((abs: string): string | null => {
    const r = rootRef.current;
    return r && abs.startsWith(r + '/') ? abs.slice(r.length + 1) : null;
  }, []);
  const refreshRevisionCount = useCallback(
    async (path: string) => {
      const rel = relInProject(path);
      if (!projectId || !rel) {
        setRevisionCount(0);
        return;
      }
      setRevisionCount((await history.listRevisions(projectId, rel)).length);
    },
    [history, projectId, relInProject],
  );
  /** Snapshot a saved file. Ineligible files and an unreachable store are quiet. */
  const recordRevision = useCallback(
    async (path: string, text: string) => {
      const rel = relInProject(path);
      if (!projectId || !rel) return;
      await history.recordRevision(projectId, rel, new TextEncoder().encode(text));
      await refreshRevisionCount(path);
    },
    [history, projectId, relInProject, refreshRevisionCount],
  );

  useEffect(() => onPdf(setPdfUrl), []);

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
        emit({ scope: 'fs', kind: 'info', message: 'previewing ' + path });
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
        emit({ scope: 'fs', kind: 'info', message: 'switched ' + path });
        if (root && path.endsWith('.tex')) void resolveMain(root, path);
        return;
      }
      emit({ scope: 'fs', kind: 'progress', message: 'loading ' + path });
      setLog('loading ' + path);
      try {
        const info = await stat(path).catch(() => null);
        const size = info?.size ?? 0;
        if (size > LARGE_FILE_BYTES) {
          if (selectToken !== selectTokenRef.current) return; // stale click lost the race
          setLargeFile(path);
          setFileName(path);
          setLog(`large file (${Math.round(size / 1024)}KB) — preview only`);
          emit({ scope: 'fs', kind: 'warn', message: `large file placeholder ${path} (${size}B)` });
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
        emit({ scope: 'fs', kind: 'success', message: 'loaded ' + path });
        if (root && path.endsWith('.tex')) void resolveMain(root, path);
      } catch (e) {
        setLog('load failed: ' + String(e).slice(0, 120));
        emit({ scope: 'fs', kind: 'error', message: 'load failed ' + path });
      }
    },
    [buffers, fileName, root, markOwnWrite, recordRevision],
  );

  useEffect(() => {
    const h = () => {
      cancelCompile().catch(() => {
        /* exiting — nothing to report */
      });
    };
    window.addEventListener('beforeunload', h);
    return () => window.removeEventListener('beforeunload', h);
  }, []);

  const resolveMain = useCallback(async (r: string, opened: string | null) => {
    const res = await resolveMainFileTauri(r, opened);
    setMainFileState(res.mainFile);
    setMainSource(res.source);
    setMainCandidates(res.candidates);
    return res.mainFile;
  }, []);

  // UI tree is 1 level + expand-on-demand. The recursive walk runs only
  // for main-file scan + watcher baseline, never on the open path.
  const reloadTree = useCallback(async (r: string, deep = false) => {
    const t0 = performance.now();
    const t = deep ? await listTreeDeep(r) : await listDir1Level(r);
    setTree(t);
    const dt = Math.round(performance.now() - t0);
    emit({
      scope: 'fs',
      kind: 'info',
      message: `tree ${deep ? 'full' : 'root'} loaded ${t.length} rows in ${dt}ms`,
    });
  }, []);

  // Watcher: notify + debounce/coalesce. Tree refreshes on create/rename;
  // open-file edits offer reload; on-disk deletes mark the buffer.
  useEffect(() => {
    if (!root) return;
    let unwatch: (() => void) | null = null;
    let cancelled = false;
    const pending: { kind: 'create' | 'modify' | 'delete'; path: string }[] = [];
    const flush = debounce(() => {
      if (cancelled || pending.length === 0) return;
      const batch = coalesceEvents(pending.splice(0));
      void reloadTree(root);
      const now = Date.now();
      for (const ev of batch) {
        // Suppress echoes of our own writes (5s window, pruned here).
        const own = ownWritesRef.current.get(ev.path);
        if (own != null && now - own < 5000) continue;
        if (own != null) ownWritesRef.current.delete(ev.path);
        if (ev.path === fileNameRef.current && ev.kind === 'modify') setReloadPath(ev.path);
        if (ev.path === fileNameRef.current && ev.kind === 'delete') {
          setLog('deleted on disk: ' + ev.path);
          emit({ scope: 'fs', kind: 'warn', message: 'deleted on disk: ' + ev.path });
        }
        emit({ scope: 'fs', kind: 'info', message: `external ${ev.kind} ${ev.path}` });
      }
    }, 250);
    watch(
      root,
      (ev) => {
        for (const c of classifyTauriEvent(ev)) pending.push(c);
        flush();
      },
      { recursive: true, delayMs: 250 },
    ).then(
      (u) => {
        if (!cancelled) unwatch = u;
        else u();
      },
      () => {
        /* watcher unavailable (web fallback) — tree still works via manual reload */
      },
    );
    return () => {
      cancelled = true;
      if (unwatch) unwatch();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [root]);

  // Recents for File > Open Recent as state. Restored entries re-validate
  // via stat.
  const [recentProjects, setRecentProjects] = useState<string[]>(() => getRecentProjects());
  async function openRoot(r: string, opts?: { warm?: boolean }) {
    // The runtime scope grant comes first: every fs call below resolves
    // through it. The backend fails closed on invalid roots
    // (empty/NUL/relative/missing/non-dir); a failed grant leaves the
    // current project untouched.
    const grant = await grantProjectAccess(r);
    if (!grant.ok || !grant.path) {
      const msg = 'open refused: ' + (grant.error ?? 'grant failed').slice(0, 200);
      setLog(msg);
      emit({ scope: 'fs', kind: 'error', message: msg });
      return;
    }
    const canon = grant.path;
    setRoot(canon);
    setRecentProjects(touchRecentProject(canon));
    await reloadTree(canon, false);
    setLog('opened ' + canon);
    emit({ scope: 'fs', kind: 'info', message: 'opened ' + canon });
    trash.clear();
    const m = await resolveMain(canon, null);
    setLog(m ? `opened ${canon} (main: ${m})` : `opened ${canon} (no main file found)`);
    // Open the main file on project select — the editor must never sit on
    // stale untitled content while the tree shows a project. No main →
    // keep the current editor as-is.
    if (m) await handleSelect(m);
    // Cache-warm on open: a background compile starts after the editor is
    // populated — but only when the engine cache is usable (previous output
    // present). No cache → no surprise build; the preview waits for the
    // user's explicit Ctrl+R. Open never fails because warm failed.
    if (opts?.warm && m) void warmCompile(m);
  }

  async function open() {
    const r = await openProject();
    if (r) {
      await openRoot(r, { warm: true });
    } else {
      setLog('open cancelled');
      emit({ scope: 'fs', kind: 'warn', message: 'cancelled' });
    }
  }

  // Restore-on-launch: the `?project=` preset wins, else the most recent
  // project that still resolves, else the Hello sample (no project forced).
  // Runs once; only roots that all fail validation are pruned.
  const restoredRef = useRef(false);
  useEffect(() => {
    if (restoredRef.current) return;
    restoredRef.current = true;
    void (async () => {
      try {
        const q = new URLSearchParams(window.location.search);
        const h = window.location.hash.match(/project=([^&]+)/);
        if (q.get('project') || h) return; // openProject() preset path owns it
      } catch {
        /* non-browser — fall through to recents */
      }
      const recents = getRecentProjects();
      const stale: string[] = [];
      for (const r of recents) {
        try {
          // Grant first: the validation stat below resolves only through the
          // runtime grant. Unreachable entries land in `stale` here.
          const grant = await grantProjectAccess(r);
          if (!grant.ok || !grant.path) throw new Error(grant.error ?? 'grant failed');
          await stat(grant.path);
          await openRoot(grant.path, { warm: true });
          return;
        } catch {
          stale.push(r);
        }
      }
      if (stale.length > 0 && stale.length === recents.length) {
        setRecentProjects(pruneRecentProjects((kept) => !stale.includes(kept)));
      }
    })();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Tree CRUD: create/rename via plugin-fs; own-write marks suppress echoes.
  async function handleCreate(dirPath: string, name: string) {
    try {
      const full = await createFile(dirPath, name);
      markOwnWrite(full);
      emit({ scope: 'fs', kind: 'success', message: 'created ' + full });
      if (root) await reloadTree(root, false);
      await handleSelect(full);
    } catch (e) {
      emit({ scope: 'fs', kind: 'error', message: 'create failed: ' + String(e).slice(0, 120) });
    }
  }

  async function handleRename(oldPath: string, newName: string) {
    try {
      const full = await renamePath(oldPath, newName);
      markOwnWrite(oldPath);
      markOwnWrite(full);
      setBuffers((b) => {
        const prev = b.get(oldPath);
        if (!prev) return b;
        const n = new Map(b);
        n.delete(oldPath);
        n.set(full, prev);
        return n;
      });
      if (fileName === oldPath) {
        setFileName(full);
        setReloadPath(null);
      }
      emit({ scope: 'fs', kind: 'success', message: `renamed to ${full}` });
      if (root) await reloadTree(root, false);
    } catch (e) {
      emit({ scope: 'fs', kind: 'error', message: 'rename failed: ' + String(e).slice(0, 120) });
    }
  }

  async function handleReload() {
    if (!reloadPath) return;
    try {
      const content = await loadTex(reloadPath);
      setBuffers((b) => {
        const n = new Map(b);
        n.set(reloadPath, {
          value: content,
          dirty: false,
          version: (n.get(reloadPath)?.version ?? 0) + 1,
        });
        return n;
      });
      if (reloadPath === fileName) setTex(content);
      setReloadPath(null);
      emit({ scope: 'fs', kind: 'success', message: 'reloaded ' + reloadPath });
    } catch (e) {
      emit({ scope: 'fs', kind: 'error', message: 'reload failed: ' + String(e).slice(0, 120) });
    }
  }

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
    emit({ scope: 'fs', kind: 'info', message: 'closed ' + path });
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
    emit({ scope: 'fs', kind: 'info', message: `closed ${n} other file${n === 1 ? '' : 's'}` });
  }

  async function handleCloseAll() {
    const paths = [...buffers.keys()];
    let n = 0;
    for (const p of paths) {
      if (await closeBufferQuiet(p)) n++;
      else break;
    }
    emit({ scope: 'fs', kind: 'info', message: `closed ${n} file${n === 1 ? '' : 's'}` });
  }

  async function handleDelete(path: string) {
    if (!root) return;
    const r = await moveToTrash(trash, root, path);
    if (r.ok) {
      emit({
        scope: 'fs',
        kind: 'success',
        message: `deleted ${path} (Edit → Undo Delete restores it)`,
      });
      setBuffers((b) => {
        const n = new Map(b);
        n.delete(path);
        return n;
      });
      if (previewFile === path) setPreviewFile(null);
      await reloadTree(root);
    } else {
      emit({
        scope: 'fs',
        kind: 'error',
        message: 'delete failed: ' + (r.error ?? '').slice(0, 120),
      });
    }
  }

  async function handleClean() {
    const target = mainFile ?? (fileName.includes('/') ? fileName : null);
    if (!target || !target.includes('/')) {
      emit({
        scope: 'compile',
        kind: 'warn',
        message: 'Clean: nothing to clean (no project file)',
      });
      return;
    }
    // App-local outdir over the app-cache dir: clean never touches the
    // project dir.
    const { appCacheDir } = await import('@tauri-apps/api/path');
    const dir = target.slice(0, target.lastIndexOf('/')) || '/tmp';
    const out = appOutDir(await appCacheDir(), dir);
    try {
      // Per-entry removal: build artifacts only, never sources. Missing
      // dir = already clean.
      const { readDir, remove } = await import('@tauri-apps/plugin-fs');
      let entries = [];
      try {
        entries = await readDir(out);
      } catch {
        emit({ scope: 'compile', kind: 'info', message: 'Clean: already clean' });
        return;
      }
      let n = 0;
      for (const e of entries) {
        try {
          await remove(out + '/' + e.name);
          n++;
        } catch {
          /* keep going — report count at end */
        }
      }
      markOwnWrite(out);
      emit({ scope: 'compile', kind: 'success', message: `Cleaned ${out} (${n} files)` });
      if (root) await reloadTree(root, false);
    } catch (e) {
      emit({
        scope: 'compile',
        kind: 'error',
        message: 'Clean failed: ' + String(e).slice(0, 120),
      });
    }
  }

  async function handleUndo() {
    const r = await undoTrash(trash);
    if (r.ok) {
      emit({ scope: 'fs', kind: 'success', message: 'restored from trash' });
      if (root) {
        await reloadTree(root);
      }
    } else {
      emit({
        scope: 'fs',
        kind: 'error',
        message: 'undo failed: ' + (r.error ?? '').slice(0, 120),
      });
    }
  }

  async function handleSetMain() {
    if (!root || !fileName.includes('/')) return;
    await setMainFile(root, fileName);
    const m = await resolveMain(root, fileName);
    setLog('main file: ' + (m ?? '(none)'));
    emit({ scope: 'fs', kind: 'success', message: 'main file set: ' + (m ?? '(none)') });
  }

  // Main-file tie-break: the scan found >1 `\documentclass` and picked the
  // first. Choosing here writes the explicit association, so the tie never
  // reappears for this project.
  async function handlePickMain(path: string) {
    if (!root) return;
    await setMainFile(root, path);
    await resolveMain(root, path);
    setMainAnchor(null);
    emit({ scope: 'fs', kind: 'success', message: 'main file set: ' + path });
  }

  // Tree-driven main association (double-click / context menu on a .tex row).
  async function handleSetMainPath(path: string) {
    if (!root) return;
    await handleSelect(path);
    await setMainFile(root, path);
    const m = await resolveMain(root, path);
    setLog('main file: ' + (m ?? '(none)'));
    emit({ scope: 'fs', kind: 'success', message: 'main file set: ' + (m ?? '(none)') });
  }

  // One-off compile of a tree-selected file. This is EXPECTED to fail for
  // fragments (a chapter without \documentclass cannot build alone) — the
  // engine error is the honest answer, surfaced through the normal failure
  // path (phase + stream + click-to-jump rows).
  async function handleCompileFile(path: string) {
    if (!path.endsWith('.tex')) {
      emit({ scope: 'compile', kind: 'error', message: 'compile blocked: open a .tex file first' });
      return;
    }
    emit({
      scope: 'compile',
      kind: 'info',
      message: `compiling ${path} directly (one-off, not the main file)`,
    });
    await runCompile(path);
  }

  // Keymap listener subscribes once and reads via refs. Menu chords
  // dispatch through the same command registry refs — one path, no
  // duplicates. Double-click in the editor = forward SyncTeX from the
  // caret line (complements single-click inverse on the PDF canvas).
  const menuActionRef = useRef<(id: string) => void>(() => {});
  const forwardSyncLineRef = useRef<(file: string, line: number) => void>(() => {});
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
  }, []);

  const workdirHint = fileName.includes('/')
    ? fileName.slice(0, fileName.lastIndexOf('/'))
    : '/tmp/maleficium-untitled';
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
      });
      return;
    }
    if (fileName.includes('/')) {
      const cur = buffers.get(fileName);
      await saveTex(fileName, cur?.value ?? tex);
      markOwnWrite(fileName);
      setBuffers((b) => markSaved(b, fileName));
      await recordRevision(fileName, cur?.value ?? tex);
      setLog('saved ' + fileName);
      emit({ scope: 'fs', kind: 'success', message: 'saved ' + fileName });
    } else {
      await saveTexToDisk(fileName, tex);
      setLog('saved ' + fileName);
      emit({ scope: 'fs', kind: 'success', message: 'saved ' + fileName });
    }
  }, [fileName, tex, buffers, largeFile, markOwnWrite, recordRevision]);

  // The counter follows the active file; a file outside the project reads 0.
  useEffect(() => {
    void refreshRevisionCount(fileName);
  }, [fileName, refreshRevisionCount]);

  const openHistory = useCallback(async () => {
    const rel = relInProject(fileName);
    const avail = historyAvailability({
      hasProject: root != null,
      relPath: rel,
      storeReady: projectId != null,
    });
    setHistoryAvail(avail);
    setHistoryOpen(true);
    if (avail !== 'ready' || !projectId || !rel) {
      setHistoryRows([]);
      setHistorySummary(null);
      setHistoryNotice(null);
      return;
    }
    const [revs, info] = await Promise.all([
      history.listRevisions(projectId, rel),
      history.retentionInfo(projectId),
    ]);
    const rows = buildRevisionRows(revs, Date.now());
    setHistoryRows(rows);
    setHistorySummary(retentionSummary(info));
    setHistoryNotice(truncationNotice(rows.length, info));
    setRevisionCount(revs.length);
  }, [fileName, history, projectId, relInProject, root]);

  // Restore writes the revision back to disk; the store keeps the replaced
  // state as a revision of its own, so the list is the way back.
  const restoreRevision = useCallback(
    async (rev: string) => {
      const rel = relInProject(fileName);
      if (!projectId || !rel) return;
      setRestoringRev(rev);
      try {
        const bytes = await history.restoreRevision(projectId, rel, rev);
        if (!bytes) {
          setLog('restore unavailable');
          emit({ scope: 'fs', kind: 'warn', message: 'restore unavailable for ' + rel });
          return;
        }
        const text = new TextDecoder().decode(bytes);
        markOwnWrite(fileName);
        setBuffers((b) => markSaved(updateBuffer(b, fileName, text), fileName));
        setTex(text);
        setReloadPath(null);
        setLog('restored ' + rel);
        emit({ scope: 'fs', kind: 'success', message: 'restored ' + rel });
      } finally {
        setRestoringRev(null);
      }
      await openHistory();
    },
    [fileName, history, markOwnWrite, openHistory, projectId, relInProject],
  );

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
            setLog('autosaved ' + new Date().toTimeString().slice(0, 8));
          })
          .catch(() => {
            /* autosave best-effort — dirty flag stays */
          });
      }
    }, 1200);
    return () => clearTimeout(t);
  }, [tex, fileName, buffers, markOwnWrite, recordRevision]);

  // Publish engine-log problems as first-class stream events (click-to-jump).
  const publishProblems = (text: string, base: string, wsRoot: string) => {
    try {
      const entries = parseLog(text, wsRoot, base).slice(0, 100);
      for (const l of entries) {
        emit({
          scope: 'compile',
          kind: l.clickable ? 'error' : 'warn',
          message: `${l.file}:${l.line} ${l.msg}`,
          data: { file: l.file, line: l.line, clickable: l.clickable },
        });
      }
    } catch {
      /* parse never blocks the stream */
    }
  };

  async function compile() {
    await runCompile(mainFile ?? (fileName.includes('/') ? fileName : null));
  }

  // Shared compile runner: `target` is the main-file target, or an explicit
  // one-off file. `skipPersist` is the warm-open path only: the target was
  // just loaded from disk, so there is nothing to persist — warm never
  // writes. All other callers persist through the ownership check.
  // Reentrancy: a second call while `compiling` is a no-op returning false
  // (warm-on-open and double-Ctrl+R collapse into one run — never two
  // engine children). Returns true when this call owned the run. The gate
  // reads a ref (not state) so a warm run racing a user Ctrl+R in the same
  // tick still collapses.
  const phaseRef = useRef('idle');
  async function runCompile(
    target: string | null,
    opts?: { skipPersist?: boolean },
  ): Promise<boolean> {
    if (phaseRef.current === 'compiling') return false;
    phaseRef.current = 'compiling';
    const finish = (phase: string) => {
      phaseRef.current = phase;
      setCompilePhase(phase);
    };
    if (largeFile || previewFile) {
      emit({ scope: 'compile', kind: 'error', message: 'compile blocked: open a .tex file first' });
      finish('failure');
      setLog('compile blocked: open a .tex file first');
      return false;
    }
    emit({ scope: 'compile', kind: 'progress', message: 'compiling ' + (target ?? fileName) });
    finish('compiling');
    setCompileStart(Date.now());
    setCompileTimer(0);
    setLog('compiling...');
    // Write-then-compile: the engine reads from disk, so persist first.
    let workdir: string;
    let unlisten: () => void = () => {};
    // `note: downloading <pkg>` lines get their own download-wait signal
    // so a long first build reads as network-wait, not an engine hang.
    const isDownloadLine = (l: string) => /(^|\s)downloading\s/i.test(l);
    try {
      unlisten = await onCompileLine((line) => {
        const s = String(line);
        if (isDownloadLine(s))
          emit({
            scope: 'compile',
            kind: 'info',
            message: 'downloading ' + s.replace(/^.*downloading\s+/i, '').slice(0, 120),
          });
        else emit({ scope: 'compile', kind: 'progress', message: s.slice(0, 300) });
      });
    } catch {
      /* listener attach best-effort — compile proceeds without live lines */
    }
    const t0 = Date.now();
    const hb = setInterval(
      () =>
        emit({
          scope: 'compile',
          kind: 'progress',
          message: `still compiling ${target ?? fileName} (${Math.floor((Date.now() - t0) / 1000)}s)`,
        }),
      5000,
    );
    let maxGap = 0;
    let lastT = performance.now();
    let probing = true;
    const tickProbe = () => {
      if (!probing) return;
      const now = performance.now();
      maxGap = Math.max(maxGap, now - lastT);
      lastT = now;
      requestAnimationFrame(tickProbe);
    };
    requestAnimationFrame(tickProbe);
    // Anti-clobber check: the persist step writes editor content to `target`.
    // Two invariants make that safe: (1) the visible editor must actually
    // own `target` (buffered, or the untitled flow); (2) `target` must be a
    // contained project file or an explicit one-off .tex — never a bare
    // untitled name resolved against a project dir. Violations abort before
    // any write. Warm skips both check and persist (disk is fresh, warm
    // never writes).
    const skipPersist = opts?.skipPersist === true;
    const ownsTarget =
      target == null ||
      buffers.has(target) ||
      target === fileName ||
      (!target.includes('/') && !fileName.includes('/'));
    if (!skipPersist && target != null && target.includes('/') && !ownsTarget) {
      finish('failure');
      setCompileStart(null);
      probing = false;
      emit({
        scope: 'compile',
        kind: 'error',
        message: `compile refused: editor does not own ${target} (open it first)`,
      });
      setLog(`compile refused: editor does not own ${target}`);
      clearInterval(hb);
      try {
        unlisten();
      } catch {
        /* already detached */
      }
      return false;
    }
    try {
      if (target && target.includes('/')) {
        if (!skipPersist) {
          // Persist ALL dirty buffers so \input parts compile from disk.
          for (const [p, buf] of buffers) {
            if (buf.dirty) {
              try {
                await saveTex(p, buf.value);
                markOwnWrite(p);
              } catch {
                /* keep dirty, reported at finish */
              }
            }
          }
          setBuffers((b) => {
            let n = b;
            for (const [p, buf] of b) if (buf.dirty) n = markSaved(n, p);
            return n;
          });
          // Also persist the visible editor if it was never buffered (untitled flow).
          if (!buffers.has(target)) {
            await saveTex(target, tex);
            markOwnWrite(target);
          }
        }
        workdir = target.slice(0, target.lastIndexOf('/')) || '/tmp';
      } else {
        workdir = '/tmp/maleficium-untitled';
        await mkdir(workdir, { recursive: true });
        const t2 = workdir + '/' + fileName;
        await saveTex(t2, tex);
        markOwnWrite(t2);
        setMainFileState(t2);
      }
    } catch (e) {
      finish('failure');
      setCompileStart(null);
      probing = false;
      emit({ scope: 'compile', kind: 'error', message: 'save failed: ' + String(e).slice(0, 200) });
      clearInterval(hb);
      try {
        unlisten();
      } catch {
        /* already detached */
      }
      return false;
    }
    const activeTarget = target ?? (fileName.includes('/') ? fileName : workdir! + '/' + fileName);
    const main = activeTarget.slice(activeTarget.lastIndexOf('/') + 1);
    const r = await compileTex(activeTarget, workdir!);
    // `compile_tex` returns the pdf path on success, empty error string;
    // the status line shows the pdf path or the failure message.
    setLog(r.ok ? (r.pdfPath ?? '') : r.log);
    const readEngineLog = async (): Promise<string | null> => {
      try {
        // App-local outdir over the app-cache dir: the engine log lives in
        // cache, never in the project.
        const { appCacheDir } = await import('@tauri-apps/api/path');
        const out = appOutDir(await appCacheDir(), workdir!);
        return await readTextFile(`${out}/${main.replace(/\.tex$/, '.log')}`);
      } catch {
        return null;
      }
    };
    if (r.ok && r.pdfPath) {
      finish('success');
      setCompileStart(null);
      setLogCollapsed(false);
      emit({ scope: 'compile', kind: 'success', message: 'compiled ' + String(r.pdfPath) });
      emitPdf(r.pdfPath);
      setPdfStamp((s) => s + 1);
      emit({ scope: 'preview', kind: 'success', message: 'preview ' + String(r.pdfPath) });
    } else if (!r.ok && r.log.includes('spawn')) {
      finish('failure');
      setCompileStart(null);
      setLogCollapsed(false);
      setLog(r.log + ' (engine sidecar failed to start)');
      emit({ scope: 'compile', kind: 'error', message: String(r.log).slice(0, 300) });
      const c = await readEngineLog();
      publishProblems(c ?? r.log, mainDir, root || workdirHint);
    } else if (!r.ok) {
      finish('failure');
      setCompileStart(null);
      setLogCollapsed(false);
      emit({ scope: 'compile', kind: 'error', message: String(r.log).slice(0, 300) });
      const c = await readEngineLog();
      publishProblems(c ?? r.log, mainDir, root || workdirHint);
    }
    clearInterval(hb);
    try {
      unlisten();
    } catch {
      /* already detached */
    }
    probing = false;
    emit({
      scope: 'compile',
      kind: 'info',
      message: `main-thread max frame ${Math.round(maxGap)}ms during compile`,
    });
    return true;
  }

  // Cache-warm on open: a background compile of a freshly opened project's
  // main file, after the editor is populated. The write phase is skipped
  // (skipPersist: disk is fresh, warm never writes). Two rules: (1) this
  // runs only when the engine cache is usable — no cache entry means a full
  // cold build, which is the user's explicit Ctrl+R to pay for, not open's;
  // (2) a warm failure is quiet (debug line only) — open must never look
  // broken because a background guess failed; the user's explicit Ctrl+R
  // reports loudly through the normal path.
  async function warmCompile(mainAbsPath: string) {
    const usable = await engineCacheUsable(mainAbsPath).catch(() => false);
    if (!usable) {
      emit({
        scope: 'compile',
        kind: 'info',
        message: 'preview will build on first Compile (no cached output)',
      });
      return;
    }
    emit({ scope: 'compile', kind: 'info', message: 'warming preview for ' + mainAbsPath });
    await runCompile(mainAbsPath, { skipPersist: true });
    // Not owned (user raced us) → their stream wins; nothing to report.
  }

  // True when the app-local outdir already holds this target's engine output
  // (pdf from a previous successful run): the warm compile then only
  // verifies freshness instead of paying a full cold build on every open.
  // Best-effort stat only — never throws. The log is not required here:
  // the engine does not reliably leave one beside every pdf.
  async function engineCacheUsable(targetAbsPath: string): Promise<boolean> {
    try {
      const { appCacheDir } = await import('@tauri-apps/api/path');
      const dir = targetAbsPath.slice(0, targetAbsPath.lastIndexOf('/')) || '/tmp';
      const stem = targetAbsPath.slice(targetAbsPath.lastIndexOf('/') + 1).replace(/\.tex$/, '');
      const out = appOutDir(await appCacheDir(), dir);
      const { stat: statFile } = await import('@tauri-apps/plugin-fs');
      await statFile(`${out}/${stem}.pdf`);
      return true;
    } catch {
      return false;
    }
  }

  async function handleForwardSync() {
    if (!pdfUrl) return;
    if (compilePhase === 'compiling') {
      emit({
        scope: 'preview',
        kind: 'warn',
        message: 'SyncTeX unavailable while compiling (synctex_no_match)',
      });
      return;
    }
    // The caret may have moved since the last jump: read the live line from
    // the viewport bridge (currentLine only tracks jumps, not caret moves).
    const liveLine = viewportRef.current?.caretLine() ?? currentLineRef.current;
    if (liveLine !== currentLineRef.current) setCurrentLine(liveLine);
    const texPath = fileName.includes('/') ? fileName : workdirHint + '/' + fileName;
    const result = await forward_sync(pdfUrl, texPath, liveLine);
    if (!result.ok) {
      emit({
        scope: 'preview',
        kind: 'warn',
        message: `SyncTeX query failed (${result.text.slice(0, 200)})`,
      });
      return;
    }
    if (isForwardNoMatch(result.text)) {
      emit({ scope: 'preview', kind: 'warn', message: 'synctex_no_match' });
      return;
    }
    const target = parseForwardSync(result.text);
    if (target == null) {
      emit({
        scope: 'preview',
        kind: 'info',
        message: `forward SyncTeX → ${result.text.slice(0, 120)}`,
      });
      return;
    }
    // Preamble/untagged lines resolve to a same-page rect with no movement:
    // arriving without moving is noise, not navigation — stay silent.
    if (target === pageNumberRef.current) return;
    setPageNumber(target);
    emit({ scope: 'preview', kind: 'info', message: `forward SyncTeX → page ${target}` });
  }

  async function handleInverseSync(page: number, x: number, y: number) {
    if (!pdfUrl) return;
    if (compilePhase === 'compiling') {
      emit({
        scope: 'preview',
        kind: 'warn',
        message: 'synctex_no_match: disabled during compile',
      });
      return;
    }
    const result = await inverse_sync(pdfUrl, page, x, y);
    if (!result.ok) {
      emit({
        scope: 'preview',
        kind: 'warn',
        message: `SyncTeX query failed (${result.text.slice(0, 200)})`,
      });
      return;
    }
    // Real `synctex edit` shape:
    //   Input:/abs/path/hello.tex\nLine:7\n...
    const { line, hitFile } = parseInverseSync(result.text);
    if (line != null) {
      // Jump the owning file when SyncTeX names one (multi-file projects);
      // otherwise reveal the line in the current buffer.
      if (hitFile && hitFile !== fileName) {
        try {
          const content = await loadTex(hitFile);
          setBuffers((b) => {
            const n = new Map(b);
            getOrCreateBuffer(n, hitFile as string, content);
            return enforceBufferCap(n, fileNameRef.current);
          });
          setTex(content);
          setFileName(hitFile as string);
          setPreviewFile(null);
          setLargeFile(null);
          setReloadPath(null);
        } catch {
          /* unreadable hit file — still reveal the line number below */
        }
      }
      setCurrentLine(line);
      setSynctexFlash((f) => f + 1);
      emit({
        scope: 'preview',
        kind: 'success',
        message: `synctex inverse → ${hitFile ?? fileName}:${line}`,
      });
    } else {
      emit({
        scope: 'preview',
        kind: 'warn',
        message: 'SyncTeX: no match at this position (synctex_no_match)',
      });
    }
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
  compileRef.current = compile;
  forwardSyncRef.current = handleForwardSync;
  handleSelectRef.current = handleSelect;
  // Forward SyncTeX from an explicit file + line (editor double-click).
  // The file is captured at click time — fileName state may lag the
  // visible buffer after a fast file switch + double-click.
  forwardSyncLineRef.current = (file: string, line: number) => {
    if (!pdfUrl || compilePhase === 'compiling') return;
    if (line !== currentLineRef.current) setCurrentLine(line);
    const texPath = file.includes('/') ? file : workdirHint + '/' + file;
    void forward_sync(pdfUrl, texPath, line).then((result) => {
      if (isForwardNoMatch(result.text)) {
        emit({ scope: 'preview', kind: 'warn', message: 'synctex_no_match' });
        return;
      }
      if (!result.ok) {
        emit({
          scope: 'preview',
          kind: 'warn',
          message: `SyncTeX query failed (${result.text.slice(0, 200)})`,
        });
        return;
      }
      const target = parseForwardSync(result.text);
      if (target == null || target === pageNumberRef.current) return;
      setPageNumber(target);
      emit({ scope: 'preview', kind: 'info', message: `forward SyncTeX → page ${target}` });
    });
  };

  // ---- Shell state: view is explicit booleans (View menu presets own them) ----
  const [treeVisible, setTreeVisible] = useState(true);
  const [editorVisible, setEditorVisible] = useState(true);
  const [previewOpen, setPreviewOpen] = useState(true);
  const [layout, setLayout] = useState(() => {
    try {
      const raw = localStorage.getItem('maleficium.layout');
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
    try {
      localStorage.setItem('maleficium.layout', JSON.stringify(layout));
    } catch {
      /* private mode — layout just won't persist */
    }
  }, [layout]);
  const [logCollapsed, setLogCollapsed] = useState(false);
  const [pageNumber, setPageNumber] = useState(1);
  const pageNumberRef = useRef(pageNumber);
  pageNumberRef.current = pageNumber;
  const [compilePhase, setCompilePhase] = useState('idle');
  const [compileTimer, setCompileTimer] = useState(0);
  const [compileStart, setCompileStart] = useState<number | null>(null);
  const [previewCollapsed, setPreviewCollapsed] = useState(false);
  const [shortcutsOpen, setShortcutsOpen] = useState(false);
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

  // Mirror compile bus → status bar phase/timer.
  useEffect(() => {
    const t = setInterval(() => {
      if (compileStart != null) setCompileTimer(Math.floor((Date.now() - compileStart) / 1000));
    }, 500);
    return () => clearInterval(t);
  }, [compileStart]);

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
    [fileName],
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
      else emit({ scope: 'fs', kind: 'warn', message: 'New File needs an open project' });
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
    renameActive: () => {
      if (!isProjectFile(fileName)) {
        emit({
          scope: 'fs',
          kind: 'warn',
          message: 'Rename needs a project file (open one first)',
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
        stamp={pdfStamp}
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
      <Dialog open={renameOpen} onClose={() => setRenameOpen(false)} maxWidth="xs" fullWidth>
        <DialogTitle>
          Rename {fileName.slice(fileName.lastIndexOf('/') + 1) || fileName}
        </DialogTitle>
        <DialogContent>
          <TextField
            autoFocus
            fullWidth
            size="small"
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
