// Bitmap windowing for the continuous-scroll PDF preview.
//
// Every document page owns a lightweight shell, but only a small WINDOW
// around the visible page holds pdf.js pixels: shells that leave the window
// have their canvas backing cleared, so memory is O(window) not O(pages).
// The visible page is picked by IntersectionObserver (largest ratio inside
// the viewport middle band), rAF-throttled. devicePixelRatio capped at 2.
// Probe + render orchestration without generation counters: every async task
// re-validates the exact thing it is about to touch (doc identity, page
// membership, mount state) instead of racing a shared counter.
// Emits `page N rendered in Xms`.

import { useCallback, useEffect, useRef, useState, type RefObject } from 'react';
import type { PdfTextContent, PdfViewport } from '../lib/pdfjs';
import { emit } from '../lib/events';
import {
  PREFETCH_AHEAD,
  PREFETCH_BEHIND,
  WINDOW_ABOVE,
  WINDOW_BELOW,
  pickVisible,
  windowFor,
} from '../lib/previewNav';
import {
  backingWidth,
  cssScaleFor,
  renderGeometry,
  stalePages,
  type PaintedPage,
} from '../lib/zoom';
import type { DocLike, TextLayerCtor } from './usePdfDocument';

/** Idle wait: the current page renders immediately, neighbors yield first. */
const idle = () =>
  new Promise<void>((resolve) => {
    const ric = (window as unknown as { requestIdleCallback?: (cb: () => void) => void })
      .requestIdleCallback;
    if (typeof ric === 'function') ric(() => resolve());
    else setTimeout(() => resolve(), 0);
  });

export interface BitmapWindowParams {
  docKey: string;
  numPages: number;
  visible: number;
  docRef: RefObject<{ key: string; pdf: DocLike } | null>;
  textLayerRef: RefObject<TextLayerCtor | null>;
  scrollRef: RefObject<HTMLDivElement | null>;
  shellRefs: RefObject<Map<number, HTMLDivElement>>;
  canvasRefs: RefObject<Map<number, HTMLCanvasElement>>;
  renderedRef: RefObject<Set<string>>;
  renderedKeys: RefObject<Map<number, string>>;
  inFlightRef: RefObject<Map<string, number>>;
  ratiosRef: RefObject<Map<number, number>>;
  visibleRef: RefObject<number>;
  lockRef: RefObject<{ target: number; until: number } | null>;
  pendingScrollRef: RefObject<number | null>;
  onPageRef: RefObject<((p: number) => void) | undefined>;
  setVisible: (n: number) => void;
  setPhase: (s: string) => void;
  scrollToShell: (n: number) => void;
}

export interface BitmapWindow {
  setShellRef: (n: number) => (el: HTMLDivElement | null) => void;
  setCanvasRef: (n: number) => (el: HTMLCanvasElement | null) => void;
  setTextRef: (n: number) => (el: HTMLDivElement | null) => void;
}

// Selectable-text overlay for one committed page. Span fonts size from
// --total-scale-factor, so pin it to the CSS-px viewport scale first.
export function commitTextLayer(
  layer: HTMLElement,
  ctor: TextLayerCtor,
  textContent: PdfTextContent,
  textViewport: PdfViewport,
): void {
  layer.replaceChildren();
  layer.style.setProperty('--total-scale-factor', String(textViewport.scale));
  layer.style.setProperty('--scale-round-x', '1px');
  layer.style.setProperty('--scale-round-y', '1px');
  try {
    void new ctor({
      textContentSource: textContent,
      container: layer,
      viewport: textViewport,
    }).render();
  } catch {
    /* text layer never blocks paint — canvas already committed */
  }
}

export function useBitmapWindow({
  docKey,
  numPages,
  visible,
  docRef,
  textLayerRef,
  scrollRef,
  shellRefs,
  canvasRefs,
  renderedRef,
  renderedKeys,
  inFlightRef,
  ratiosRef,
  visibleRef,
  lockRef,
  pendingScrollRef,
  onPageRef,
  setVisible,
  setPhase,
  scrollToShell,
}: BitmapWindowParams): BitmapWindow {
  const textRefs = useRef(new Map<number, HTMLDivElement>());
  const pickRafRef = useRef<number | null>(null);
  const recheckRef = useRef<(() => void) | null>(null);
  // Phase is a count of outstanding window renders, not a boolean: two
  // overlapping passes must not let the loser clear the winner's
  // `rendering...` line while pixels are in flight.
  const phaseCountRef = useRef(0);
  const [renderEpoch, setRenderEpoch] = useState(0);

  useEffect(
    () => () => {
      if (pickRafRef.current != null) cancelAnimationFrame(pickRafRef.current);
    },
    [],
  );

  // Render pdf.js pixels for one page onto a detached canvas. Awaiting this
  // never touches mounted DOM: the caller commits via commitBitmap only when
  // the page still belongs to the live document and window. Skips pdf.js
  // work entirely when this revision already holds the page. Cancellation is
  // by token: each render call takes its own `alive()` that is false only
  // when its own document was superseded or a newer render of the same page
  // started.
  //
  // The canvas is rasterized at one backing pixel per device pixel, and zoom
  // re-renders rather than stretching. Selectable text comes from a pdf.js
  // TextLayer of DOM spans over the canvas, so copy and find work.
  const renderBitmap = useCallback(
    async (
      pdf: DocLike,
      key: string,
      target: number,
      alive: () => boolean,
    ): Promise<{
      canvas: HTMLCanvasElement;
      textContent: PdfTextContent;
      textViewport: PdfViewport;
    } | null> => {
      if (renderedRef.current.has(key)) return null;
      const pg = await pdf.getPage(target);
      if (!alive()) return null;
      const shell = shellRefs.current.get(target);
      const cssWidth = shell ? shell.getBoundingClientRect().width : 820;
      // The backing matches the shell's laid-out width in device pixels; the
      // text layer gets its own CSS-px viewport (no DPR fold).
      const probe = pg.getViewport({ scale: 1 });
      const page = { width: probe.width, height: probe.height };
      const geo = renderGeometry(cssWidth, page, window.devicePixelRatio);
      const viewport = pg.getViewport({ scale: geo.scale });
      const textViewport = pg.getViewport({ scale: cssScaleFor(cssWidth, page) });
      const off = document.createElement('canvas');
      off.width = geo.width;
      off.height = geo.height;
      const ctx = off.getContext('2d');
      if (!ctx) throw new Error('2d context unavailable');
      const t1 = Date.now();
      await pg.render({ canvasContext: ctx, canvas: off, viewport }).promise;
      if (!alive()) return null;
      const textContent = await pg.getTextContent();
      if (!alive()) return null;
      const ms = Date.now() - t1;
      emit({
        scope: 'preview',
        kind: 'progress',
        actor: 'system',
        message: `page ${target} rendered in ${ms}ms`,
        event: {
          action: 'preview.page-render',
          page: target,
          ms,
          width: geo.width,
          height: geo.height,
        },
      });
      return { canvas: off, textContent, textViewport };
    },
    [renderedRef, shellRefs],
  );

  // Commit a rendered page into its shell: canvas paint + text layer.
  // drawImage commits pixels with no re-layout and no blank flash; the text
  // div is rebuilt by pdf.js (spans positioned from the CSS-px viewport, so
  // selection lands on glyphs). Records identity AFTER pixels land, so a
  // commit can never mark a page rendered that isn't. A null bitmap (already
  // rendered, or lost its liveness race) commits nothing.
  const commitBitmap = useCallback(
    (
      target: number,
      key: string,
      done: {
        canvas: HTMLCanvasElement;
        textContent: PdfTextContent;
        textViewport: PdfViewport;
      } | null,
    ) => {
      if (!done) return;
      const canvas = canvasRefs.current.get(target);
      const layer = textRefs.current.get(target);
      if (!canvas) return;
      const { canvas: off, textContent, textViewport } = done;
      if (canvas.width !== off.width || canvas.height !== off.height) {
        canvas.width = off.width;
        canvas.height = off.height;
      }
      const ctx = canvas.getContext('2d');
      if (!ctx) return;
      ctx.drawImage(off, 0, 0);
      const ctor = textLayerRef.current;
      if (layer && ctor) commitTextLayer(layer, ctor, textContent, textViewport);
      renderedRef.current.add(key);
      renderedKeys.current.set(target, key);
      // Rendered for a width the shell has since left: render again.
      const shell = shellRefs.current.get(target);
      if (
        shell &&
        backingWidth(shell.getBoundingClientRect().width, window.devicePixelRatio) !== off.width
      )
        recheckRef.current?.();
    },
    [canvasRefs, renderedKeys, renderedRef, shellRefs, textLayerRef],
  );

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
  }, [lockRef, onPageRef, pendingScrollRef, ratiosRef, setVisible, visibleRef]);

  const schedulePick = useCallback(() => {
    if (pickRafRef.current != null) return;
    pickRafRef.current = requestAnimationFrame(() => {
      pickRafRef.current = null;
      applyPick();
    });
  }, [applyPick]);

  const beginPhase = useCallback(() => {
    phaseCountRef.current++;
    setPhase('rendering...');
  }, [setPhase]);

  const endPhase = useCallback(() => {
    phaseCountRef.current = Math.max(0, phaseCountRef.current - 1);
    if (phaseCountRef.current === 0) setPhase('');
  }, [setPhase]);

  // Render the bitmap window: visible page immediately, neighbors idle.
  // The jump scroll fires once the target bitmap lands (not after the whole
  // window), so pager jumps stick without waiting on neighbors. Awaited work
  // never touches canvas identity: render pixels offscreen, then commit.
  // Liveness is per-page (inFlight token): a newer render of the same page
  // supersedes the old one; renders of other pages never cancel each other.
  useEffect(() => {
    const pdf = docRef.current?.pdf;
    if (!pdf || !docKey) return;
    const key = docKey;
    const target = visible;
    const total = numPages;
    const win = windowFor(target, total);
    const offsets: number[] = [0];
    const maxD = Math.max(WINDOW_ABOVE, WINDOW_BELOW);
    for (let d = 1; d <= maxD; d++) {
      if (d <= WINDOW_BELOW) offsets.push(d);
      if (d <= WINDOW_ABOVE) offsets.push(-d);
    }
    const ordered = offsets.map((o) => target + o).filter((n) => n >= win.lo && n <= win.hi);
    const inWindow = new Set(ordered);
    // Idle prefetch ring: one page beyond each window edge, same identity
    // maps, exempt from eviction.
    const preWin = windowFor(
      target,
      total,
      WINDOW_ABOVE + PREFETCH_BEHIND,
      WINDOW_BELOW + PREFETCH_AHEAD,
    );
    const prefetch = preWin.pages.filter((n) => !inWindow.has(n));
    // Claim one in-flight token PER page: a newer pass for the same page
    // supersedes the older one; other pages are unaffected.
    const myTokens = new Map<string, number>();
    for (const n of ordered) {
      const k = `${key}#${n}`;
      const t = (inFlightRef.current.get(k) ?? 0) + 1;
      inFlightRef.current.set(k, t);
      myTokens.set(k, t);
    }
    const alivePage = (k: string) =>
      docRef.current?.key === key && inFlightRef.current.get(k) === myTokens.get(k);
    beginPhase();
    void (async () => {
      try {
        // Render the target to an offscreen canvas FIRST: the commit below
        // swaps it in only if this pass still owns the page AND the page is
        // still the visible one (a fast scroll-away cancels the swap instead
        // of painting a stale bitmap over the new window).
        const pageKey = `${key}#${target}`;
        const nodes = await renderBitmap(pdf, pageKey, target, () => alivePage(pageKey));
        if (!alivePage(pageKey)) return;
        if (!inWindow.has(target) || visibleRef.current !== target) return;
        commitBitmap(target, pageKey, nodes);
        if (pendingScrollRef.current != null) scrollToShell(pendingScrollRef.current);
        for (const n of ordered.slice(1)) {
          const nk = `${key}#${n}`;
          if (!alivePage(nk)) continue;
          await idle();
          if (!alivePage(nk)) continue;
          const nb = await renderBitmap(pdf, nk, n, () => alivePage(nk));
          if (!alivePage(nk)) continue;
          // Same guard per neighbor: skip the commit when the window moved
          // on while this bitmap was rendering.
          if (visibleRef.current !== target) break;
          commitBitmap(n, nk, nb);
        }
        // Lowest priority, fully idle: fill the prefetch ring.
        for (const n of prefetch) {
          const nk = `${key}#${n}`;
          if (!alivePage(nk) || renderedRef.current.has(nk)) continue;
          await idle();
          if (!alivePage(nk) || visibleRef.current !== target) break;
          const nb = await renderBitmap(pdf, nk, n, () => alivePage(nk));
          if (!alivePage(nk) || visibleRef.current !== target) break;
          commitBitmap(n, nk, nb);
        }
        // Memory contract: shells stay for scroll height, but bitmaps
        // outside the window are freed (canvas backing cleared). Keyed by
        // the captured doc identity so a newer document's bitmaps (same
        // page numbers, different key) are never touched — and a page with
        // a NEWER in-flight token is never evicted (its pixels are coming).
        // Prefetch ring: idle only, exempt from eviction below
        // (at most PREFETCH_AHEAD+PREFETCH_BEHIND pages).
        const keepPrefetch = new Set(prefetch);
        for (const [n, canvas] of canvasRefs.current) {
          if (renderedKeys.current.get(n) !== key || inWindow.has(n) || keepPrefetch.has(n))
            continue;
          if (
            inFlightRef.current.get(`${key}#${n}`) !== myTokens.get(`${key}#${n}`) &&
            inFlightRef.current.has(`${key}#${n}`)
          )
            continue;
          renderedKeys.current.delete(n);
          renderedRef.current.delete(`${key}#${n}`);
          canvas.width = 0;
          canvas.height = 0;
        }
      } finally {
        endPhase();
      }
    })();
  }, [
    docKey,
    visible,
    numPages,
    renderEpoch,
    scrollToShell,
    beginPhase,
    endPhase,
    commitBitmap,
    renderBitmap,
    canvasRefs,
    docRef,
    inFlightRef,
    pendingScrollRef,
    renderedKeys,
    renderedRef,
    visibleRef,
  ]);

  // Visible pick: one observer over all shells, middle-band ratios, largest
  // wins (rAF-throttled). The callback itself is O(changed) — it only
  // records ratios; the O(window) pick runs at most once per frame.
  // Shells mount per document, so observe on section count (numPages), not
  // on doc identity: re-mounts re-fire this effect even when a recompile
  // yields the same page count.
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
      {
        root: box,
        threshold: Array.from({ length: 11 }, (_, i) => i / 10),
        rootMargin: '-40% 0px -40% 0px',
      },
    );
    // Shell refs populate during this commit's layout; the observer must see
    // them, so observe on the next frame (disconnect still cleans up).
    const raf = requestAnimationFrame(() => {
      for (const [, el] of shellRefs.current) observer.observe(el);
    });
    return () => {
      cancelAnimationFrame(raf);
      observer.disconnect();
    };
  }, [docKey, numPages, schedulePick, ratiosRef, scrollRef, shellRefs]);

  // Re-render when a page's backing no longer matches its shell: a zoom or
  // pane resize changed the shell width, or the DPR changed. Stale pages leave
  // the rendered set (the window pass re-renders them, eviction still frees
  // them); plain scroll never re-renders.
  useEffect(() => {
    if (!docKey) return;
    let timer: ReturnType<typeof setTimeout> | null = null;
    const recheck = () => {
      if (timer) clearTimeout(timer);
      timer = setTimeout(() => {
        timer = null;
        const painted: PaintedPage[] = [];
        for (const [n] of renderedKeys.current) {
          const shell = shellRefs.current.get(n);
          const canvas = canvasRefs.current.get(n);
          if (!shell || !canvas) continue;
          painted.push({
            page: n,
            cssWidth: shell.getBoundingClientRect().width,
            backing: canvas.width,
          });
        }
        const stale = stalePages(painted, window.devicePixelRatio);
        for (const n of stale) renderedRef.current.delete(renderedKeys.current.get(n) ?? '');
        if (stale.length > 0) setRenderEpoch((e) => e + 1);
      }, 100);
    };
    recheckRef.current = recheck;
    const ro = new ResizeObserver(recheck);
    const raf = requestAnimationFrame(() => {
      for (const [, el] of shellRefs.current) ro.observe(el);
    });
    window.addEventListener('resize', recheck);
    return () => {
      cancelAnimationFrame(raf);
      ro.disconnect();
      window.removeEventListener('resize', recheck);
      if (timer) clearTimeout(timer);
      if (recheckRef.current === recheck) recheckRef.current = null;
    };
  }, [docKey, numPages, canvasRefs, renderedKeys, renderedRef, shellRefs]);

  const setShellRef = useCallback(
    (n: number) => (el: HTMLDivElement | null) => {
      if (el) shellRefs.current.set(n, el);
      else shellRefs.current.delete(n);
    },
    [shellRefs],
  );

  const setCanvasRef = useCallback(
    (n: number) => (el: HTMLCanvasElement | null) => {
      if (el) canvasRefs.current.set(n, el);
      else canvasRefs.current.delete(n);
    },
    [canvasRefs],
  );

  const setTextRef = useCallback(
    (n: number) => (el: HTMLDivElement | null) => {
      if (el) textRefs.current.set(n, el);
      else textRefs.current.delete(n);
    },
    [],
  );

  return { setShellRef, setCanvasRef, setTextRef };
}
