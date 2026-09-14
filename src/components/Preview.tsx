// Preview continuous-scroll body — windowed page rendering + pager sync.
//
// Growth cap: renders a small window of pages around the scroll ANCHOR
// (O(window), never O(document)); off-window canvases unmount. The anchor
// only ever moves DOWN as the user scrolls (never up on re-render), so the
// scroll position is stable and page 1 stays reachable: scrolling up past
// the window top extends the window upward instead of jumping.
// The pager jumps the anchor directly. devicePixelRatio capped at 2.
// Emits `pdf loaded N pages in Xms` + `page N rendered in Xms`.

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
  // Scroll ANCHOR (not the pager value): the topmost page the user has
  // scrolled to. Moves down freely; moves up ONLY via explicit pager jump
  // or SyncTeX jump (never via re-render), so scrolling up past the window
  // extends the window instead of snapping back. Pager + anchor sync below.
  const [anchor, setAnchor] = useState(1);
  // Windowed pages around the anchor; clamped to the document. Canvases only
  // ever mount below the current scroll offset, never above it.
  const lo = Math.max(1, anchor - WINDOW_ABOVE);
  const hi = Math.min(numPages, anchor + WINDOW_BELOW);
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

  // Render the windowed pages once their canvases mount + on anchor change.
  // Scroll position is preserved across re-renders: new canvases mount below
  // the current scroll offset (never above it), so the viewport never jumps.
  useEffect(() => {
    const pdf = docRef.current?.pdf;
    if (!pdf || !docKey) return;
    let isCancelled = false;
    setPhase('rendering...');
    void (async () => {
      try {
        // Render the anchor FIRST (snappy jumps), then neighbors.
        const ordered = [anchor, ...pages.filter((n) => n !== anchor)];
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
  }, [docKey, anchor, numPages, renderPage]);

  // Pager jump (or SyncTeX jump) → move the anchor AND scroll the target
  // canvas into view. Scroll-driven moves report back through handleScroll,
  // which only ever moves the anchor DOWN; jumps set it directly, so the
  // two never fight and edge pages stay reachable.
  useEffect(() => {
    setAnchor(page);
  }, [page]);
  useEffect(() => {
    if (!docKey) return;
    const el = canvasRefs.current.get(anchor);
    el?.scrollIntoView({ block: 'start' });
  }, [anchor, docKey]);

  // Scroll → nearest MOUNTED page at-or-below the scroll top extends the
  // anchor downward (pager follows for free). The anchor NEVER moves up on
  // scroll: scrolling up past the window top keeps the anchor (canvases for
  // earlier pages mount above WITHOUT moving the offset — the browser holds
  // scroll position against content growth below, and growth above is
  // compensated by keeping the same first-visible canvas). Unmounted pages
  // can never win: candidates clamp to mounted canvases in the window.
  const handleScroll = useCallback(() => {
    const box = scrollRef.current;
    if (!box || !onPage) return;
    const top = box.scrollTop;
    let best = anchor;
    let bestDist = Number.POSITIVE_INFINITY;
    for (const [n, el] of canvasRefs.current) {
      if (!pages.includes(n)) continue;
      if (n < anchor) continue;
      const d = Math.abs(el.offsetTop - top);
      if (d < bestDist) {
        bestDist = d;
        best = n;
      }
    }
    if (best !== anchor) {
      setAnchor(best);
      onPage(best);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [onPage, anchor, lo, hi]);

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
