import { useEffect, useState } from 'react';
import Box from '@mui/material/Box';
import Button from '@mui/material/Button';
import Typography from '@mui/material/Typography';
import MainLayout from './components/MainLayout';
import Editor from './components/Editor';
import Preview from './components/Preview';
import FileTree from './components/FileTree';
import { openProject, listTree, loadTex, saveTex, saveTexToDisk, TreeEntry } from './lib/files';
import { compileTex } from './lib/compile';
import { emitPdf, onPdf } from './lib/preview-bus';
const HELLO = '\\documentclass{article}\n\\begin{document}\nHello Maleficium\n\\end{document}\n';
export default function App() {
  const [tex, setTex] = useState(HELLO);
  const [root, setRoot] = useState<string|null>(null);
  const [tree, setTree] = useState<TreeEntry[]>([]);
  const [fileName, setFileName] = useState('hello.tex');
  const [log, setLog] = useState('ready');
  const [pdfUrl, setPdfUrl] = useState<string|null>(null);
  useEffect(()=>onPdf(setPdfUrl),[]);
  async function open(){ const r=await openProject(); if(r){setRoot(r);const t=await listTree(r);setTree(t);setLog('opened '+r)} else setLog('open cancelled'); }
  async function handleSelect(path:string){ if(path.endsWith('.tex')){const content=await loadTex(path);setTex(content);setFileName(path);setLog('loaded '+path);} }
  async function save(){ if(fileName.includes('/')){await saveTex(fileName,tex);setLog('saved '+fileName);} else {await saveTexToDisk(fileName,tex);setLog('saved '+fileName);} }
  async function compile(){ setLog('compiling...'); const r=await compileTex(fileName,'/tmp'); setLog(r.log); if(r.ok&&r.pdfPath){emitPdf(r.pdfPath)} else if(!r.ok&&r.log.includes('spawn')){setLog(r.log+' (tectonic not installed — bundle in P5)')} }
  return (<MainLayout editor={<Box sx={{p:2}}><Box sx={{display:'flex',gap:1,mb:1}}><Button variant="outlined" onClick={open}>Open Project</Button><Button variant="outlined" onClick={save}>Save</Button><Button variant="contained" onClick={compile}>Compile</Button><Typography variant="body2" sx={{ml:1}}>{fileName}</Typography></Box>{root?<Box sx={{maxHeight:200,overflow:'auto'}}><FileTree tree={tree} selected={fileName} onSelect={handleSelect} /></Box>:null}<Editor value={tex} onChange={setTex} onSave={save} /></Box>} preview={<Box sx={{p:2}}><Preview pdfUrl={pdfUrl} /><Typography variant="caption" sx={{display:'block',mt:1}}>{log}</Typography></Box>} />);
}