type Cb=(url:string|null)=>void;
const set=new Set<Cb>();
export function emitPdf(url:string|null){set.forEach(cb=>cb(url))}
export function onPdf(cb:Cb){set.add(cb); return ()=>{set.delete(cb)}}