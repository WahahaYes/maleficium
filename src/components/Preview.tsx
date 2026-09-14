// Preview continuous-scroll body — all shells mounted, windowed bitmaps.
//
// Scroll contract: every document page owns a lightweight shell div whose
// height is reserved from probed aspect ratios, but only a small bitmap
// WINDOW around the visible page holds pdf.js pixels (bounded memory —
// shells that leave the window have their canvas backing cleared). The
// visible page is picked by IntersectionObserver (largest ratio inside the
// viewport middle band), free in BOTH directions, so page 1 stays reachable
// by scroll AND by pager. The pager (App-owned pageNumber) only drives
// jumps: a prop change that differs from the visible page scrolls AFTER the
// target bitmap lands; neighbors then render idle. Programmatic scrolls
// carry a flag so a user grab mid-jump cancels it. devicePixelRatio capped
// at 2.
// Shell count never scales with the document (buffers + observers do): one
// shell div per page is O(pages) DOM by design, and O(visible) work per
// event holds because the observer callback only records ratios while the
// rAF-throttled pick + idle-scheduled neighbors defer the rest.
// At very large page counts the DOM itself is the cost (not the bitmaps —
// those stay O(window)). The outline already covers long-doc navigation,
// and full virtualization stays a non-goal (see closeout §C-3).
// Emits `pdf loaded N pages in Xms` + `page N rendered in Xms`.

import { Box, Typography } from '@mui/material';
import { useCallback, useEffect, useLayoutEffect, useRef, useState } from 'react';
import { openPdf, openPdfFromBytes } from '../lib/pdfjs';
import { emit } from '../lib/events';
import { readFile } from '@tauri-apps/plugin-fs';
import PreviewToolbar from './PreviewToolbar';
import {
  WINDOW_ABOVE,
  WINDOW_BELOW,
  clampPage,
  compensateScrollTop,
  pickVisible,
  windowFor,
} from '../lib/previewNav';

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

/** Top padding of the scroll container (matches py:1) for jump math. */
const SCROLL_PAD_TOP = 8;

/** Jump lock lifetime: the observer landing clears earlier; this is the backstop. */
const JUMP_LOCK_MS = 1500;

/** Idle wait: the current page renders immediately, neighbors yield first. */
const idle = () => new Promise<void>((resolve) => {
  const ric = (window as unknown as { requestIdleCallback?: (cb: () => void) => void }).requestIdleCallback;
  if (typeof ric === 'function') ric(() => resolve());
  else setTimeout(() => resolve(), 0);
});

export default function Preview({ pdfUrl, stamp, pageNumber = 1, onPage, onSync, onInverse, syncDisabled }: PreviewProps) {
  const scrollRef = useRef<HTMLDivElement>(null);
  const shellRefs = useRef(new Map<number, HTMLDivElement>());
  const canvasRefs = useRef(new Map<number, HTMLCanvasElement>());
  // Bitmap identity is per document revision AND page: cleared on every doc
  // change so entries can never accumulate across recompiles (bounded by the
  // window). Keyed (page -> docKey) so the eviction pass can never clear a
  // NEWER document's bitmaps when a stale generation finishes late.
  const renderedRef = useRef(new Set<string>());
  const renderedKeys = useRef(new Map<number, string>());
  const dimsRef = useRef(new Map<number, { w: number; h: number }>());
  const [numPages, setNumPages] = useState(1);
  const [error, setError] = useState<string | null>(null);
  const [phase, setPhase] = useState('');
  const [docKey, setDocKey] = useState('');
  const [visible, setVisible] = useState(1);
  const [dimsVersion, setDimsVersion] = useState(0);
  const [renderEpoch, setRenderEpoch] = useState(0);
  const page = clampPage(pageNumber, numPages);
  // Open pdf.js document ONCE per pdfUrl+stamp: renders serve from the
  // cached handle instead of re-opening the whole document per page.
  const docRef = useRef<{ key: string; pdf: PdfDoc } | null>(null);
  const visibleRef = useRef(1);
  const pageRef = useRef(page);
  pageRef.current = page;
  const onPageRef = useRef(onPage);
  onPageRef.current = onPage;
  const ratiosRef = useRef(new Map<number, number>());
  const lockRef = useRef<{ target: number; until: number } | null>(null);
  const pendingScrollRef = useRef<number | null>(null);
  const programmaticRef = useRef(false);
  const pickRafRef = useRef<number | null>(null);
  const jumpTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const renderGenRef = useRef(0);
  const mountedRef = useRef(true);
  const dprRef = useRef(typeof window === 'undefined' ? 1 : window.devicePixelRatio || 1);
  const compRef = useRef<{ page: number; delta: number } | null>(null);

  useEffect(() => () => {
    mountedRef.current = false;
    if (pickRafRef.current != null) cancelAnimationFrame(pickRafRef.current);
    if (jumpTimerRef.current) clearTimeout(jumpTimerRef.current);
  }, []);

  const bumpDims = useCallback(() => setDimsVersion((v) => v + 1), []);

  // Render pdf.js pixels for one page onto a DETACHED canvas. Awaiting this
  // never touches mounted DOM: the caller commits via commitBitmap only when
  // the generation is still current and the page still in-window. Skips
  // pdf.js work entirely when this revision already holds the page.
  const renderBitmap = useCallback(async (
    pdf: PdfDoc,
    key: string,
    target: number,
    isCancelled: () => boolean,
  ): Promise<HTMLCanvasElement | null> => {
    if (renderedRef.current.has(key)) return canvasRefs.current.get(target) ?? null;
    const pg = await pdf.getPage(target);
    if (isCancelled()) return null;
    const scale = Math.min(window.devicePixelRatio || 1, 2);
    const viewport = pg.getViewport({ scale });
    const off = document.createElement('canvas');
    off.height = viewport.height;
    off.width = viewport.width;
    const ctx = off.getContext('2d');
    if (!ctx) throw new Error('2d context unavailable');
    const t1 = Date.now();
    await pg.render({ canvasContext: ctx, viewport }).promise;
    if (isCancelled()) return null;
    emit({ scope: 'preview', kind: 'progress', message: `page ${target} rendered in ${Date.now() - t1}ms` });
    return off;
  }, []);

  // Commit a rendered bitmap into the mounted shell canvas (drawImage — no
  // re-layout, no blank flash). Already-rendered pages commit WITHOUT
  // re-rendering: renderBitmap returns the mounted canvas itself, and the
  // drawImage below self-copies harmlessly. Records identity AFTER pixels
  // land, so a commit can never mark a page rendered that isn't.
  const commitBitmap = useCallback((target: number, key: string, off: HTMLCanvasElement | null) => {
    const canvas = canvasRefs.current.get(target);
    if (!canvas || !off) return;
    if (off !== canvas) {
      if (canvas.width !== off.width || canvas.height !== off.height) {
        canvas.width = off.width;
        canvas.height = off.height;
      }
      const ctx = canvas.getContext('2d');
      if (!ctx) return;
      ctx.drawImage(off, 0, 0);
    }
    renderedRef.current.add(key);
    renderedKeys.current.set(target, key);
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

  // Jump the viewport to a shell via rect deltas (offsetTop is
  // offsetParent-relative, so rect math against the container is exact).
  // The flag is set ONLY when scrollTop actually moves: a no-op scroll fires
  // no event, and a stale flag would eat the next genuine user scroll.
  const scrollToShell = useCallback((n: number) => {
    const box = scrollRef.current;
    const shell = shellRefs.current.get(n);
    if (!box || !shell) return;
    const delta = shell.getBoundingClientRect().top - box.getBoundingClientRect().top - SCROLL_PAD_TOP;
    const top = box.scrollTop + delta;
    if (Math.abs(top - box.scrollTop) > 0.5) {
      programmaticRef.current = true;
      box.scrollTop = top;
    }
  }, []);

  const armJumpTimeout = useCallback((target: number) => {
    if (jumpTimerRef.current) clearTimeout(jumpTimerRef.current);
    jumpTimerRef.current = setTimeout(() => {
      if (lockRef.current?.target === target) lockRef.current = null;
      if (pendingScrollRef.current === target) pendingScrollRef.current = null;
    }, JUMP_LOCK_MS);
  }, []);

  // rAF-throttled visible pick: largest observer ratio wins; a jump in flight
  // freezes reporting until the target lands (or the lock lapses, in which
  // case the user took over and we follow them).
  const applyPick = useCallback(() => {
    const entries: { page: number; ratio: number }[] = [];
    for (const [p, r] of ratiosRef.current) entries.push({ page: p, ratio: r });
    const picked = pickVisible(entries);
    if (picked == null) return;
    const lock = lockRef.current;
    if (lock && picked !== lock.target && Date.now() <= lock.until) return;
    lockRef.current = null;
    pendingScrollRef.current = null;
    if (picked !== visibleRef.current) {
      visibleRef.current = picked;
      setVisible(picked);
      onPageRef.current?.(picked);
    }
  }, []);

  const schedulePick = useCallback(() => {
    if (pickRafRef.current != null) return;
    pickRafRef.current = requestAnimationFrame(() => {
      pickRafRef.current = null;
      applyPick();
    });
  }, [applyPick]);

  // Probe page aspects (scale-1 viewports, no pixels) so shells reserve true
  // heights before bitmaps land. Page 1 publishes immediately with its aspect
  // assumed for the whole document (uniform docs settle in one layout pass);
  // later batches publish ONLY when a page differs from the assumption, with
  // an anchor snapshot each time so the viewport can be re-anchored below.
  const probeDims = useCallback(async (pdf: PdfDoc, total: number, isCancelled: () => boolean) => {
    const snapAnchor = () => {
      const box = scrollRef.current;
      const anchorPage = visibleRef.current;
      const shell = shellRefs.current.get(anchorPage);
      compRef.current = box && shell
        ? { page: anchorPage, delta: shell.getBoundingClientRect().top - box.getBoundingClientRect().top }
        : null;
    };
    const differs = (a: { w: number; h: number }, b: { w: number; h: number }) =>
      Math.abs(a.w - b.w) / Math.max(1, b.w) > 0.005 || Math.abs(a.h - b.h) / Math.max(1, b.h) > 0.005;
    try {
      const first = await pdf.getPage(1);
      if (isCancelled()) return;
      const v1 = first.getViewport({ scale: 1 });
      const base = { w: v1.width, h: v1.height };
      const dims = new Map<number, { w: number; h: number }>();
      for (let n = 1; n <= total; n++) dims.set(n, base);
      dimsRef.current = dims;
      snapAnchor();
      bumpDims();
      for (let n = 2; n <= total; n++) {
        if (isCancelled()) return;
        // eslint-disable-next-line no-await-in-loop
        const pg = await pdf.getPage(n);
        if (isCancelled()) return;
        const v = pg.getViewport({ scale: 1 });
        const next = { w: v.width, h: v.height };
        if (!differs(next, dims.get(n) as { w: number; h: number })) continue;
        dims.set(n, next);
        if (n % 10 === 0 || n === total) {
          if (isCancelled()) return;
          dimsRef.current = new Map(dims);
          snapAnchor();
          bumpDims();
          // eslint-disable-next-line no-await-in-loop
          await new Promise<void>((r) => setTimeout(r, 0));
        }
      }
    } catch {
      // Probe never blocks the preview; shells keep the default aspect.
    }
  }, [bumpDims]);

  // Document open (per pdfUrl+stamp) + canonical reset to the pager page.
  // Section-count state (numPages) commits TOGETHER with the doc identity so
  // the shell list and the observer mount in the same pass. A resolve
  // generation cancels the previous document's probe without touching the
  // render generation (renders are keyed by docKey and self-isolate).
  const resolveGenRef = useRef(0);
  useEffect(() => {
    if (!pdfUrl) return;
    let cancelled = false;
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
          if (cancelled) return;
          await yieldUi();
          setPhase('loading...');
          pdf = (isRemote ? await openPdf(pdfUrl) : await openPdfFromBytes(bytes as Uint8Array)) as unknown as PdfDoc;
          if (cancelled) return;
          emit({ scope: 'preview', kind: 'progress', message: `pdf loaded ${pdf.numPages} pages in ${Date.now() - t0}ms` });
          docRef.current = { key, pdf };
        }
        if (cancelled) return;
        await yieldUi();
        if (cancelled) return;
        // Fresh identity per revision: bitmaps, ratios, and aspects restart.
        renderedRef.current.clear();
        ratiosRef.current.clear();
        dimsRef.current = new Map();
        bumpDims();
        const target = clampPage(pageRef.current, pdf.numPages);
        visibleRef.current = target;
        setVisible(target);
        pendingScrollRef.current = target;
        lockRef.current = { target, until: Date.now() + JUMP_LOCK_MS };
        armJumpTimeout(target);
        setPhase('');
        if (!cancelled) {
          setNumPages(pdf.numPages);
          setDocKey(key);
        }
        const myResolve = ++resolveGenRef.current;
        void probeDims(pdf, pdf.numPages, () => cancelled || myResolve !== resolveGenRef.current);
      } catch (e) {
        if (!cancelled) {
          setError(`Failed to load PDF: ${String(e)}`);
          setPhase('');
        }
      }
    };
    void load();

    return () => {
      cancelled = true;
    };
  }, [pdfUrl, stamp, probeDims, armJumpTimeout]);

  // Render the bitmap window: visible page immediately, neighbors idle.
  // The jump scroll fires once the TARGET bitmap lands (not after the whole
  // window), so pager jumps stick without waiting on neighbors. Awaited work
  // NEVER touches canvas identity: render pixels offscreen, then commit —
  // clearing a canvas whose bitmap is still referenced (same pass, other
  // window) is what blanked every page but the window on large documents.
  useEffect(() => {
    const pdf = docRef.current?.pdf;
    if (!pdf || !docKey) return;
    const key = docKey;
    const target = visible;
    const total = numPages;
    const gen = ++renderGenRef.current;
    const isCancelled = () => gen !== renderGenRef.current || !mountedRef.current;
    const win = windowFor(target, total);
    const offsets: number[] = [0];
    const maxD = Math.max(WINDOW_ABOVE, WINDOW_BELOW);
    for (let d = 1; d <= maxD; d++) {
      if (d <= WINDOW_BELOW) offsets.push(d);
      if (d <= WINDOW_ABOVE) offsets.push(-d);
    }
    const ordered = offsets.map((o) => target + o).filter((n) => n >= win.lo && n <= win.hi);
    const inWindow = new Set(ordered);
    setPhase('rendering...');
    void (async () => {
      try {
        if (isCancelled()) return;
        // Render the target to an offscreen canvas FIRST: the commit below
        // swaps it in only if this generation is still current AND the page
        // is still the visible one (a fast scroll-away cancels the swap
        // instead of painting a stale bitmap over the new window).
        const pageKey = `${key}#${target}`;
        const nodes = await renderBitmap(pdf, pageKey, target, isCancelled);
        if (isCancelled()) return;
        if (!inWindow.has(target) || visibleRef.current !== target) return;
        commitBitmap(target, pageKey, nodes);
        if (!isCancelled() && pendingScrollRef.current != null) scrollToShell(pendingScrollRef.current);
        for (const n of ordered.slice(1)) {
          if (isCancelled()) break;
          // eslint-disable-next-line no-await-in-loop
          await idle();
          if (isCancelled()) break;
          // eslint-disable-next-line no-await-in-loop
          const nb = await renderBitmap(pdf, `${key}#${n}`, n, isCancelled);
          if (isCancelled()) break;
          // Same guard per neighbor: skip the commit when the window moved
          // on while this bitmap was rendering.
          if (visibleRef.current !== target) break;
          commitBitmap(n, `${key}#${n}`, nb);
        }
        if (!isCancelled()) {
          // Memory contract: shells stay for scroll height, but bitmaps
          // outside the window are freed (canvas backing cleared). Keyed by
          // the captured doc identity so a newer document's bitmaps (same
          // page numbers, different key) are never touched.
          for (const [n, canvas] of canvasRefs.current) {
            if (renderedKeys.current.get(n) !== key || inWindow.has(n)) continue;
            renderedKeys.current.delete(n);
            canvas.width = 0;
            canvas.height = 0;
          }
        }
      } finally {
        if (!isCancelled()) setPhase('');
      }
    })();
  }, [docKey, visible, numPages, renderEpoch, scrollToShell]);

  // Pager/SyncTeX jump: a prop page that differs from the visible page is an
  // EXTERNAL jump (our own scroll reports echo back equal and no-op here).
  // The bitmap window moves at once so the target renders; the scroll lands
  // in the render effect once that bitmap exists. User scrolls mid-jump
  // cancel the lock via the onScroll handler above — the scroll wins, no
  // second programmatic scroll fires after it.
  useEffect(() => {
    if (!docKey) return;
    if (page === visibleRef.current) return;
    visibleRef.current = page;
    setVisible(page);
    pendingScrollRef.current = page;
    lockRef.current = { target: page, until: Date.now() + JUMP_LOCK_MS };
    armJumpTimeout(page);
  }, [page, docKey, armJumpTimeout]);

  // Visible pick: one observer over all shells, middle-band ratios, largest
  // wins (rAF-throttled). The callback itself is O(changed) — it only
  // records ratios; the O(window) pick runs at most once per frame.
  // Shells mount per document, so observe on section count (numPages), not
  // on doc identity: re-mounts re-fire this effect even when an idempotent
  // recompile yields the SAME page count.
  // Replaces all offsetTop math.
  useEffect(() => {
    const box = scrollRef.current;
    if (!box || !docKey) return;
    ratiosRef.current.clear();
    const observer = new IntersectionObserver(
      (entries) => {
        for (const e of entries) {
          const p = Number((e.target as HTMLElement).dataset.page);
          if (!Number.isFinite(p)) continue;
          ratiosRef.current.set(p, e.isIntersecting ? e.intersectionRatio : 0);
        }
        schedulePick();
      },
      { root: box, threshold: Array.from({ length: 11 }, (_, i) => i / 10), rootMargin: '-40% 0px -40% 0px' },
    );
    // Shell refs populate during this commit's layout; the observer must see
    // them, so observe on the next frame (disconnect still cleans up).
    let raf = requestAnimationFrame(() => {
      for (const [, el] of shellRefs.current) observer.observe(el);
    });
    return () => {
      cancelAnimationFrame(raf);
      observer.disconnect();
    };
  }, [docKey, numPages, schedulePick]);

  // Probe updates give non-uniform shells their true heights after mount;
  // re-anchor the SAME page the snapshot named so content doesn't drift under
  // the reader. A jump in flight re-lands exact instead.
  useLayoutEffect(() => {
    const c = compRef.current;
    compRef.current = null;
    if (!c) return;
    const pending = pendingScrollRef.current;
    if (pending != null) {
      scrollToShell(pending);
      return;
    }
    const box = scrollRef.current;
    const shell = shellRefs.current.get(c.page);
    if (!box || !shell) return;
    const shift = shell.getBoundingClientRect().top - box.getBoundingClientRect().top - c.delta;
    if (Math.abs(shift) <= 0.5) return;
    // Growth (content above gained height) pushes down by exactly the added
    // height; refinement shrink pulls up signed — same anchor, both ways.
    const top = shift > 0 ? compensateScrollTop(box.scrollTop, shift) : box.scrollTop + shift;
    if (Math.abs(top - box.scrollTop) > 0.5) {
      programmaticRef.current = true;
      box.scrollTop = top;
    }
  }, [dimsVersion, scrollToShell]);

  // Re-render bitmaps when the backing scale changes (e.g. the window moved
  // across monitors). Plain width changes only re-scale CSS — no re-render.
  useEffect(() => {
    let timer: ReturnType<typeof setTimeout> | null = null;
    const onResize = () => {
      if (timer) clearTimeout(timer);
      timer = setTimeout(() => {
        const dpr = window.devicePixelRatio || 1;
        if (dpr !== dprRef.current) {
          dprRef.current = dpr;
          renderedRef.current.clear();
          setRenderEpoch((e) => e + 1);
        }
      }, 300);
    };
    window.addEventListener('resize', onResize);
    return () => {
      window.removeEventListener('resize', onResize);
      if (timer) clearTimeout(timer);
    };
  }, []);

  // Picking comes from the observer (rAF-throttled). Scroll events only
  // separate programmatic jumps from user scrolls: a user grab mid-jump
  // cancels it so the viewport never fights the hand on the scrollbar.
  const handleScroll = useCallback(() => {
    if (programmaticRef.current) {
      programmaticRef.current = false;
      return;
    }
    if (lockRef.current) {
      lockRef.current = null;
      pendingScrollRef.current = null;
    }
  }, []);

  const setShellRef = useCallback((n: number) => (el: HTMLDivElement | null) => {
    if (el) shellRefs.current.set(n, el);
    else shellRefs.current.delete(n);
  }, []);

  const setCanvasRef = useCallback((n: number) => (el: HTMLCanvasElement | null) => {
    if (el) canvasRefs.current.set(n, el);
    else canvasRefs.current.delete(n);
  }, []);

  // Every page owns a shell (stable scroll height from the probed aspect);
  // the canvas fills it, so unrendered pages read as placeholders, never void.
  // Sized from the probed aspect where known: width-flexible (max 820) with
  // height from aspect-ratio, so narrow-pane zoom is CSS-only and DPR changes
  // are the sole re-render trigger below.
  const shellStyle = (n: number): React.CSSProperties => {
    const d = dimsRef.current.get(n);
    return {
      position: 'relative',
      width: '100%',
      maxWidth: 820,
      flexShrink: 0,
      aspectRatio: d ? `${d.w} / ${d.h}` : '1 / 1.4143',
      backgroundColor: 'rgba(128, 128, 128, 0.12)',
    };
  };

  const allPages: number[] = [];
  for (let n = 1; n <= numPages; n++) allPages.push(n);

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
        {allPages.map((n) => (
          <div key={`${docKey}#${n}`} ref={setShellRef(n)} data-page={n} style={shellStyle(n)}>
            <canvas
              ref={setCanvasRef(n)}
              data-page={n}
              onClick={handleCanvasClick}
              className="synctex-canvas"
              style={{ position: 'absolute', inset: 0, width: '100%', height: '100%' }}
              title={syncDisabled ? 'SyncTeX unavailable while compiling' : 'Click for inverse SyncTeX'}
            />
          </div>
        ))}
      </Box>
    </Box>
  );
}
