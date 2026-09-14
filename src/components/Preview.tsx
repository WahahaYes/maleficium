// Preview continuous-scroll body — windowed page rendering + pager sync.
//
// Growth cap: renders a small window of pages around the current page
// (O(window), never O(document)); off-window canvases unmount. Scrolling
// moves through pages; the pager follows and still jumps directly.
// devicePixelRatio capped at 2. Emits `pdf loaded N pages in Xms` +
// `page N rendered in Xms`.

import { Box, Typography } from '@mui/material';
import { useCallback, useEffect, useRef, useState } from 'react';
import { openPdf, openPdfFromBytes } from '../lib/pdfjs';
import { emit } from '../lib/events';
import { readFile } from '@tauri-apps/plugin-fs';
import PreviewToolbar from './PreviewToolbar';

interface PreviewProps {
  pdfUrl: string | null;
  stamp: number;
  pageNumber?: number;
  onPage?: (p: number) => void;
  onSync?: () => void;
  /** Inverse SyncTeX: canvas click → editor line (disabled while compiling). */
  onInverse?: (page: number, x: number, y: number) => void;
  syncDisabled?: boolean;
}

interface PdfPage {
  getViewport: (o: { scale: number }) => { height: number; width: number };
  render: (o: { canvasContext: CanvasRenderingContext2D; viewport: unknown }) => { promise: Promise<void> };
}

interface PdfDoc {
  numPages: number;
  getPage: (n: number) => Promise<PdfPage>;
}

/** Pages rendered around the current page (above + below). */
const WINDOW_ABOVE = 1;
const WINDOW_BELOW = 2;

export default function Preview({ pdfUrl, stamp, pageNumber = 1, onPage, onSync, onInverse, syncDisabled }: PreviewProps) {
  const scrollRef = useRef<HTMLDivElement>(null);
  const canvasRefs = useRef(new Map<number, HTMLCanvasElement>());
  const renderedRef = useRef(new Map<string, number>());
  const [numPages, setNumPages] = useState(1);
  const [error, setError] = useState<string | null>(null);
  const [phase, setPhase] = useState('');
  const [docKey, setDocKey] = useState('');
  const page = Math.min(Math.max(1, pageNumber), numPages);
  // Open pdf.js document ONCE per pdfUrl+stamp: renders serve from the
  // cached handle instead of re-opening the whole document per page.
  const docRef = useRef<{ key: string; pdf: PdfDoc } | null>(null);
  // Windowed pages around the current page; clamped to the document.
  // Sized so the scroll position is STABLE across re-renders: canvases only
  // ever mount below the current offset, never above it.
  const lo = Math.max(1, page - WINDOW_ABOVE);
  const hi = Math.min(numPages, page + WINDOW_BELOW);
  const pages: number[] = [];
  for (let n = lo; n <= hi; n++) pages.push(n);

  const renderPage = useCallback(async (pdf: PdfDoc, key: string, target: number, isCancelled: () => boolean) => {
    if (renderedRef.current.get(key) === target) return;
    const canvas = canvasRefs.current.get(target);
    if (!canvas) return;
    const pg = await pdf.getPage(target);
    if (isCancelled()) return;
    const scale = Math.min(window.devicePixelRatio || 1, 2) * 1.0;
    const viewport = pg.getViewport({ scale });
    canvas.height = viewport.height;
    canvas.width = viewport.width;
    const ctx = canvas.getContext('2d');
    if (!ctx) throw new Error('2d context unavailable');
    const t1 = Date.now();
    await pg.render({ canvasContext: ctx, viewport }).promise;
    if (isCancelled()) return;
    renderedRef.current.set(key, target);
    emit({ scope: 'preview', kind: 'progress', message: `page ${target} rendered in ${Date.now() - t1}ms` });
  }, []);

  const handleCanvasClick = (e: React.MouseEvent<HTMLCanvasElement>) => {
    if (syncDisabled || !onInverse) return;
    const canvas = e.currentTarget;
    const hit = Number(canvas.dataset.page ?? page);
    const rect = canvas.getBoundingClientRect();
    // pdf.js viewport scale (render): CSS px → PDF points. Inverse SyncTeX
    // wants PDF points, not screen px — unscale here (DPR applied at render).
    const scaleX = canvas.width / Math.max(1, rect.width);
    const scaleY = canvas.height / Math.max(1, rect.height);
    const x = (e.clientX - rect.left) * scaleX;
    const y = (e.clientY - rect.top) * scaleY;
    // Hit feedback WITHOUT remount: toggle a class, remove after 300ms.
    // (The old `key={flash}` trick recreated the canvas and blanked the view.)
    canvas.classList.remove('synctex-hit');
    // Force reflow so rapid clicks retrigger the outline.
    void canvas.offsetWidth;
    canvas.classList.add('synctex-hit');
    setTimeout(() => canvas.classList.remove('synctex-hit'), 300);
    onInverse(hit, Math.round(x), Math.round(y));
  };

  // Document open (per pdfUrl+stamp) + current-page render.
  useEffect(() => {
    if (!pdfUrl) return;
    let isCancelled = false;
    const yieldUi = () => new Promise<void>((r) => setTimeout(r, 0));

    const load = async () => {
      try {
        setError(null);
        const key = `${pdfUrl}#${stamp}`;
        let pdf: PdfDoc;
        if (docRef.current?.key === key) {
          setPhase('rendering...');
          pdf = docRef.current.pdf;
        } else {
          setPhase('reading...');
          const t0 = Date.now();
          const isRemote = pdfUrl.startsWith('blob:') || pdfUrl.startsWith('http://') || pdfUrl.startsWith('https://') || pdfUrl.startsWith('asset:');
          const bytes = isRemote ? null : new Uint8Array(await readFile(pdfUrl));
          if (isCancelled) return;
          await yieldUi();
          setPhase('loading...');
          pdf = (isRemote ? await openPdf(pdfUrl) : await openPdfFromBytes(bytes as Uint8Array)) as unknown as PdfDoc;
          if (isCancelled) return;
          emit({ scope: 'preview', kind: 'progress', message: `pdf loaded ${pdf.numPages} pages in ${Date.now() - t0}ms` });
          docRef.current = { key, pdf };
        }
        await yieldUi();
        setNumPages(pdf.numPages);
        setDocKey(key);
        setPhase('');
      } catch (e) {
        if (!isCancelled) {
          setError(`Failed to load PDF: ${String(e)}`);
          setPhase('');
        }
      }
    };
    void load();

    return () => {
      isCancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [pdfUrl, stamp]);

  // Render the windowed pages once their canvases mount + on page change.
  // Scroll position is preserved across re-renders: new canvases mount below
  // the current scroll offset (never above it), so the viewport never jumps.
  useEffect(() => {
    const pdf = docRef.current?.pdf;
    if (!pdf || !docKey) return;
    let isCancelled = false;
    setPhase('rendering...');
    void (async () => {
      try {
        // Render the current page FIRST (snappy pager jumps), then neighbors.
        const ordered = [page, ...pages.filter((n) => n !== page)];
        for (const n of ordered) {
          if (isCancelled) break;
          // eslint-disable-next-line no-await-in-loop
          await renderPage(pdf, `${docKey}#${n}`, n, () => isCancelled);
        }
      } finally {
        if (!isCancelled) setPhase('');
      }
    })();
    return () => {
      isCancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [docKey, page, numPages, renderPage]);

  // Pager jump → scroll the target canvas into view. Scroll-driven moves
  // report back through handleScroll, which clamps to mounted canvases;
  // programmatic jumps set state directly, so the two never fight: without
  // this split the scroll effect fires before the target canvas mounts,
  // nearest-mounted wins, and edge pages become unreachable.
  useEffect(() => {
    const el = canvasRefs.current.get(page);
    el?.scrollIntoView({ block: 'start' });
  }, [page, docKey]);

  // Scroll → nearest MOUNTED page becomes current (pager follows for free).
  // Unmounted pages can never win: the window only ever contains pages
  // around the current one, so clamping to mounted canvases is exact.
  const handleScroll = useCallback(() => {
    const box = scrollRef.current;
    if (!box || !onPage) return;
    const top = box.scrollTop;
    let best = page;
    let bestDist = Number.POSITIVE_INFINITY;
    for (const [n, el] of canvasRefs.current) {
      if (!pages.includes(n)) continue;
      const d = Math.abs(el.offsetTop - top);
      if (d < bestDist) {
        bestDist = d;
        best = n;
      }
    }
    if (best !== page) onPage(best);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [onPage, page, lo, hi]);

  const setCanvasRef = useCallback((n: number) => (el: HTMLCanvasElement | null) => {
    if (el) canvasRefs.current.set(n, el);
    else canvasRefs.current.delete(n);
  }, []);

  if (!pdfUrl) return <Typography variant="body1">No PDF yet</Typography>;

  if (error) return <Typography variant="body1" color="error">{error}</Typography>;

  return (
    <Box sx={{ display: 'flex', flexDirection: 'column', height: '100%', minHeight: 0, overflow: 'hidden' }}>
      <PreviewToolbar
        pageNumber={page}
        totalPages={numPages}
        onPage={(p) => onPage?.(p)}
        onSync={() => onSync?.()}
        syncDisabled={syncDisabled}
      />
      {phase ? <Typography variant="caption">{phase}</Typography> : null}
      <Box
        ref={scrollRef}
        onScroll={handleScroll}
        sx={{ flex: 1, overflowY: 'auto', minHeight: 0, display: 'flex', flexDirection: 'column', alignItems: 'center', gap: 1, py: 1 }}
      >
        {pages.map((n) => (
          <canvas
            key={`${docKey}#${n}`}
            ref={setCanvasRef(n)}
            data-page={n}
            onClick={handleCanvasClick}
            className="synctex-canvas"
            style={{ maxWidth: '100%', flexShrink: 0 }}
            title={syncDisabled ? 'SyncTeX unavailable while compiling' : 'Click for inverse SyncTeX'}
          />
        ))}
      </Box>
    </Box>
  );
}
