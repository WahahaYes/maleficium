import { useEffect, useState, useRef } from 'react';
import Box from '@mui/material/Box';
import Button from '@mui/material/Button';
import Typography from '@mui/material/Typography';
import MainLayout from './components/MainLayout';
import Editor from './components/Editor';
import Preview from './components/Preview';
import FileTree from './components/FileTree';
import Problems from './components/Problems';
import { openProject, listTree, loadTex, saveTex, saveTexToDisk, TreeEntry } from './lib/files';
import { compileTex } from './lib/compile';
import { emitPdf, onPdf } from './lib/preview-bus';
import { gitStatus } from './lib/git';
import { forward_sync } from './lib/synctex';
import { readTextFile } from '@tauri-apps/plugin-fs';

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
    if(r){setRoot(r);const t=await listTree(r);setTree(t);setLog('opened '+r)} 
    else setLog('open cancelled'); 
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
    } else {
      await saveTexToDisk(fileName,tex);
      setLog('saved '+fileName);
    } 
  }

  async function compile(){ 
    setLog('compiling...'); 
    const r=await compileTex(fileName,'/tmp'); 
    setLog(r.log);
    if (r.ok && r.pdfPath) {
      emitPdf(r.pdfPath);
      const workdir = '/tmp';
      const base = fileName.replace(/\.tex$/, '');
      try {
        const logContent = await readTextFile(`${workdir}/out/${base}.log`);
        setLogText(logContent);
      } catch {}
    } else if(!r.ok && r.log.includes('spawn')){
      setLog(r.log+' (tectonic not installed — bundle in P5)');
    }
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

  const base = fileName.replace(/\.tex$/, '');

  return (
    <MainLayout 
      editor={
        <Box sx={{p:2}}>
          <Box sx={{display:'flex',gap:1,mb:1}}>
            <Button variant="outlined" onClick={open}>Open Project</Button>
            <Button variant="outlined" onClick={save}>Save</Button>
            <Button variant="contained" onClick={compile}>Compile</Button>
            <Typography variant="body2" sx={{ml:1}}>{fileName}</Typography>
          </Box>
          {root?<Box sx={{maxHeight:200,overflow:'auto'}}><FileTree tree={tree} selected={fileName} onSelect={handleSelect} /></Box>:null}
          <Editor value={tex} onChange={setTex} onSave={save} />
          <Typography variant="caption" sx={{display:'block',mt:1}}>{log}</Typography>
          {logText ? <Problems logText={logText} root={root || ''} base={base} onJump={handleJump} /> : null}
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
          {logText ? <Problems logText={logText} root={root || ''} base={base} onJump={handleJump} /> : null}
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