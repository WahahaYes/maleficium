// Preview paginated body — visible page only + LRU 5-page cache.
//
// Growth cap: single canvas + at most 5 cached page bitmaps; stale canvas
// cleared before render; devicePixelRatio capped at 2. Emits
// `pdf loaded N pages in Xms` + `page N rendered in Xms` (01 handoff template).

import { Typography } from '@mui/material';
import { useEffect, useRef, useState } from 'react';
import { openPdf, openPdfFromBytes } from '../lib/pdfjs';
import { emit } from '../lib/events';
import { readFile } from '@tauri-apps/plugin-fs';
import PreviewToolbar from './PreviewToolbar';

interface PreviewProps {
  pdfUrl: string | null;
  stamp: number;
  pageNumber?: number;
  onPage?: (p: number) => void;
  collapsible?: boolean;
  /** Stub: same pdfUrl reference; Tauri Window plugin deferred. */
  popout?: boolean;
  onSync?: () => void;
}

const LRU_MAX = 5;
const pageCache = new Map<string, unknown[]>();

function cacheKey(url: string, page: number): string {
  return `${url}#${page}`;
}

export function touchPageCache(url: string, page: number, handle: unknown): void {
  const k = cacheKey(url, page);
  pageCache.delete(k);
  pageCache.set(k, [handle]);
  while (pageCache.size > LRU_MAX) {
    const oldest = pageCache.keys().next().value as string;
    pageCache.delete(oldest);
  }
}

export default function Preview({ pdfUrl, stamp, pageNumber = 1, onPage, popout, onSync }: PreviewProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [numPages, setNumPages] = useState(1);
  const [error, setError] = useState<string | null>(null);
  const [phase, setPhase] = useState('');
  const page = Math.min(Math.max(1, pageNumber), numPages);

  useEffect(() => {
    if (popout && pdfUrl) {
      // eslint-disable-next-line no-console
      console.log('popout: preview (stub — same pdfUrl reference)');
      window.open(pdfUrl.startsWith('blob:') ? pdfUrl : undefined, '_blank');
    }
  }, [popout, pdfUrl]);

  useEffect(() => {
    if (!pdfUrl || !canvasRef.current) return;

    let isCancelled = false;
    const yieldUi = () => new Promise<void>((r) => setTimeout(r, 0));

    const loadImage = async () => {
      try {
        setError(null);
        const canvas = canvasRef.current!;
        canvas.getContext('2d')?.clearRect(0, 0, canvas.width, canvas.height);
        setPhase('reading...');
        const t0 = Date.now();
        const isRemote = pdfUrl.startsWith('blob:') || pdfUrl.startsWith('http://') || pdfUrl.startsWith('https://') || pdfUrl.startsWith('asset:');
        const bytes = isRemote ? null : new Uint8Array(await readFile(pdfUrl));
        if (isCancelled) return;
        await yieldUi();
        setPhase('loading...');
        const pdf = isRemote ? await openPdf(pdfUrl) : await openPdfFromBytes(bytes as Uint8Array);
        if (isCancelled) return;
        emit({ scope: 'preview', kind: 'progress', message: `pdf loaded ${pdf.numPages} pages in ${Date.now() - t0}ms` });
        await yieldUi();
        setNumPages(pdf.numPages);
        const target = Math.min(Math.max(1, pageNumber), pdf.numPages);
        const pg = await pdf.getPage(target);
        const scale = Math.min(window.devicePixelRatio || 1, 2) * 1.0;
        const viewport = pg.getViewport({ scale });
        canvas.height = viewport.height;
        canvas.width = viewport.width;
        const ctx = canvas.getContext('2d');
        if (!ctx) throw new Error('2d context unavailable');
        setPhase('rendering...');
        await yieldUi();
        const t1 = Date.now();
        await pg.render({ canvasContext: ctx, viewport }).promise;
        if (isCancelled) return;
        touchPageCache(pdfUrl, target, pg);
        emit({ scope: 'preview', kind: 'progress', message: `page ${target} rendered in ${Date.now() - t1}ms` });
        setPhase('');
      } catch (e) {
        if (!isCancelled) {
          setError(`Failed to load PDF: ${String(e)}`);
          setPhase('');
        }
      }
    };
    loadImage();

    return () => {
      isCancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [pdfUrl, stamp, pageNumber]);

  if (!pdfUrl) return <Typography variant="body1">No PDF yet</Typography>;

  if (error) return <Typography variant="body1" color="error">{error}</Typography>;

  return (
    <>
      <PreviewToolbar
        pageNumber={page}
        totalPages={numPages}
        onPage={(p) => onPage?.(p)}
        onSync={() => onSync?.()}
        onPopout={() => {
          // eslint-disable-next-line no-console
          console.log('popout: preview toolbar (stub)');
        }}
      />
      {phase ? <Typography variant="caption">{phase}</Typography> : null}
      <canvas ref={canvasRef} onClick={() => onSync?.()} style={{ maxWidth: '100%' }} />
    </>
  );
}
