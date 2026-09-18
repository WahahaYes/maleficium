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
import { getOrCreateBuffer, updateBuffer, markSaved, type BufferState } from './lib/buffers';
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
  const [reloadPath, setReloadPath] = useState<string | null>(null);
  const [log, setLog] = useState('ready');
  const [largeFile, setLargeFile] = useState<string | null>(null);
  // Non-text selection (image/video/pdf/binary): rich preview, never the editor.
  const [previewFile, setPreviewFile] = useState<string | null>(null);
  const [pdfUrl, setPdfUrl] = useState<string | null>(null);
  const [pdfStamp, setPdfStamp] = useState(0);
  const [currentLine, setCurrentLine] = useState(1);
  // Ref mirror: forward SyncTeX reads via forwardSyncRef (subscribe-once
  // listener) — state would go stale the same way compile did.
  const currentLineRef = useRef(currentLine);
  currentLineRef.current = currentLine;
  // Bumped on every inverse SyncTeX hit → EditorViewport flashes the line amber.
  const [synctexFlash, setSynctexFlash] = useState(0);
  // Ref mirror for the watcher closure (effect is [root]-scoped; fileName would go stale).
  const fileNameRef = useRef(fileName);
  fileNameRef.current = fileName;
  const buffersRef = useRef(buffers);
  buffersRef.current = buffers;
  // Latest closures for the global keymap listener (subscribes once, never stale —
  // untitled typing never touches `buffers`, so dep-driven resubscription misses it).
  const compileRef = useRef<() => Promise<void>>(async () => {});
  const forwardSyncRef = useRef<() => Promise<void>>(async () => {});
  const handleSelectRef = useRef<(path: string) => Promise<void>>(async () => {});
  // Latest tree selection wins: rapid clicks resolve out of order otherwise.
  const selectTokenRef = useRef(0);
  // At most 10 open buffers (LRU persist-then-evict; dirty never lost —
  // eviction persists first, so content is always on disk before the drop).
  const MAX_BUFFERS = 10;
  const enforceBufferCap = useCallback((m: Map<string, BufferState>): Map<string, BufferState> => {
    if (m.size <= MAX_BUFFERS) return m;
    const keys = [...m.keys()];
    // Evict oldest clean non-active first; active file never evicted.
    for (const k of keys) {
      if (m.size <= MAX_BUFFERS) break;
      if (k === fileNameRef.current) continue;
      const b = m.get(k);
      if (b && !b.dirty) m.delete(k);
    }
    return m;
  }, []);
  // Paths WE just wrote (save/autosave/compile persist/undo): watcher echoes of
  // our own writes must not raise the reload banner. Windowed suppression.
  const ownWritesRef = useRef<Map<string, number>>(new Map());
  const markOwnWrite = useCallback((p: string) => {
    ownWritesRef.current.set(p, Date.now());
  }, []);

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
          return enforceBufferCap(n);
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
    [buffers, fileName, root],
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

  // UI tree is 1 level + expand-on-demand. The recursive
  // walk survives ONLY for main-file scan + watcher baseline (off open path).
  // open timing emission proves O(depth 1) on large projects.
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

  // Recents for File > Open Recent: state (not derive-per-render — the menu
  // reads it, openRoot writes it). Restored entries re-validate via stat.
  const [recentProjects, setRecentProjects] = useState<string[]>(() => getRecentProjects());
  async function openRoot(r: string, opts?: { warm?: boolean }) {
    // Trust-boundary Slice A: the runtime scope grant comes FIRST — under
    // least-privilege static caps every fs call below (tree, stat, watch,
    // main-file scan) resolves through this grant. The backend fails closed
    // on invalid roots (empty/NUL/relative/missing/non-dir); a failed grant
    // leaves the current project untouched (fail closed on the frontend too).
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
    // Open the main file on project select (data-loss guard 2026-09-14:
    // the editor must never sit on stale untitled content while the tree
    // shows a project — a compile from that state wrote the HELLO stub
    // over the real main file). No main → keep the current editor as-is.
    if (m) await handleSelect(m);
    // Cache-warm on open (user decision): a background compile starts AFTER
    // the editor is populated — but ONLY when the engine cache is usable
    // (previous output present). No cache → no surprise build; the preview
    // waits for the user's explicit Ctrl+R. runCompile is reentrancy-safe
    // (phase-ref gate) and reports through the normal stream — open never
    // fails because warm failed.
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

  // Restore-on-launch: the dev-loop `?project=` preset wins (scripted runs),
  // else the most recent project that still resolves, else the Hello sample
  // (current untitled state — no project forced). Runs once; only roots that
  // ALL fail validation are pruned (a transient stat failure keeps entries).
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
          // Grant FIRST (design §4): under least-privilege static caps the
          // validation stat below resolves only through the runtime grant.
          // The grant fails closed on missing roots, so unreachable entries
          // land in `stale` here — one grant-first path, no special cases.
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
    // App-local outdir (V-4, mirrors Rust `out_dir_for`): clean NEVER touches
    // the project dir (RULES §8: no legacy).
    const { tempDir } = await import('@tauri-apps/api/path');
    const { appOutDir } = await import('./lib/paths');
    const dir = target.slice(0, target.lastIndexOf('/')) || '/tmp';
    const out = appOutDir(await tempDir(), dir);
    try {
      // Per-entry removal (no recursive-remove capability needed): build
      // artifacts only, never sources. Missing dir = already clean.
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

  // Main-file tie-break (D-7): the scan found >1 `\documentclass` and picked
  // the first. Choosing here writes the explicit association (same path as
  // Set as Main File), so the tie never reappears for this project.
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

  // (B) keymap listener subscribes ONCE and reads via refs. Adding
  // a new global chord = extend `lib/keymap.ts` + this listener only (never a
  // second window listener). Menu chords (Ctrl+O/W/G…) dispatch through the
  // same command registry refs as MenuBar clicks — one path, no duplicates.
  // Double-click in the editor = forward SyncTeX from the caret line
  // (complements single-click inverse on the PDF canvas).
  const menuActionRef = useRef<(id: string) => void>(() => {});
  const forwardSyncLineRef = useRef<(line: number) => void>(() => {});
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
        // Tab cycling is handled by BufferTabs when focused; global fallback:
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
      setLog('saved ' + fileName);
      emit({ scope: 'fs', kind: 'success', message: 'saved ' + fileName });
    } else {
      await saveTexToDisk(fileName, tex);
      setLog('saved ' + fileName);
      emit({ scope: 'fs', kind: 'success', message: 'saved ' + fileName });
    }
  }, [fileName, tex, buffers, largeFile, markOwnWrite]);

  useEffect(() => {
    if (!fileName.includes('/')) return;
    const t = setTimeout(() => {
      const cur = buffers.get(fileName);
      if (cur?.dirty) {
        markOwnWrite(fileName);
        saveTex(fileName, cur.value)
          .then(() => {
            setBuffers((b) => markSaved(b, fileName));
            setLog('autosaved ' + new Date().toTimeString().slice(0, 8));
          })
          .catch(() => {
            /* autosave best-effort — dirty flag stays */
          });
      }
    }, 1200);
    return () => clearTimeout(t);
  }, [tex, fileName, buffers, markOwnWrite]);

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
  // one-off file (compile-from-this-file bypasses the main association).
  // Reentrancy: a second call while `compiling` is a no-op returning false
  // (warm-on-open and double-Ctrl+R collapse into one run — never two
  // engine children). Returns true when THIS call owned the run. The gate
  // reads a REF (not state) so a warm run racing a user Ctrl+R in the same
  // tick still collapses — state would be stale for both.
  const phaseRef = useRef('idle');
  async function runCompile(target: string | null): Promise<boolean> {
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
    // DEFENSE IN DEPTH against clobbering (2026-09-14 data-loss bug): the
    // persist step below writes editor content to `target`. Two invariants
    // make that safe: (1) the visible editor must actually OWN `target`
    // (buffered, or the untitled flow); (2) `target` must be a contained
    // project file or an explicit one-off .tex — never a bare untitled name
    // resolved against a project dir. Violations abort BEFORE any write.
    const ownsTarget =
      target == null ||
      buffers.has(target) ||
      target === fileName ||
      (!target.includes('/') && !fileName.includes('/'));
    if (target != null && target.includes('/') && !ownsTarget) {
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
        // App-local outdir (V-4, mirrors Rust `out_dir_for`): the engine log
        // lives in tmp, never in the project (RULES §8: no legacy).
        const { tempDir } = await import('@tauri-apps/api/path');
        const { appOutDir } = await import('./lib/paths');
        const out = appOutDir(await tempDir(), workdir!);
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
      setLog(r.log + ' (sidecar failed — see notes/01-compile-events/STATUS.md)');
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
  // main file, AFTER the editor is populated (openRoot awaits handleSelect
  // first). NOT a second code path — the same runCompile with the same
  // stream, same phase, same clobber guard. Two honesty rules: (1) this runs
  // only when the engine CACHE is usable — no cache entry means a full cold
  // build, which is the user's explicit Ctrl+R to pay for, not open's;
  // (2) a warm FAILURE is quiet (debug line only) — open must never look
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
    await runCompile(mainAbsPath);
    // Not owned (user raced us) → their stream wins; nothing to report.
  }

  // True when the app-local outdir already holds this target's engine output
  // (pdf + log from a previous successful run): the warm compile then only
  // verifies freshness (~free when tectonic skips work) instead of paying a
  // full cold build on every open. Best-effort stat only — never throws.
  async function engineCacheUsable(targetAbsPath: string): Promise<boolean> {
    try {
      const { tempDir } = await import('@tauri-apps/api/path');
      const { appOutDir } = await import('./lib/paths');
      const dir = targetAbsPath.slice(0, targetAbsPath.lastIndexOf('/')) || '/tmp';
      const stem = targetAbsPath.slice(targetAbsPath.lastIndexOf('/') + 1).replace(/\.tex$/, '');
      const out = appOutDir(await tempDir(), dir);
      const { stat: statFile } = await import('@tauri-apps/plugin-fs');
      await statFile(`${out}/${stem}.pdf`);
      await statFile(`${out}/${stem}.log`);
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
      emit({ scope: 'preview', kind: 'warn', message: 'SyncTeX query failed (synctex_no_match)' });
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
      emit({ scope: 'preview', kind: 'warn', message: 'SyncTeX query failed (synctex_no_match)' });
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
            return enforceBufferCap(n);
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
        return enforceBufferCap(n);
      });
      setTex(content);
      setFileName(absPath);
      setPreviewFile(null);
      setLargeFile(null);
      setCurrentLine(line);
    });
  }

  // (A) Latest-closure refs for the subscribe-once keymap listener below.
  // The listener must never close over render state: assign every render and
  // call only `*.current()`. Why: typing in an UNTITLED file (`hello.tex`, no
  // `/`) updates `tex` alone — `buffers`/`fileName`/`root` never change, so a
  // dep-driven listener never resubscribes and Ctrl+R compiles the mount-time
  // buffer forever (the stale-compile bug). Placed after `compile` /
  // `handleForwardSync` declarations so the names resolve.
  compileRef.current = compile;
  forwardSyncRef.current = handleForwardSync;
  handleSelectRef.current = handleSelect;
  // Forward SyncTeX from an explicit line (editor double-click). The line is
  // given, so no caret read — but the same-page silence rule still applies.
  forwardSyncLineRef.current = (line: number) => {
    if (!pdfUrl || compilePhase === 'compiling') return;
    if (line !== currentLineRef.current) setCurrentLine(line);
    const texPath = fileName.includes('/') ? fileName : workdirHint + '/' + fileName;
    void forward_sync(pdfUrl, texPath, line).then((result) => {
      if (!result.ok || isForwardNoMatch(result.text)) {
        emit({ scope: 'preview', kind: 'warn', message: 'synctex_no_match' });
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
  // Anchor for the main-file tie-break menu (D-7): which element it opens from.
  const [mainAnchor, setMainAnchor] = useState<HTMLElement | null>(null);
  // Rename dialog for the ACTIVE file (D-5): same persist-then-select path as
  // the tree row dialog — the menu command opens it, the tree owns nothing.
  const [renameOpen, setRenameOpen] = useState(false);
  const [renameDraft, setRenameDraft] = useState('');
  // Viewport bridge is assigned inside EditorViewport via viewportRef prop.
  // Without it selectAll/expand/shrink/goToLine no-op (the Selection bug).
  const viewportRef = useRef<EditorViewportHandle | null>(null);
  // Outline: active buffer only, debounced 500ms (scale law #3 — never per keystroke).
  // Full-fidelity entries (parse DATA cap 1000); the VIEW slices to 100 per
  // filter INSIDE OutlineView (filter-first, cap-second). Sections-only rows
  // feed the Selection submenus (unchanged contract: sections navigate).
  const [outline, setOutline] = useState<OutlineEntry[]>([]);
  // Multi-pick set for Selection > Pick Sections (choose-N demo + future batch ops).
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

  // Mirror compile bus → StatusBar phase/timer (placement only; 01 owns contract).
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
      // Budget emission: keystroke-to-paint probe (target <50ms at 5MB).
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

  // ---- Command registry binding (source of truth: lib/commands.ts) ----
  // Menus, icon buttons, chords, and (later) MCP all invoke THESE actions.
  // Rename needs the SAME project-file predicate the registry gates on —
  // factored here so the menu action and buildMenus can't drift apart.
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
    // Selection submenus navigate SECTIONS (markers live in the outline view
    // filter, not in menus — menus stay jump-targets, the view stays the map).
    outlineLines: outline
      .filter((o) => o.kind === 'section')
      .map((o) => ({ line: o.line, title: o.title })),
    outlinePicks,
    canUndoDelete: trash.size > 0,
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

  // Quiet caption (D-6): the editor header names the file + its main file and
  // nothing else. Buffer counts live on the tabs, trash depth on the StatusBar
  // `↩ N`, transient outcomes in the LogStream — the caption never carries them.
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
      <StatusBar
        mainFile={relOf(mainFile)}
        mainFileTitle={mainFile}
        historyCount={trash.size}
        phase={compilePhase}
        timer={compileTimer}
        message={log}
      />
    </Box>
  );
}
