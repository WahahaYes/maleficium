import { useEffect, useState, useRef } from 'react';
import CompileStatus from './components/CompileStatus';
import Box from '@mui/material/Box';
import Button from '@mui/material/Button';
import Typography from '@mui/material/Typography';
import MainLayout from './components/MainLayout';
import Editor from './components/Editor';
import Preview from './components/Preview';
import FileTree from './components/FileTree';
import Problems from './components/Problems';
import EventLog from './components/EventLog';
import { openProject, listTree, loadTex, saveTex, saveTexToDisk, TreeEntry } from './lib/files';
import { compileTex, onCompileLine, cancelCompile } from './lib/compile';
import { emitPdf, onPdf } from './lib/preview-bus';
import { gitStatus } from './lib/git';
import { forward_sync } from './lib/synctex';
import { emit } from './lib/events';
import { mkdir, readTextFile } from '@tauri-apps/plugin-fs';

const HELLO = '\\documentclass{article}\n\\begin{document}\nHello Maleficium\n\\end{document}\n';

export default function App() {
  const [tex, setTex] = useState(HELLO);
  const [root, setRoot] = useState<string|null>(null);
  const [tree, setTree] = useState<TreeEntry[]>([]);
  const [fileName, setFileName] = useState('hello.tex');
  const [log, setLog] = useState('ready');
  const [logText, setLogText] = useState('');
  const [gitText, setGitText] = useState('');
  const [forwardMsg, setForwardMsg] = useState('');
  const [pdfUrl, setPdfUrl] = useState<string|null>(null);
  const [currentLine, setCurrentLine] = useState(1);
  const autosaveTimeout = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(()=>onPdf(setPdfUrl),[]);

  useEffect(()=>{ const h=()=>{ cancelCompile().catch(()=>{}); }; window.addEventListener('beforeunload',h); return ()=>window.removeEventListener('beforeunload',h); },[]);
  useEffect(() => {
    if (fileName.includes('/')) {
      if (autosaveTimeout.current) clearTimeout(autosaveTimeout.current);
      autosaveTimeout.current = setTimeout(() => {
        saveTex(fileName, tex);
        setLog('autosaved ' + new Date().toTimeString().slice(0, 8));
      }, 1200);
    }
    return () => {
      if (autosaveTimeout.current) clearTimeout(autosaveTimeout.current);
    };
  }, [tex]);

  async function open(){ 
    const r=await openProject(); 
    if(r){setRoot(r);const t=await listTree(r);setTree(t);setLog('opened '+r); emit({scope:'fs',kind:'info',message:'opened '+r})} 
    else {setLog('open cancelled'); emit({scope:'fs',kind:'warn',message:'cancelled'});} 
  }

  async function handleSelect(path:string){ 
    if(path.endsWith('.tex')){
      const content=await loadTex(path);
      setTex(content);
      setFileName(path);
      setLog('loaded '+path);
      const logName = path.replace(/\.tex$/, '.log');
      try {
        const logContent = await readTextFile(logName);
        setLogText(logContent);
      } catch {}
    } 
  }

  async function save(){ 
    if(fileName.includes('/')){
      await saveTex(fileName,tex);
      setLog('saved '+fileName);
      emit({scope:'fs',kind:'success',message:'saved '+fileName});
    } else {
      await saveTexToDisk(fileName,tex);
      setLog('saved '+fileName);
      emit({scope:'fs',kind:'success',message:'saved '+fileName});
    } 
  }

  async function compile(){
    emit({scope:'compile',kind:'progress',message:'compiling '+fileName});
    setLog('compiling...');
    // Write-then-compile: the engine reads from disk, so persist first.
    let target: string;
    let workdir: string;
    let unlisten: ()=>void = ()=>{};
    try { unlisten = await onCompileLine((line)=>emit({scope:'compile',kind:'progress',message:String(line).slice(0,300)})); } catch {}
    const t0 = Date.now(); const hb = setInterval(()=>emit({scope:'compile',kind:'progress',message:`still compiling ${fileName} (${Math.floor((Date.now()-t0)/1000)}s)`}), 5000);
    try {
      if (fileName.includes('/')) {
        await saveTex(fileName, tex);
        target = fileName;
        workdir = fileName.slice(0, fileName.lastIndexOf('/')) || '/tmp';
      } else {
        workdir = '/tmp/maleficium-untitled';
        await mkdir(workdir, { recursive: true });
        target = workdir + '/' + fileName;
        await saveTex(target, tex);
      }
    } catch(e){
      emit({scope:'compile',kind:'error',message:'save failed: '+String(e).slice(0,200)});
      clearInterval(hb); try{unlisten();}catch{}
      return;
    }
    const main = target.slice(target.lastIndexOf('/') + 1);
    const r = await compileTex(target, workdir);
    setLog(r.log);
    if (r.ok && r.pdfPath) {
      emit({scope:'compile',kind:'success',message:'compiled '+String(r.pdfPath)});
      emitPdf(r.pdfPath);
      emit({scope:'preview',kind:'success',message:'preview '+String(r.pdfPath)});
      try {
        const logContent = await readTextFile(`${workdir}/out/${main.replace(/\.tex$/, '.log')}`);
        setLogText(logContent);
      } catch {}
    } else if (!r.ok && r.log.includes('spawn')) {
      setLog(r.log + ' (sidecar failed — see notes/01-compile-events/STATUS.md)');
      emit({scope:'compile',kind:'error',message:String(r.log).slice(0,300)});
      try { const c = await readTextFile(`${workdir}/out/${main.replace(/\.tex$/, '.log')}`); setLogText(c); } catch { setLogText(r.log); }
    } else if (!r.ok) {
      emit({scope:'compile',kind:'error',message:String(r.log).slice(0,300)});
      try { const c = await readTextFile(`${workdir}/out/${main.replace(/\.tex$/, '.log')}`); setLogText(c); } catch { setLogText(r.log); }
    }
    clearInterval(hb); try { unlisten(); } catch {}
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
  }

  function handleJump(absPath: string, line: number){
    loadTex(absPath).then(content => {
      setTex(content);
      setFileName(absPath);
      setCurrentLine(line);
    });
  }

  const workdirHint = fileName.includes('/') ? fileName.slice(0,fileName.lastIndexOf('/')) : '/tmp/maleficium-untitled';

  return (
    <MainLayout 
      editor={
        <Box sx={{p:2}}>
          <Box sx={{display:'flex',gap:1,mb:1}}>
            <Button variant="outlined" onClick={open}>Open Project</Button>
            <Button variant="outlined" onClick={save}>Save</Button>
            <Button variant="contained" onClick={compile}>Compile</Button>
            <Typography variant="body2" sx={{ml:1}}>{fileName}</Typography>
          <CompileStatus/>
          </Box>
          {root?<Box sx={{maxHeight:200,overflow:'auto'}}><FileTree tree={tree} selected={fileName} onSelect={handleSelect} /></Box>:null}
          <Editor value={tex} onChange={setTex} onSave={save} />
          <Typography variant="caption" sx={{display:'block',mt:1}}>{log}</Typography>
          {logText ? <Problems logText={logText} root={root || workdirHint} base={workdirHint} onJump={handleJump} /> : null}
          <EventLog/>
          <Box sx={{display:'flex',gap:1,mt:1}}>
            <Button variant="outlined" onClick={handleForwardSync}>Forward SyncTeX</Button>
            <Button variant="outlined" onClick={handleGitStatus}>Git Status</Button>
          </Box>
          <Typography variant="caption" sx={{display:'block',mt:1}}>{forwardMsg}</Typography>
          <Typography component="pre" variant="caption" sx={{display:'block',mt:1,whiteSpace:'pre-wrap'}}>{gitText}</Typography>
        </Box>
      }
      preview={
        <Box sx={{p:2}}>
          <Preview pdfUrl={pdfUrl} />
          <Typography variant="caption" sx={{display:'block',mt:1}}>{log}</Typography>
          {logText ? <Problems logText={logText} root={root || workdirHint} base={workdirHint} onJump={handleJump} /> : null}
          <Box sx={{display:'flex',gap:1,mt:1}}>
            <Button variant="outlined" onClick={handleForwardSync}>Forward SyncTeX</Button>
            <Button variant="outlined" onClick={handleGitStatus}>Git Status</Button>
          </Box>
          <Typography variant="caption" sx={{display:'block',mt:1}}>{forwardMsg}</Typography>
          <Typography component="pre" variant="caption" sx={{display:'block',mt:1,whiteSpace:'pre-wrap'}}>{gitText}</Typography>
        </Box>
      }
    />
  );
}