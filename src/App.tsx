import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import Box from '@mui/material/Box';
import Button from '@mui/material/Button';
import Dialog from '@mui/material/Dialog';
import DialogActions from '@mui/material/DialogActions';
import DialogContent from '@mui/material/DialogContent';
import DialogTitle from '@mui/material/DialogTitle';
import TextField from '@mui/material/TextField';
import Typography from '@mui/material/Typography';
import EditorViewport, { type EditorViewportHandle } from './components/EditorViewport';
import BufferTabs from './components/BufferTabs';
import CompileButton from './components/CompileButton';
import MenuBar from './components/MenuBar';
import Preview from './components/Preview';
import FileTree from './components/FileTree';
import LogStream from './components/LogStream';
import OutlineView from './components/OutlineView';
import ShortcutsDialog from './components/ShortcutsDialog';
import StatusBar from './components/StatusBar';
import Pane, { PaneSplitter } from './components/Pane';
import { openProject, listDir1Level, loadTex, saveTex, saveTexToDisk, createFile, renamePath, LARGE_FILE_BYTES, TreeEntry } from './lib/files';
import { getOrCreateBuffer, updateBuffer, markSaved, type BufferState } from './lib/buffers';
import { compileTex, onCompileLine, cancelCompile } from './lib/compile';
import { emitPdf, onPdf } from './lib/preview-bus';
import { forward_sync, inverse_sync } from './lib/synctex';
import { emit } from './lib/events';
import { parseLog } from './lib/parseLog';
import { parseOutline } from './lib/outline';
import { matchesCompile, matchesForwardSync, matchesMenuChord, menuChordId } from './lib/keymap';
import { buildMenus, presetOf, type CommandActions, type MenuContext } from './lib/commands';
import { listTreeDeep } from './lib/files';
import { resolveMainFileTauri, setMainFile } from './lib/mainFile.tauri';
import { FileHistory } from './lib/file-history';
import { moveToTrash, undoTrash } from './lib/trash';
import { watch, readTextFile, mkdir, stat } from '@tauri-apps/plugin-fs';
import { coalesceEvents, classifyTauriEvent, debounce } from './lib/watcher';

const HELLO = '\\documentclass{article}\n\\begin{document}\nHello Maleficium\n\\end{document}\n';

export default function App({ themeMode = 'dark', onThemeMode = () => {} }: {
  themeMode?: 'dark' | 'light';
  onThemeMode?: (m: 'dark' | 'light') => void;
}) {
  const [tex, setTex] = useState(HELLO);
  const [root, setRoot] = useState<string|null>(null);
  const [tree, setTree] = useState<TreeEntry[]>([]);
  const [fileName, setFileName] = useState('hello.tex');
  const [mainFile, setMainFileState] = useState<string | null>(null);
  const [mainSource, setMainSource] = useState('');
  const [buffers, setBuffers] = useState<Map<string, BufferState>>(new Map());
  const [trash] = useState(() => new FileHistory());
  const [trashMsg, setTrashMsg] = useState('');
  const [reloadPath, setReloadPath] = useState<string | null>(null);
  const [log, setLog] = useState('ready');
  const [largeFile, setLargeFile] = useState<string | null>(null);
  const [pdfUrl, setPdfUrl] = useState<string|null>(null);
  const [pdfStamp, setPdfStamp] = useState(0);
  const [currentLine, setCurrentLine] = useState(1);
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
  // P-10: at most 10 open buffers (LRU persist-then-evict; dirty never lost —
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
  // our own writes must not raise the reload banner (M-6). Windowed suppression.
  const ownWritesRef = useRef<Map<string, number>>(new Map());
  const markOwnWrite = useCallback((p: string) => {
    ownWritesRef.current.set(p, Date.now());
  }, []);

  useEffect(()=>onPdf(setPdfUrl),[]);

  const handleSelect = useCallback(async (path: string) => {
    // Persist current buffer before switching (dirty survives switch via map).
    if (fileName.includes('/') && path !== fileName) {
      const cur = buffers.get(fileName);
      if (cur?.dirty) {
        try { await saveTex(fileName, cur.value); markOwnWrite(fileName); setBuffers((b) => markSaved(b, fileName)); } catch { /* keep dirty */ }
      }
    }
    const selectToken = ++selectTokenRef.current;
    // Reuse preserved buffer without re-reading.
    const kept = buffers.get(path);
    if (kept) {
      if (selectToken !== selectTokenRef.current) return; // stale click lost the race
      setTex(kept.value);
      setFileName(path);
      setLargeFile(null);
      setReloadPath(null);
      setLog('switched ' + path + (kept.dirty ? ' (unsaved changes)' : ''));
      emit({ scope: 'fs', kind: 'info', message: 'switched ' + path });
      if (root && path.endsWith('.tex')) void resolveMain(root, path);
      return;
    }
    emit({scope:'fs',kind:'progress',message:'loading '+path}); setLog('loading '+path);
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
      const content=await loadTex(path);
      if (selectToken !== selectTokenRef.current) return; // stale load: drop, keep newest
      setBuffers((b) => { const n = new Map(b); getOrCreateBuffer(n, path, content); return enforceBufferCap(n); });
      setTex(content);
      setFileName(path);
      setReloadPath(null);
      setLog('loaded '+path);
      emit({scope:'fs',kind:'success',message:'loaded '+path});
      if (root && path.endsWith('.tex')) void resolveMain(root, path);
    } catch (e) {
      setLog('load failed: ' + String(e).slice(0, 120));
      emit({ scope: 'fs', kind: 'error', message: 'load failed ' + path });
    }
    }, [buffers, fileName, root]);


  useEffect(()=>{ const h=()=>{ cancelCompile().catch(()=>{}); }; window.addEventListener('beforeunload',h); return ()=>window.removeEventListener('beforeunload',h); },[]);

  const resolveMain = useCallback(async (r: string, opened: string | null) => {
    const res = await resolveMainFileTauri(r, opened);
    setMainFileState(res.mainFile);
    setMainSource(res.source + (res.candidates.length > 1 ? ` (${res.candidates.length} candidates, first wins)` : ''));
    return res.mainFile;
  }, []);

  // Honest lazy (P-06): UI tree is 1 level + expand-on-demand. The recursive
  // walk survives ONLY for main-file scan + watcher baseline (off open path).
  // open timing emission proves O(depth 1) on large projects.
  const reloadTree = useCallback(async (r: string, deep = false) => {
    const t0 = performance.now();
    const t = deep ? await listTreeDeep(r) : await listDir1Level(r);
    setTree(t);
    const dt = Math.round(performance.now() - t0);
    emit({ scope: 'fs', kind: 'info', message: `tree ${deep ? 'full' : 'root'} loaded ${t.length} rows in ${dt}ms` });
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
    watch(root, (ev) => {
      for (const c of classifyTauriEvent(ev)) pending.push(c);
      flush();
    }, { recursive: true, delayMs: 250 }).then(
      (u) => { if (!cancelled) unwatch = u; else u(); },
      () => { /* watcher unavailable (web fallback) — tree still works via manual reload */ },
    );
    return () => { cancelled = true; if (unwatch) unwatch(); };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [root]);

  async function open(){
    const r=await openProject();
    if(r){
      setRoot(r);
      await reloadTree(r, false);
      setLog('opened '+r); emit({scope:'fs',kind:'info',message:'opened '+r});
      trash.clear();
      const m = await resolveMain(r, null);
      setLog(m ? `opened ${r} (main: ${m})` : `opened ${r} (no main file found)`);
    }
    else {setLog('open cancelled'); emit({scope:'fs',kind:'warn',message:'cancelled'});}
  }

  // Tree CRUD (P-06): create/rename via plugin-fs; own-write marks suppress echoes.
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
      markOwnWrite(oldPath); markOwnWrite(full);
      setBuffers((b) => {
        const prev = b.get(oldPath);
        if (!prev) return b;
        const n = new Map(b);
        n.delete(oldPath);
        n.set(full, prev);
        return n;
      });
      if (fileName === oldPath) { setFileName(full); setReloadPath(null); }
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
      setBuffers((b) => { const n = new Map(b); n.set(reloadPath, { value: content, dirty: false, version: (n.get(reloadPath)?.version ?? 0) + 1 }); return n; });
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
        try { await saveTex(fileName, cur.value); markOwnWrite(fileName); } catch { /* keep dirty, still evict? no — stay */ return; }
      }
    }
    setBuffers((b) => { const n = new Map(b); n.delete(path); return n; });
    if (path === fileName) {
      // Fall through to nearest remaining buffer (keeps editor populated).
      const rest = [...buffers.keys()].filter((k) => k !== path);
      if (rest.length > 0) {
        const next = buffers.get(rest[rest.length - 1]);
        if (next) {
          setTex(next.value);
          setFileName(rest[rest.length - 1]);
          setLargeFile(null);
          setReloadPath(null);
        }
      }
    }
    emit({ scope: 'fs', kind: 'info', message: 'closed ' + path });
  }

  async function handleDelete(path: string) {
    if (!root) return;
    const r = await moveToTrash(trash, root, path);
    if (r.ok) {
      setTrashMsg(`deleted ${path} (undo available)`);
      emit({ scope: 'fs', kind: 'warn', message: 'trashed ' + path });
      setBuffers((b) => { const n = new Map(b); n.delete(path); return n; });
      await reloadTree(root);
    } else {
      setTrashMsg('delete failed: ' + (r.error ?? '').slice(0, 120));
    }
  }

  async function handleClean() {
    const target = mainFile ?? (fileName.includes('/') ? fileName : null);
    if (!target || !target.includes('/')) {
      emit({ scope: 'compile', kind: 'warn', message: 'Clean: nothing to clean (no project file)' });
      return;
    }
    const dir = target.slice(0, target.lastIndexOf('/')) || '/tmp';
    const out = dir + '/out';
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
        } catch { /* keep going — report count at end */ }
      }
      markOwnWrite(out);
      emit({ scope: 'compile', kind: 'success', message: `Cleaned ${out} (${n} files)` });
      if (root) await reloadTree(root, false);
    } catch (e) {
      emit({ scope: 'compile', kind: 'error', message: 'Clean failed: ' + String(e).slice(0, 120) });
    }
  }

  async function handleUndo() {
    const r = await undoTrash(trash);
    setTrashMsg(r.ok ? 'restored' : 'undo failed: ' + (r.error ?? '').slice(0, 120));
    if (r.ok) {
      emit({ scope: 'fs', kind: 'success', message: 'trash undo' });
      if (root) { await reloadTree(root); }
    }
  }

  async function handleSetMain() {
    if (!root || !fileName.includes('/')) return;
    await setMainFile(root, fileName);
    const m = await resolveMain(root, fileName);
    setLog('main file: ' + (m ?? '(none)'));
    emit({ scope: 'fs', kind: 'success', message: 'main file set: ' + (m ?? '(none)') });
  }


  // (B) keymap listener subscribes ONCE and reads via refs (§A below). Adding
  // a new global chord = extend `lib/keymap.ts` + this listener only (never a
  // second window listener). Menu chords (Ctrl+O/W/G…) dispatch through the
  // same command registry refs as MenuBar clicks — one path, no duplicates.
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

  const workdirHint = fileName.includes('/') ? fileName.slice(0,fileName.lastIndexOf('/')) : '/tmp/maleficium-untitled';
  const mainDir = mainFile ? mainFile.slice(0, mainFile.lastIndexOf('/')) : workdirHint;
  // Repo-relative for display (absolute kept in tooltips); P-09 plain language.
  const relOf = (abs: string | null): string | null => {
    if (!abs) return null;
    if (root && abs.startsWith(root + '/')) return abs.slice(root.length + 1);
    return abs;
  };

  const save = useCallback(async ()=>{
    if (largeFile) {
      setLog('save blocked: large placeholder file is not loaded');
      emit({ scope: 'fs', kind: 'warn', message: 'save blocked for large placeholder ' + largeFile });
      return;
    }
    if(fileName.includes('/')){
      const cur = buffers.get(fileName);
      await saveTex(fileName, cur?.value ?? tex); markOwnWrite(fileName);
      setBuffers((b) => markSaved(b, fileName));
      setLog('saved '+fileName);
      emit({scope:'fs',kind:'success',message:'saved '+fileName});
    } else {
      await saveTexToDisk(fileName,tex);
      setLog('saved '+fileName);
      emit({scope:'fs',kind:'success',message:'saved '+fileName});
    }
  }, [fileName, tex, buffers, largeFile]);

  useEffect(() => {
    if (!fileName.includes('/')) return;
    const t = setTimeout(() => {
      const cur = buffers.get(fileName);
      if (cur?.dirty) {
        markOwnWrite(fileName); saveTex(fileName, cur.value).then(() => {
          setBuffers((b) => markSaved(b, fileName));
          setLog('autosaved ' + new Date().toTimeString().slice(0, 8));
        }).catch(() => {});
      }
    }, 1200);
    return () => clearTimeout(t);
  }, [tex, fileName, buffers]);

  // Publish engine-log problems as first-class stream events (click-to-jump).
  const publishProblems = (text: string, base: string, wsRoot: string) => {
    try {
      const entries = parseLog(text, wsRoot, base).slice(0, 100);
      for (const l of entries) {
        emit({ scope: 'compile', kind: l.clickable ? 'error' : 'warn', message: `${l.file}:${l.line} ${l.msg}`, data: { file: l.file, line: l.line, clickable: l.clickable } });
      }
    } catch { /* parse never blocks the stream */ }
  };

  async function compile(){
    const target = mainFile ?? (fileName.includes('/') ? fileName : null);
    if (largeFile) {
      emit({ scope: 'compile', kind: 'error', message: 'compile blocked: large placeholder file selected — open a .tex file' });
      setCompilePhase('failure');
      setLog('compile blocked: selected file is a large-file placeholder');
      return;
    }
    emit({scope:'compile',kind:'progress',message:'compiling '+(target ?? fileName)});
    setCompilePhase('compiling'); setCompileStart(Date.now()); setCompileTimer(0);
    setLog('compiling...');
    // Write-then-compile: the engine reads from disk, so persist first.
    let workdir: string;
    let unlisten: ()=>void = ()=>{};
    try { unlisten = await onCompileLine((line)=>emit({scope:'compile',kind:'progress',message:String(line).slice(0,300)})); } catch {}
    const t0 = Date.now(); const hb = setInterval(()=>emit({scope:'compile',kind:'progress',message:`still compiling ${target ?? fileName} (${Math.floor((Date.now()-t0)/1000)}s)`}), 5000);
    let maxGap = 0; let lastT = performance.now(); let probing = true;
    const tickProbe = () => { if (!probing) return; const now = performance.now(); maxGap = Math.max(maxGap, now - lastT); lastT = now; requestAnimationFrame(tickProbe); };
    requestAnimationFrame(tickProbe);
    try {
      if (target && target.includes('/')) {
        // Persist ALL dirty buffers so \input parts compile from disk.
        for (const [p, buf] of buffers) {
          if (buf.dirty) {
            try { await saveTex(p, buf.value); markOwnWrite(p); } catch {}
          }
        }
        setBuffers((b) => { let n = b; for (const [p, buf] of b) if (buf.dirty) n = markSaved(n, p); return n; });
        // Also persist the visible editor if it was never buffered (untitled flow).
        if (!buffers.has(target)) { await saveTex(target, tex); markOwnWrite(target); }
        workdir = target.slice(0, target.lastIndexOf('/')) || '/tmp';
      } else {
        workdir = '/tmp/maleficium-untitled';
        await mkdir(workdir, { recursive: true });
        const t2 = workdir + '/' + fileName;
        await saveTex(t2, tex); markOwnWrite(t2);
        setMainFileState(t2);
      }
    } catch(e){
      setCompilePhase('failure'); setCompileStart(null);
      probing = false;
      emit({scope:'compile',kind:'error',message:'save failed: '+String(e).slice(0,200)});
      clearInterval(hb); try{unlisten();}catch{}
      return;
    }
    const activeTarget = mainFile ?? (fileName.includes('/') ? fileName : workdir! + '/' + fileName);
    const main = activeTarget.slice(activeTarget.lastIndexOf('/') + 1);
    const r = await compileTex(activeTarget, workdir!);
    setLog(r.log);
    const readEngineLog = async (): Promise<string | null> => {
      try {
        return await readTextFile(`${workdir!}/out/${main.replace(/\.tex$/, '.log')}`);
      } catch {
        return null;
      }
    };
    if (r.ok && r.pdfPath) {
      setCompilePhase('success'); setCompileStart(null);
      setLogCollapsed(false);
      emit({scope:'compile',kind:'success',message:'compiled '+String(r.pdfPath)});
      emitPdf(r.pdfPath);
      setPdfStamp(s=>s+1);
      emit({scope:'preview',kind:'success',message:'preview '+String(r.pdfPath)});
    } else if (!r.ok && r.log.includes('spawn')) {
      setCompilePhase('failure'); setCompileStart(null);
      setLogCollapsed(false);
      setLog(r.log + ' (sidecar failed — see notes/01-compile-events/STATUS.md)');
      emit({scope:'compile',kind:'error',message:String(r.log).slice(0,300)});
      const c = await readEngineLog();
      publishProblems(c ?? r.log, mainDir, root || workdirHint);
    } else if (!r.ok) {
      setCompilePhase('failure'); setCompileStart(null);
      setLogCollapsed(false);
      emit({scope:'compile',kind:'error',message:String(r.log).slice(0,300)});
      const c = await readEngineLog();
      publishProblems(c ?? r.log, mainDir, root || workdirHint);
    }
    clearInterval(hb); try { unlisten(); } catch {}
    probing = false; emit({scope:'compile',kind:'info',message:`main-thread max frame ${Math.round(maxGap)}ms during compile`});
  }

  async function handleForwardSync(){
    if (!pdfUrl) return;
    if (compilePhase === 'compiling') {
      emit({ scope: 'preview', kind: 'warn', message: 'SyncTeX unavailable while compiling (synctex_no_match)' });
      return;
    }
    const base = fileName.replace(/\.tex$/, '.pdf');
    const result = await forward_sync(pdfUrl, base, currentLine);
    emit({ scope: 'preview', kind: 'info', message: `forward SyncTeX → ${result.text.slice(0, 120)}` });
    if (result.text.includes('no_match') || result.text === '{}') {
      emit({ scope: 'preview', kind: 'warn', message: 'synctex_no_match' });
    }
  }

  async function handleInverseSync(page: number, x: number, y: number){
    if (!pdfUrl) return;
    if (compilePhase === 'compiling') {
      emit({ scope: 'preview', kind: 'warn', message: 'synctex_no_match: disabled during compile' });
      return;
    }
    const result = await inverse_sync(pdfUrl, page, x, y);
    // Real `synctex edit` shape:
    //   Input:/abs/path/hello.tex\nLine:7\n... (stub: {"line":N,"page":P}).
    let line: number | null = null;
    let hitFile: string | null = null;
    try {
      const j = JSON.parse(result.text) as { line?: unknown };
      if (typeof j.line === 'number') line = j.line;
    } catch { /* raw synctex output — parse below */ }
    if (line == null) {
      const lm = result.text.match(/^Line:\s*(\d+)\s*$/m);
      if (lm) line = parseInt(lm[1], 10);
      const im = result.text.match(/^Input:\s*(.+?)\s*$/m);
      if (im) hitFile = im[1].trim();
    }
    // Legacy fallback: File:Line scan (never matches real synctex output,
    // kept for forward-compat with stub shapes).
    if (line == null) {
      const m = result.text.match(/(?:^|\s)([\w\-./]+\.tex):(\d+)/m);
      if (m) line = parseInt(m[2], 10);
    }
    if (line != null) {
      // Jump the owning file when SyncTeX names one (multi-file projects);
      // otherwise reveal the line in the current buffer.
      if (hitFile && hitFile !== fileName) {
        try {
          const content = await loadTex(hitFile);
          setBuffers((b) => { const n = new Map(b); getOrCreateBuffer(n, hitFile as string, content); return enforceBufferCap(n); });
          setTex(content);
          setFileName(hitFile as string);
          setLargeFile(null);
          setReloadPath(null);
        } catch {
          /* unreadable hit file — still reveal the line number below */
        }
      }
      setCurrentLine(line);
      setSynctexFlash((f) => f + 1);
      emit({ scope: 'preview', kind: 'success', message: `synctex inverse → ${hitFile ?? fileName}:${line}` });
    } else {
      emit({ scope: 'preview', kind: 'warn', message: 'SyncTeX: no match at this position (synctex_no_match)' });
    }
  }

  function handleJump(absPath: string, line: number){
    loadTex(absPath).then(content => {
      setBuffers((b) => { const n = new Map(b); getOrCreateBuffer(n, absPath, content); return enforceBufferCap(n); });
      setTex(content);
      setFileName(absPath);
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

  const dirtyCount = useMemo(() => [...buffers.values()].filter((b) => b.dirty).length, [buffers]);

  // ---- Shell state: view is explicit booleans (View menu presets own them) ----
  const [treeVisible, setTreeVisible] = useState(true);
  const [editorVisible, setEditorVisible] = useState(true);
  const [previewOpen, setPreviewOpen] = useState(true);
  const [layout, setLayout] = useState(() => {
    try {
      const raw = localStorage.getItem('maleficium.layout');
      if (raw) {
        const j = JSON.parse(raw) as Partial<{ editorRatio: number; previewRatio: number; logHeight: number }>;
        return {
          editorRatio: typeof j.editorRatio === 'number' ? Math.min(0.8, Math.max(0.2, j.editorRatio)) : 0.6,
          previewRatio: typeof j.previewRatio === 'number' ? Math.min(0.8, Math.max(0.2, j.previewRatio)) : 0.4,
          logHeight: typeof j.logHeight === 'number' ? Math.max(80, Math.min(600, j.logHeight)) : 160,
        };
      }
    } catch { /* corrupted prefs — defaults win */ }
    return { editorRatio: 0.6, previewRatio: 0.4, logHeight: 160 };
  });
  useEffect(() => {
    try { localStorage.setItem('maleficium.layout', JSON.stringify(layout)); } catch { /* private mode — layout just won't persist */ }
  }, [layout]);
  const [logCollapsed, setLogCollapsed] = useState(false);
  const [pageNumber, setPageNumber] = useState(1);
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
  // Viewport bridge is assigned inside EditorViewport via viewportRef prop.
  // Without it selectAll/expand/shrink/goToLine no-op (the Selection bug).
  const viewportRef = useRef<EditorViewportHandle | null>(null);
  // Outline: active buffer only, debounced 500ms (scale law #3 — never per keystroke).
  const [outline, setOutline] = useState<{ title: string; line: number; level: number }[]>([]);
  // Multi-pick set for Selection > Pick Sections (choose-N demo + future batch ops).
  const [outlinePicks, setOutlinePicks] = useState<number[]>([]);
  useEffect(() => {
    const t = setTimeout(() => {
      try {
        const t0 = performance.now();
        const entries = parseOutline(tex).slice(0, 100);
        setOutline(entries);
        if (tex.length > 1_000_000) {
          emit({ scope: 'app', kind: 'info', message: `outline parsed ${entries.length} entries in ${Math.round(performance.now() - t0)}ms` });
        }
      } catch { /* outline never blocks editing */ }
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

  const handleTexChange = useCallback((v: string) => {
    const t0 = performance.now();
    setTex(v);
    if (fileName.includes('/')) {
      setBuffers((b) => updateBuffer(b, fileName, v));
    }
    // Budget emission: keystroke-to-paint probe (target <50ms at 5MB).
    requestAnimationFrame(() => {
      const dt = Math.round(performance.now() - t0);
      if (v.length > 1_000_000) {
        emit({ scope: 'app', kind: 'info', message: `editor render ${(v.length / 1_048_576).toFixed(1)}MB file in ${dt}ms` });
      }
    });
  }, [fileName]);

  // ---- Command registry binding (source of truth: lib/commands.ts) ----
  // Menus, icon buttons, chords, and (later) MCP all invoke THESE actions.
  const compileTarget = mainFile ?? (fileName.includes('/') ? fileName : null);
  const workingLabel = (() => {
    const t = compileTarget ?? (largeFile ?? fileName);
    const base = t.slice(t.lastIndexOf('/') + 1) || t;
    return relOf(t) === t ? base : `${relOf(t)}`;
  })();
  const menuCtx: MenuContext = {
    hasProject: root != null,
    isProjectFile: root != null && fileName.includes('/') && fileName.startsWith(root + '/'),
    dirty: !!buffers.get(fileName)?.dirty,
    compiling: compilePhase === 'compiling',
    pdfOpen: pdfUrl != null,
    editorReady: viewportRef.current != null && largeFile == null,
    view: { tree: treeVisible, editor: editorVisible, preview: previewOpen },
    preset: presetOf({ tree: treeVisible, editor: editorVisible, preview: previewOpen }),
    logCollapsed,
    outlineVisible,
    outlineLines: outline.map((o) => ({ line: o.line, title: o.title })),
    outlinePicks,
    canUndoDelete: trash.size > 0,
    reloadPending: reloadPath != null,
    theme: themeMode,
  };
  const menuActions: CommandActions = {
    openProject: () => { void open(); },
    newFile: () => {
      if (root) void handleCreate(root, 'untitled.tex');
      else emit({ scope: 'fs', kind: 'warn', message: 'New File needs an open project' });
    },
    closeFile: () => { void handleCloseBuffer(fileName); },
    save: () => { void save(); },
    setMainFile: () => { void handleSetMain(); },
    reloadFromDisk: () => { void handleReload(); },
    keepMine: () => setReloadPath(null),
    clean: () => { void handleClean(); },
    undoDelete: () => { void handleUndo(); },
    renameActive: () => {
      // Rename flows through the tree row dialog; menu focuses the tree instead.
      emit({ scope: 'fs', kind: 'info', message: 'Rename: right-click the file in the tree' });
    },
    deleteActive: () => { void handleDelete(fileName); },
    selectAll: () => viewportRef.current?.selectAll(),
    expandSelection: () => viewportRef.current?.expandSelection(),
    shrinkSelection: () => viewportRef.current?.shrinkSelection(),
    goToLine: () => { setGoToDraft(String(currentLine)); setGoToOpen(true); },
    pickOutlineSection: (line: number) => setCurrentLine(line),
    toggleOutlinePick: (line: number) =>
      setOutlinePicks((prev) => (prev.includes(line) ? prev.filter((l) => l !== line) : [...prev, line])),
    setPreset: (preset) => {
      if (preset === 'both') { setTreeVisible(true); setEditorVisible(true); setPreviewOpen(true); setPreviewCollapsed(false); }
      else if (preset === 'editor') { setTreeVisible(false); setEditorVisible(true); setPreviewOpen(false); }
      else { setTreeVisible(false); setEditorVisible(false); setPreviewOpen(true); setPreviewCollapsed(false); }
    },
    toggleTree: () => setTreeVisible((v) => !v),
    togglePreview: () => { setPreviewOpen((v) => !v); setPreviewCollapsed(false); },
    toggleLog: () => setLogCollapsed((c) => !c),
    toggleOutline: () => setOutlineVisible((v) => !v),
    setTheme: (m) => onThemeMode(m),
    compile: () => { void compileRef.current(); },
    cancelCompile: () => { void cancelCompile().catch((e) => emit({ scope: 'compile', kind: 'error', message: 'cancel failed: ' + String(e).slice(0, 120) })); },
    forwardSync: () => { void forwardSyncRef.current(); },
    inverseHint: () => emit({ scope: 'preview', kind: 'info', message: 'Inverse SyncTeX: click anywhere on the PDF' }),
    showShortcuts: () => setShortcutsOpen(true),
    showAbout: () => setAboutOpen(true),
  };
  const menuSections = buildMenus(menuCtx, menuActions);
  menuActionRef.current = (id: string) => {
    for (const sec of menuSections) {
      const cmd = sec.commands.find((c) => c.id === id);
      if (cmd && cmd.enabled && cmd.visible !== false) { void cmd.run?.(); return; }
    }
  };

  const editorPane = (
    <Box sx={{ p: 2, display: 'flex', flexDirection: 'column', height: '100%', overflow: 'hidden' }}>
      <Typography variant="caption" sx={{ display: 'block', mb: 1 }} title={fileName}>
        {relOf(fileName) ?? fileName}{buffers.get(fileName)?.dirty ? ' ●' : ''} · main: {relOf(mainFile) ?? '(none)'} {mainSource ? `(${mainSource})` : ''} · {buffers.size} open · {dirtyCount} unsaved{trashMsg ? ` · ${trashMsg}` : ''}
      </Typography>
      <BufferTabs buffers={buffers} active={fileName} onSelect={(p) => { void handleSelect(p); }} onClose={(p) => { void handleCloseBuffer(p); }} />
      {reloadPath ? (
        <Box sx={{ display: 'flex', gap: 1, mb: 1, alignItems: 'center' }}>
          <Typography variant="body2">Changed on disk: {reloadPath}</Typography>
          <Button size="small" variant="outlined" onClick={handleReload}>Reload</Button>
          <Button size="small" onClick={() => setReloadPath(null)}>Keep mine</Button>
        </Box>
      ) : null}
      {largeFile ? (
        <Typography variant="body2" sx={{ mt: 1 }}>Large file — not loaded into the editor ({largeFile}). Open externally to edit.</Typography>
      ) : (
        <Box sx={{ flex: 1, overflow: 'auto' }}>
          <EditorViewport value={tex} onChange={handleTexChange} onSave={save} line={currentLine} flashKey={synctexFlash} viewportRef={viewportRef} />
        </Box>
      )}
      <Typography variant="caption" sx={{ display: 'block', mt: 1 }}>{log}</Typography>
    </Box>
  );

  const previewPane = (
    <Box sx={{ p: 2, display: 'flex', flexDirection: 'column', height: '100%', overflow: 'hidden' }}>
      <Preview
        pdfUrl={pdfUrl}
        stamp={pdfStamp}
        pageNumber={pageNumber}
        onPage={setPageNumber}
        onSync={handleForwardSync}
        onInverse={(page, x, y) => { void handleInverseSync(page, x, y); }}
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
          <Box sx={{ width: 260, flexShrink: 0, overflow: 'auto', borderRight: 1, borderColor: 'divider', p: 1, display: 'flex', flexDirection: 'column' }}>
            {root ? (
              <>
                <Box sx={{ flexShrink: 0 }}>
                  <FileTree tree={tree} selected={fileName} onSelect={handleSelect} onDelete={handleDelete} onCreate={handleCreate} onRename={handleRename} onExpandDir={listDir1Level} rootDir={root} mainFile={mainFile} lazy maxDepth={2} filterHidden />
                </Box>
                {outlineVisible ? (
                  <OutlineView
                    entries={outline}
                    totalShown={outline.length >= 100 ? '100+' : String(outline.length)}
                    onJump={(line) => setCurrentLine(line)}
                  />
                ) : null}
              </>
            ) : (
              <Typography variant="body2" color="text.secondary">Open a project to browse files.</Typography>
            )}
          </Box>
        )}
        {editorVisible ? (
          <Pane label="editor" ratio={layout.editorRatio} onRatio={(r) => setLayout((l) => ({ ...l, editorRatio: r, previewRatio: 1 - r }))}>
            {editorPane}
          </Pane>
        ) : null}
        {editorVisible && previewVisible ? (
          <PaneSplitter
            label="Resize editor and preview"
            onDrag={(dx) => setLayout((l) => {
              const w = window.innerWidth || 1000;
              const r = Math.min(0.8, Math.max(0.2, l.editorRatio + dx / w));
              return { ...l, editorRatio: r, previewRatio: 1 - r };
            })}
            onKeyResize={(dir) => setLayout((l) => {
              const r = Math.min(0.8, Math.max(0.2, l.editorRatio + dir * 0.05));
              return { ...l, editorRatio: r, previewRatio: 1 - r };
            })}
          />
        ) : null}
        {!previewVisible ? (
          <Box sx={{ width: 48, flexShrink: 0, display: 'flex', alignItems: 'flex-start', justifyContent: 'center', pt: 1 }}>
            <Button
              size="small"
              aria-label="Show preview"
              onClick={() => { setPreviewOpen(true); setPreviewCollapsed(false); }}
            >
              show
            </Button>
          </Box>
        ) : (
          <Pane label="preview" ratio={layout.previewRatio} onRatio={(r) => setLayout((l) => ({ ...l, previewRatio: r, editorRatio: 1 - r }))}>
            <Box sx={{ display: 'flex', justifyContent: 'flex-end' }}>
              <Button size="small" aria-label="Hide preview" onClick={() => setPreviewCollapsed(true)}>hide</Button>
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
      <Dialog open={goToOpen} onClose={() => setGoToOpen(false)} maxWidth="xs">
        <DialogTitle>Go to Line</DialogTitle>
        <DialogContent>
          <TextField
            autoFocus fullWidth size="small" aria-label="Line number"
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
          <Typography variant="body2">Maleficium — desktop-native LaTeX editor (Tauri 2 + React + Tectonic sidecar).</Typography>
          <Typography variant="caption" color="text.secondary" sx={{ display: 'block', mt: 1 }}>Version 0.1.0 · offline-first · Linux-first.</Typography>
        </DialogContent>
        <DialogActions>
          <Button variant="contained" onClick={() => setAboutOpen(false)}>Close</Button>
        </DialogActions>
      </Dialog>
      <StatusBar
        mainFile={relOf(mainFile)}
        mainFileTitle={mainFile}
        phase={compilePhase}
        timer={compileTimer}
        message={log}
      />
    </Box>
  );
}
