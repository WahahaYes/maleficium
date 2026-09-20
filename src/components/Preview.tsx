// Preview continuous-scroll body — all shells mounted, windowed bitmaps.
//
// Scroll contract: every document page owns a lightweight shell div whose
// height is reserved from probed aspect ratios, but only a small bitmap
// WINDOW around the visible page holds pdf.js pixels (bounded memory —
// shells that leave the window have their canvas backing cleared). The
// visible page is picked by IntersectionObserver (largest ratio inside the
// viewport middle band), free in BOTH directions, so page 1 stays reachable
// by scroll AND by pager. The pager only drives jumps: a prop change that
// differs from the visible page scrolls AFTER the target bitmap lands;
// neighbors then render idle. Programmatic scrolls carry a flag so a user
// grab mid-jump cancels it. devicePixelRatio capped at 2.
// Shell count never scales with the document (buffers + observers do): one
// shell div per page is O(pages) DOM by design, and O(visible) work per
// event holds because the observer callback only records ratios while the
// rAF-throttled pick + idle-scheduled neighbors defer the rest.
// At very large page counts the DOM itself is the cost (not the bitmaps —
// those stay O(window)).
//
// This body owns the state the three concerns share — document identity,
// the bitmap identity maps, the visible page, the jump lock — and threads it
// into usePdfDocument (open + probe), useBitmapWindow (pixels + eviction)
// and useSyncLock (jumps). Anything touched by one concern alone lives in
// that hook.

import { Box, Typography } from '@mui/material';
import { useCallback, useRef, useState } from 'react';
import PreviewToolbar from './PreviewToolbar';
import { clampPage } from '../lib/previewNav';
import { usePdfDocument } from '../hooks/usePdfDocument';
import type { DocLike, TextLayerCtor } from '../hooks/usePdfDocument';
import { useBitmapWindow } from '../hooks/useBitmapWindow';
import { useSyncLock } from '../hooks/useSyncLock';

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

export default function Preview({
  pdfUrl,
  stamp,
  pageNumber = 1,
  onPage,
  onSync,
  onInverse,
  syncDisabled,
}: PreviewProps) {
  const scrollRef = useRef<HTMLDivElement>(null);
  const shellRefs = useRef(new Map<number, HTMLDivElement>());
  const canvasRefs = useRef(new Map<number, HTMLCanvasElement>());
  // Bitmap identity is per document revision AND page: cleared on every doc
  // change so entries can never accumulate across recompiles (bounded by the
  // window). Keyed (page -> docKey) so the eviction pass can never clear a
  // NEWER document's bitmaps when a stale generation finishes late.
  const renderedRef = useRef(new Set<string>());
  const renderedKeys = useRef(new Map<number, string>());
  // One in-flight token per document revision + page: a newer render of the
  // same page supersedes the older one, and exempts it from eviction.
  const inFlightRef = useRef(new Map<string, number>());
  const dimsRef = useRef(new Map<number, { w: number; h: number }>());
  const [numPages, setNumPages] = useState(1);
  const [phase, setPhase] = useState('');
  const [docKey, setDocKey] = useState('');
  const [visible, setVisible] = useState(1);
  const [dimsVersion, setDimsVersion] = useState(0);
  const page = clampPage(pageNumber, numPages);
  // Open pdf.js document ONCE per pdfUrl+stamp: renders serve from the
  // cached handle instead of re-opening the whole document per page.
  const docRef = useRef<{ key: string; pdf: DocLike } | null>(null);
  const visibleRef = useRef(1);
  const pageRef = useRef(page);
  pageRef.current = page;
  const onPageRef = useRef(onPage);
  onPageRef.current = onPage;
  const ratiosRef = useRef(new Map<number, number>());
  const lockRef = useRef<{ target: number; until: number } | null>(null);
  const pendingScrollRef = useRef<number | null>(null);
  const compRef = useRef<{ page: number; delta: number } | null>(null);
  // TextLayer constructor resolved per document open. Instance-scoped, not
  // module state.
  const textLayerRef = useRef<TextLayerCtor | null>(null);

  const bumpDims = useCallback(() => setDimsVersion((v) => v + 1), []);

  const { scrollToShell, armJumpTimeout, handleScroll } = useSyncLock({
    page,
    docKey,
    dimsVersion,
    scrollRef,
    shellRefs,
    lockRef,
    pendingScrollRef,
    compRef,
    visibleRef,
    setVisible,
  });

  const { error } = usePdfDocument({
    pdfUrl,
    stamp,
    scrollRef,
    shellRefs,
    docRef,
    textLayerRef,
    dimsRef,
    compRef,
    renderedRef,
    renderedKeys,
    inFlightRef,
    ratiosRef,
    visibleRef,
    pageRef,
    pendingScrollRef,
    lockRef,
    setVisible,
    setPhase,
    setNumPages,
    setDocKey,
    bumpDims,
    armJumpTimeout,
  });

  const { setShellRef, setCanvasRef, setTextRef } = useBitmapWindow({
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
  });

  const handleShellClick = (n: number) => (e: React.MouseEvent<HTMLDivElement>) => {
    if (syncDisabled || !onInverse) return;
    // Text selection wins over navigation: a drag-select ending here leaves
    // a non-collapsed range; only a plain (collapsed) click syncs.
    const sel = window.getSelection();
    if (sel && !sel.isCollapsed) return;
    const canvas = canvasRefs.current.get(n);
    const rect = (canvas ?? e.currentTarget).getBoundingClientRect();
    // SyncTeX y grows down from the page top. Map through the scale-1
    // probe dims (true page points), never the DPR-folded render viewport.
    // Falls back to canvas backing size.
    const dims = dimsRef.current.get(n);
    const cssX = e.clientX - rect.left;
    const cssY = e.clientY - rect.top;
    let x: number;
    let y: number;
    if (dims) {
      x = (cssX / Math.max(1, rect.width)) * dims.w;
      y = (cssY / Math.max(1, rect.height)) * dims.h;
    } else {
      const scaleX = (canvas?.width ?? rect.width) / Math.max(1, rect.width);
      const scaleY = (canvas?.height ?? rect.height) / Math.max(1, rect.height);
      x = cssX * scaleX;
      y = cssY * scaleY;
    }
    // Hit feedback without remount: toggle a class, remove after 300ms.
    if (canvas) {
      canvas.classList.remove('synctex-hit');
      // Force reflow so rapid clicks retrigger the outline.
      void canvas.offsetWidth;
      canvas.classList.add('synctex-hit');
      setTimeout(() => canvas.classList.remove('synctex-hit'), 300);
    }
    onInverse(n, Math.round(x), Math.round(y));
  };

  // Every page owns a shell (stable scroll height from the probed aspect);
  // the canvas fills it, so unrendered pages read as placeholders, never void.
  // Sized from the probed aspect where known: width-flexible (max 820) with
  // height from aspect-ratio, so narrow-pane zoom is CSS-only and DPR changes
  // are the sole re-render trigger.
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

  if (error)
    return (
      <Typography variant="body1" color="error">
        {error}
      </Typography>
    );

  return (
    <Box
      sx={{
        display: 'flex',
        flexDirection: 'column',
        height: '100%',
        minHeight: 0,
        overflow: 'hidden',
      }}
    >
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
        sx={{
          flex: 1,
          overflowY: 'auto',
          minHeight: 0,
          display: 'flex',
          flexDirection: 'column',
          alignItems: 'center',
          gap: 1,
          py: 1,
        }}
      >
        {allPages.map((n) => (
          <div
            key={`${docKey}#${n}`}
            ref={setShellRef(n)}
            data-page={n}
            style={shellStyle(n)}
            onClick={handleShellClick(n)}
            title={
              syncDisabled ? 'SyncTeX unavailable while compiling' : 'Click for inverse SyncTeX'
            }
          >
            <canvas
              ref={setCanvasRef(n)}
              data-page={n}
              className="synctex-canvas"
              style={{ position: 'absolute', inset: 0, width: '100%', height: '100%' }}
            />
            <div
              ref={setTextRef(n)}
              className="textLayer"
              data-page={n}
              style={{
                position: 'absolute',
                inset: 0,
                overflow: 'hidden',
                pointerEvents: 'none',
              }}
            />
          </div>
        ))}
      </Box>
    </Box>
  );
}
