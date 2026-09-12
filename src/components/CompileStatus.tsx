import {useEffect,useState} from 'react';
import Box from '@mui/material/Box';
import Typography from '@mui/material/Typography';
import {subscribe,BusEvent} from '../lib/events';
type St='idle'|'preparing'|'compiling'|'success'|'failure';
export default function CompileStatus(){
const [st,setSt]=useState<St>('idle');
const [started,setStarted]=useState<number|null>(null);
const [elapsed,setElapsed]=useState(0);
const [last,setLast]=useState('');
useEffect(()=>subscribe((e:BusEvent)=>{
if(e.scope!=='compile')return;
if(e.kind==='progress'&&e.message.startsWith('compiling')){setSt('compiling');setStarted(Date.now());setElapsed(0);setLast(e.message)}
else if(e.kind==='progress'){setLast(e.message)}
else if(e.kind==='success'){setSt('success');setLast(e.message)}
else if(e.kind==='error'){setSt('failure');setLast(e.message)}
}),[]);
useEffect(()=>{if(st!=='compiling'||started==null)return;const t=setInterval(()=>setElapsed(Math.floor((Date.now()-started)/1000)),500);return ()=>clearInterval(t)},[st,started]);
const color=st==='success'?'green':st==='failure'?'red':st==='compiling'?'orange':'grey';
return (<Box sx={{display:'flex',gap:1,alignItems:'center'}}><Typography variant="body2" sx={{color}}>{st}{st==='compiling'?` (${elapsed}s)`:''}</Typography><Typography variant="caption" sx={{overflow:'hidden',textOverflow:'ellipsis',whiteSpace:'nowrap',maxWidth:400}}>{last}</Typography></Box>)
}