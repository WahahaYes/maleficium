import {useEffect,useRef,useState} from 'react';
import Box from '@mui/material/Box';
import Button from '@mui/material/Button';
import List from '@mui/material/List';
import ListItem from '@mui/material/ListItem';
import ListItemText from '@mui/material/ListItemText';
import Typography from '@mui/material/Typography';
import {list,subscribe,clear,BusEvent} from '../lib/events';
export default function EventLog(){
const [evts,setEvts]=useState<BusEvent[]>(()=>list());
const [filter,setFilter]=useState<string>('all');
const ref=useRef<HTMLDivElement>(null);
useEffect(()=>subscribe(()=>setEvts(list())),[]);
useEffect(()=>{const el=ref.current;if(el)el.scrollTop=el.scrollHeight},[evts]);
const shown=filter==='all'?evts:evts.filter(e=>e.kind===filter);
return (<Box><Box sx={{display:'flex',gap:1,alignItems:'center'}}><Typography variant="subtitle2">Event Log ({shown.length}/{evts.length})</Typography>{['all','info','progress','success','warn','error'].map(f=>(<Button key={f} size="small" variant={filter===f?'contained':'outlined'} onClick={()=>setFilter(f)}>{f}</Button>))}<Button size="small" onClick={()=>{clear();setEvts([])}}>Clear</Button></Box><Box ref={ref} sx={{maxHeight:160,overflow:'auto',border:'1px solid #eee',mt:1}}><List dense>{shown.slice(-500).map((e,i)=>(<ListItem key={i} disablePadding><ListItemText primary={`${new Date(e.at).toLocaleTimeString()} [${e.scope}/${e.kind}] ${e.message}`} /></ListItem>))}</List></Box></Box>)
}