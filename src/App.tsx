import { useCallback, useEffect, useMemo, useState } from 'react';
import Box from '@mui/material/Box';
import Button from '@mui/material/Button';
import Slide from '@mui/material/Slide';
import Drawer from '@mui/material/Drawer';
import Typography from '@mui/material/Typography';
import EditorViewport from './components/EditorViewport';
import ActivityBar, { type ActivityMode } from './components/ActivityBar';
import EditorToolbar from './components/EditorToolbar';
import Preview from './components/Preview';
import FileTree from './components/FileTree';
import BottomPanel, { type BottomTab } from './components/BottomPanel';
import StatusBar from './components/StatusBar';
import Pane, { PaneSplitter } from './components/Pane';
import { openProject, listTree, listDir1Level, loadTex, saveTex, saveTexToDisk, LARGE_FILE_BYTES, TreeEntry } from './lib/files';
import { getOrCreateBuffer, updateBuffer, markSaved, type BufferState } from './lib/buffers';
import { compileTex, onCompileLine, cancelCompile } from './lib/compile';
import { emitPdf, onPdf } from './lib/preview-bus';
import { gitStatus, gitStatusState, gitShowHead, type GitBadge } from './lib/git';
import { forward_sync } from './lib/synctex';
import { emit } from './lib/events';
import { resolveMainFileTauri, setMainFile } from './lib/mainFile.tauri';
import { FileHistory } from './lib/file-history';
import { moveToTrash, undoTrash } from './lib/trash';
import { watch, readTextFile, mkdir, stat } from '@tauri-apps/plugin-fs';
import { coalesceEvents, classifyTauriEvent, debounce } from './lib/watcher';

const HELLO = '\\documentclass{article}\n\\begin{document}\nHello Maleficium\n\\end{document}\n';

export default function App() {
  const [tex, setTex] = useState(HELLO);
  const [root, setRoot] = useState<string|null>(null);
  const [tree, setTree] = useState<TreeEntry[]>([]);
  const [fileName, setFileName] = useState('hello.tex');
  const [mainFile, setMainFileState] = useState<string | null>(null);
  const [mainSource, setMainSource] = useState('');
  const [buffers, setBuffers] = useState<Map<string, BufferState>>(new Map());
  const [trash] = useState(() => new FileHistory());
  const [trashMsg, setTrashMsg] = useState('');
  const [gitBadges, setGitBadges] = useState<Record<string, GitBadge>>({});
  const [gitBranch, setGitBranch] = useState<string | null>(null);
  const [reloadPath, setReloadPath] = useState<string | null>(null);
  const [log, setLog] = useState('ready');
  const [logText, setLogText] = useState('');
  const [largeFile, setLargeFile] = useState<string | null>(null);
  const [gitText, setGitText] = useState('');
  const [forwardMsg, setForwardMsg] = useState('');
  const [pdfUrl, setPdfUrl] = useState<string|null>(null);
  const [pdfStamp, setPdfStamp] = useState(0);
  const [currentLine, setCurrentLine] = useState(1);

  useEffect(()=>onPdf(setPdfUrl),[]);

  useEffect(()=>{ const h=()=>{ cancelCompile().catch(()=>{}); }; window.addEventListener('beforeunload',h); return ()=>window.removeEventListener('beforeunload',h); },[]);

  const refreshGit = useCallback(async (r: string) => {
    const st = await gitStatusState(r);
    if (st.ok) {
      setGitBadges(st.badges);
      setGitBranch(st.branch);
    } else {
      setGitBadges({});
      setGitBranch(null);
    }
  }, []);

  const resolveMain = useCallback(async (r: string, opened: string | null) => {
    const res = await resolveMainFileTauri(r, opened);
    setMainFileState(res.mainFile);
    setMainSource(res.source + (res.candidates.length > 1 ? ` (${res.candidates.length} candidates, first wins)` : ''));
    return res.mainFile;
  }, []);

  const reloadTree = useCallback(async (r: string) => {
    const t = await listTree(r);
    setTree(t);
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
      void refreshGit(root);
      for (const ev of batch) {
        if (ev.path === fileName && ev.kind === 'modify') setReloadPath(ev.path);
        if (ev.path === fileName && ev.kind === 'delete') {
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
      const t=await listTree(r);setTree(t);
      setLog('opened '+r); emit({scope:'fs',kind:'info',message:'opened '+r});
      trash.clear();
      const m = await resolveMain(r, null);
      await refreshGit(r);
      setLog(m ? `opened ${r} (main: ${m})` : `opened ${r} (no main file found)`);
    }
    else {setLog('open cancelled'); emit({scope:'fs',kind:'warn',message:'cancelled'});}
  }

  async function handleSelect(path:string){
    // Persist current buffer before switching (dirty survives switch via map).
    if (fileName.includes('/')) {
      const cur = buffers.get(fileName);
      if (cur?.dirty) {
        try { await saveTex(fileName, cur.value); setBuffers((b) => markSaved(b, fileName)); } catch { /* keep dirty */ }
      }
    }
    // Reuse preserved buffer without re-reading.
    const kept = buffers.get(path);
    if (kept) {
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
        setLargeFile(path);
        setFileName(path);
        setLog(`large file (${Math.round(size / 1024)}KB) — preview only`);
        emit({ scope: 'fs', kind: 'warn', message: `large file placeholder ${path} (${size}B)` });
        return;
      }
      setLargeFile(null);
      const content=await loadTex(path);
      setBuffers((b) => { const n = new Map(b); getOrCreateBuffer(n, path, content); return n; });
      setTex(content);
      setFileName(path);
      setReloadPath(null);
      setLog('loaded '+path);
      emit({scope:'fs',kind:'success',message:'loaded '+path});
      if (root && path.endsWith('.tex')) void resolveMain(root, path);
      if (path.endsWith('.tex')) {
        const logName = path.replace(/\.tex$/, '.log');
        try {
          const logContent = await readTextFile(logName);
          setLogText(logContent);
        } catch {}
      }
    } catch (e) {
      setLog('load failed: ' + String(e).slice(0, 120));
      emit({ scope: 'fs', kind: 'error', message: 'load failed ' + path });
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

  async function handleDelete(path: string) {
    if (!root) return;
    const r = await moveToTrash(trash, root, path);
    if (r.ok) {
      setTrashMsg(`deleted ${path} (undo available)`);
      emit({ scope: 'fs', kind: 'warn', message: 'trashed ' + path });
      setBuffers((b) => { const n = new Map(b); n.delete(path); return n; });
      await reloadTree(root);
      await refreshGit(root);
    } else {
      setTrashMsg('delete failed: ' + (r.error ?? '').slice(0, 120));
    }
  }

  async function handleUndo() {
    const r = await undoTrash(trash);
    setTrashMsg(r.ok ? 'restored' : 'undo failed: ' + (r.error ?? '').slice(0, 120));
    if (r.ok) {
      emit({ scope: 'fs', kind: 'success', message: 'trash undo' });
      if (root) { await reloadTree(root); await refreshGit(root); }
    }
  }

  async function handleSetMain() {
    if (!root || !fileName.includes('/')) return;
    await setMainFile(root, fileName);
    const m = await resolveMain(root, fileName);
    setLog('main file: ' + (m ?? '(none)'));
    emit({ scope: 'fs', kind: 'success', message: 'main file set: ' + (m ?? '(none)') });
  }

  const handleGitStatusClick = () => { void handleGitStatus().then(() => setBottomTab('log')); };
  void handleGitStatusClick;

  const save = useCallback(async ()=>{
    if(fileName.includes('/')){
      const cur = buffers.get(fileName);
      await saveTex(fileName, cur?.value ?? tex);
      setBuffers((b) => markSaved(b, fileName));
      setLog('saved '+fileName);
      emit({scope:'fs',kind:'success',message:'saved '+fileName});
    } else {
      await saveTexToDisk(fileName,tex);
      setLog('saved '+fileName);
      emit({scope:'fs',kind:'success',message:'saved '+fileName});
    }
  }, [fileName, tex, buffers]);

  useEffect(() => {
    if (!fileName.includes('/')) return;
    const t = setTimeout(() => {
      const cur = buffers.get(fileName);
      if (cur?.dirty) {
        saveTex(fileName, cur.value).then(() => {
          setBuffers((b) => markSaved(b, fileName));
          setLog('autosaved ' + new Date().toTimeString().slice(0, 8));
        }).catch(() => {});
      }
    }, 1200);
    return () => clearTimeout(t);
  }, [tex, fileName, buffers]);

  async function compile(){
    const target = mainFile ?? (fileName.includes('/') ? fileName : null);
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
            try { await saveTex(p, buf.value); } catch {}
          }
        }
        setBuffers((b) => { let n = b; for (const [p, buf] of b) if (buf.dirty) n = markSaved(n, p); return n; });
        // Also persist the visible editor if it was never buffered (untitled flow).
        if (!buffers.has(target)) await saveTex(target, tex);
        workdir = target.slice(0, target.lastIndexOf('/')) || '/tmp';
      } else {
        workdir = '/tmp/maleficium-untitled';
        await mkdir(workdir, { recursive: true });
        const t2 = workdir + '/' + fileName;
        await saveTex(t2, tex);
        setMainFileState(t2);
      }
    } catch(e){
      emit({scope:'compile',kind:'error',message:'save failed: '+String(e).slice(0,200)});
      clearInterval(hb); try{unlisten();}catch{}
      return;
    }
    const activeTarget = mainFile ?? (fileName.includes('/') ? fileName : workdir! + '/' + fileName);
    const main = activeTarget.slice(activeTarget.lastIndexOf('/') + 1);
    const r = await compileTex(activeTarget, workdir!);
    setLog(r.log);
    if (r.ok && r.pdfPath) {
      setCompilePhase('success'); setCompileStart(null);
      setBottomTab('log');
      emit({scope:'compile',kind:'success',message:'compiled '+String(r.pdfPath)});
      emitPdf(r.pdfPath);
      setPdfStamp(s=>s+1);
      emit({scope:'preview',kind:'success',message:'preview '+String(r.pdfPath)});
      try {
        const logContent = await readTextFile(`${workdir!}/out/${main.replace(/\.tex$/, '.log')}`);
        setLogText(logContent);
      } catch { setLogText(''); }
    } else if (!r.ok && r.log.includes('spawn')) {
      setCompilePhase('failure'); setCompileStart(null);
      setBottomTab('problems');
      setLog(r.log + ' (sidecar failed — see notes/01-compile-events/STATUS.md)');
      emit({scope:'compile',kind:'error',message:String(r.log).slice(0,300)});
      try { const c = await readTextFile(`${workdir!}/out/${main.replace(/\.tex$/, '.log')}`); setLogText(c); } catch { setLogText(r.log); }
    } else if (!r.ok) {
      setCompilePhase('failure'); setCompileStart(null);
      setBottomTab('problems');
      emit({scope:'compile',kind:'error',message:String(r.log).slice(0,300)});
      try { const c = await readTextFile(`${workdir!}/out/${main.replace(/\.tex$/, '.log')}`); setLogText(c); } catch { setLogText(r.log); }
    }
    clearInterval(hb); try { unlisten(); } catch {}
    probing = false; emit({scope:'compile',kind:'info',message:`main-thread max frame ${Math.round(maxGap)}ms during compile`});
  }

  async function handleForwardSync(){
    if (!pdfUrl) return;
    const base = fileName.replace(/\.tex$/, '.pdf');
    const result = await forward_sync(pdfUrl, base, currentLine);
    setForwardMsg(result.text);
  }

  async function handleGitStatus(){
    if (!root) return;
    const result = await gitStatus(root);
    setGitText(result.text);
    await refreshGit(root);
  }

  async function handleGitShowHead() {
    if (!root || !fileName.includes('/')) return;
    const rel = fileName.startsWith(root + '/') ? fileName.slice(root.length + 1) : fileName;
    const r = await gitShowHead(root, rel);
    setGitText(r.ok ? r.text : '(no HEAD version: ' + r.text.slice(0, 80) + ')');
  }

  function handleJump(absPath: string, line: number){
    loadTex(absPath).then(content => {
      setBuffers((b) => { const n = new Map(b); getOrCreateBuffer(n, absPath, content); return n; });
      setTex(content);
      setFileName(absPath);
      setLargeFile(null);
      setCurrentLine(line);
    });
  }

  const workdirHint = fileName.includes('/') ? fileName.slice(0,fileName.lastIndexOf('/')) : '/tmp/maleficium-untitled';
  const mainDir = mainFile ? mainFile.slice(0, mainFile.lastIndexOf('/')) : workdirHint;
  const dirtyCount = useMemo(() => [...buffers.values()].filter((b) => b.dirty).length, [buffers]);
  const gitBadgeMap = useMemo(() => {
    const m = new Map<string, GitBadge>();
    if (root) {
      for (const [rel, badge] of Object.entries(gitBadges)) {
        m.set(rel.includes('/') || !root ? (rel.startsWith('/') ? rel : root + '/' + rel) : root + '/' + rel, badge);
      }
    }
    return m;
  }, [gitBadges, root]);

  // ---- Pitch 1 shell state (scalar layout ratios; D.4 density/theme deferred) ----
  const [mode, setMode] = useState<ActivityMode>('file');
  const [layout, setLayout] = useState({ editorRatio: 0.6, previewRatio: 0.4, bottomHeight: 200 });
  const [bottomTab, setBottomTab] = useState<BottomTab>('problems');
  const [pageNumber, setPageNumber] = useState(1);
  const [compilePhase, setCompilePhase] = useState('idle');
  const [compileTimer, setCompileTimer] = useState(0);
  const [compileStart, setCompileStart] = useState<number | null>(null);
  const [fileTreeOpen, setFileTreeOpen] = useState(false);
  const [previewCollapsed, setPreviewCollapsed] = useState(false);

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
  void gitText;

  const editorPane = (
    <Box sx={{ p: 2, display: 'flex', flexDirection: 'column', height: '100%', overflow: 'hidden' }}>
      <Slide direction="down" in={true} mountOnEnter={false} unmountOnExit={false}>
        <Box>
          <EditorToolbar fileName={fileName} dirty={!!buffers.get(fileName)?.dirty} onOpen={open} onSave={save} onCompile={() => { setBottomTab('log'); void compile(); }} />
        </Box>
      </Slide>
      <Typography variant="caption" sx={{ display: 'block', mb: 1 }}>
        main: {mainFile ?? '(none)'} {mainSource ? `(${mainSource})` : ''} · {buffers.size} open · {dirtyCount} unsaved{gitBranch ? ` · ${gitBranch}` : ''}{trashMsg ? ` · ${trashMsg}` : ''}
      </Typography>
      <Box sx={{ display: 'flex', gap: 1, mb: 1 }}>
        <Button variant="outlined" size="small" onClick={handleSetMain} disabled={!root || !fileName.includes('/')}>Set as main</Button>
        <Button variant="outlined" size="small" onClick={handleUndo}>Undo delete</Button>
        <Button variant="outlined" size="small" onClick={handleGitShowHead}>HEAD diff</Button>
      </Box>
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
          <EditorViewport value={tex} onChange={handleTexChange} onSave={save} line={currentLine} />
        </Box>
      )}
      <Typography variant="caption" sx={{ display: 'block', mt: 1 }}>{log}</Typography>
    </Box>
  );

  const previewPane = (
    <Box sx={{ p: 2, display: 'flex', flexDirection: 'column', height: '100%', overflow: 'hidden' }}>
      <Preview pdfUrl={pdfUrl} stamp={pdfStamp} pageNumber={pageNumber} onPage={setPageNumber} onSync={handleForwardSync} />
      <Typography variant="caption" sx={{ display: 'block', mt: 1 }}>{forwardMsg}</Typography>
    </Box>
  );

  return (
    <Box sx={{ display: 'flex', flexDirection: 'column', height: '100vh' }}>
      <Box sx={{ display: 'flex', flex: 1, minHeight: 0 }}>
        <ActivityBar mode={mode} onMode={(m) => { setMode(m); if (m === 'file') setFileTreeOpen((v) => !v); }} />
        {/* File tree: Drawer overlay (Pitch 3 pattern) + inline when project open */}
        <Drawer anchor="left" variant="temporary" open={fileTreeOpen} onClose={() => setFileTreeOpen(false)}>
          <Box sx={{ width: 300, p: 1, overflow: 'auto' }}>
            <FileTree tree={tree} selected={fileName} onSelect={(p) => { setFileTreeOpen(false); void handleSelect(p); }} onDelete={handleDelete} onExpandDir={listDir1Level} mainFile={mainFile} lazy maxDepth={2} filterHidden gitStatus={gitBadgeMap} />
          </Box>
        </Drawer>
        {(mode === 'file' || root) && (
          <Box sx={{ width: 260, flexShrink: 0, overflow: 'auto', borderRight: 1, borderColor: 'divider', p: 1 }}>
            <FileTree tree={tree} selected={fileName} onSelect={handleSelect} onDelete={handleDelete} onExpandDir={listDir1Level} mainFile={mainFile} lazy maxDepth={2} filterHidden gitStatus={gitBadgeMap} />
          </Box>
        )}
        <Pane label="editor" ratio={layout.editorRatio} onRatio={(r) => setLayout((l) => ({ ...l, editorRatio: r, previewRatio: 1 - r }))}>
          {editorPane}
        </Pane>
        <PaneSplitter onDrag={(dx) => setLayout((l) => {
          const w = window.innerWidth || 1000;
          const r = Math.min(0.8, Math.max(0.2, l.editorRatio + dx / w));
          return { ...l, editorRatio: r, previewRatio: 1 - r };
        })} />
        {previewCollapsed ? (
          <Box sx={{ width: 48, flexShrink: 0, display: 'flex', alignItems: 'flex-start', justifyContent: 'center', pt: 1 }}>
            <Button size="small" onClick={() => setPreviewCollapsed(false)}>show</Button>
          </Box>
        ) : (
          <Pane label="preview" ratio={layout.previewRatio} onRatio={(r) => setLayout((l) => ({ ...l, previewRatio: r, editorRatio: 1 - r }))}>
            <Box sx={{ display: 'flex', justifyContent: 'flex-end' }}>
              <Button size="small" onClick={() => setPreviewCollapsed(true)}>hide</Button>
            </Box>
            {previewPane}
          </Pane>
        )}
      </Box>
      <BottomPanel
        tab={bottomTab}
        onTab={setBottomTab}
        problems={{ logText, root: root || workdirHint, base: mainDir, onJump: handleJump }}
        events={[]}
        terminalVisible
        height={layout.bottomHeight}
        onHeight={(h) => setLayout((l) => ({ ...l, bottomHeight: h }))}
        mainFile={mainFile}
      />
      {gitText ? (
        <Typography component="pre" variant="caption" sx={{ display: 'block', maxHeight: 80, overflow: 'auto', whiteSpace: 'pre-wrap', px: 1 }}>{gitText}</Typography>
      ) : null}
      <StatusBar
        mainFile={mainFile}
        gitBranch={gitBranch}
        phase={compilePhase}
        timer={compileTimer}
        message={log}
      />
    </Box>
  );
}
