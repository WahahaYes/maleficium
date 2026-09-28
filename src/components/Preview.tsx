// Preview continuous-scroll body — all shells mounted, windowed bitmaps.
//
// Every page owns a lightweight shell div sized from its probed aspect, so
// the scroll height is right from the start; only a small window of pages
// around the visible one holds pdf.js pixels. Pager and SyncTeX jumps scroll
// once the target page has rendered. Per-event work stays O(visible); at very
// large page counts the one-div-per-page DOM is the remaining cost.
//
// This body owns the state the three concerns share — document identity,
// the bitmap identity maps, the visible page, the jump lock — and threads it
// into usePdfDocument (open + probe), useBitmapWindow (pixels + eviction)
// and useSyncLock (jumps). Anything touched by one concern alone lives in
// that hook.

import { Box, Typography, useTheme } from '@mui/material';
import { useCallback, useLayoutEffect, useRef, useState } from 'react';
import type { RefObject } from 'react';
import { DEVICE_PREF_KEYS, store } from '../lib/app-store';
import {
  applyZoom,
  pageWidth,
  parseZoom,
  percentOf,
  zoomLabel,
  type Size,
  type ZoomAction,
  type ZoomMode,
} from '../lib/zoom';
import PreviewToolbar from './PreviewToolbar';
import { clampPage } from '../lib/previewNav';
import { usePdfDocument } from '../hooks/usePdfDocument';
import type { DocLike, TextLayerCtor } from '../hooks/usePdfDocument';
import { useBitmapWindow } from '../hooks/useBitmapWindow';
import { useExternalRefresh } from '../hooks/useExternalRefresh';
import type { PreviewSource } from '../lib/preview-bus';
import { useSyncLock } from '../hooks/useSyncLock';

export interface PreviewProps {
  pdfUrl: string | null;
  stamp: number;
  pageNumber?: number;
  onPage?: (p: number) => void;
  onSync?: () => void;
  /** Inverse SyncTeX: canvas click → editor line (disabled while compiling). */
  onInverse?: (page: number, x: number, y: number) => void;
  syncDisabled?: boolean;
  /** Filled with the zoom dispatcher, for menu commands and chords. */
  zoomActionRef?: RefObject<((a: ZoomAction) => void) | null>;
  /** The zoom changed: the mode and the percent a page now shows at. */
  onZoom?: (mode: ZoomMode, percent: number) => void;
  /** The open project's main file: watched for an outside compile while no pdf is shown. */
  mainSource?: PreviewSource | null;
}

/** Page size (PDF points) assumed before a page is probed: A4. */
const UNPROBED: Size = { width: 595, height: 842 };

function loadZoom(): ZoomMode {
  try {
    return parseZoom(store().get(DEVICE_PREF_KEYS.previewZoom));
  } catch {
    // No app store configured (tests, storage off): the default zoom.
    return parseZoom(null);
  }
}

export default function Preview({
  pdfUrl,
  stamp,
  pageNumber = 1,
  onPage,
  onSync,
  onInverse,
  syncDisabled,
  zoomActionRef,
  onZoom,
  mainSource = null,
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
  const pageFilterCss = useTheme().preview.pageFilter;
  useExternalRefresh(mainSource);

  // Zoom: the mode is a device pref; the pane size drives the fit modes.
  const [zoom, setZoom] = useState<ZoomMode>(loadZoom);
  const [pane, setPane] = useState<Size>({ width: 836, height: 600 });
  const anchorRef = useRef<{ page: number; frac: number } | null>(null);
  // Measured before paint, so the first layout already uses the real pane.
  useLayoutEffect(() => {
    const el = scrollRef.current;
    if (!el) return;
    const measure = () => setPane({ width: el.clientWidth, height: el.clientHeight });
    measure();
    const ro = new ResizeObserver(measure);
    ro.observe(el);
    return () => ro.disconnect();
  }, [pdfUrl]);
  const pageSize = (n: number): Size => {
    const d = dimsRef.current.get(n);
    return d ? { width: d.w, height: d.h } : UNPROBED;
  };
  const shownPercent = percentOf(pageWidth(zoom, pane, pageSize(visible)), pageSize(visible));
  const setZoomMode = (next: ZoomMode) => {
    // Keep the visible page, at the same fraction of its height, in place.
    const scroll = scrollRef.current;
    const shell = shellRefs.current.get(visibleRef.current);
    if (scroll && shell) {
      const top = shell.getBoundingClientRect().top - scroll.getBoundingClientRect().top;
      anchorRef.current = {
        page: visibleRef.current,
        frac: -top / Math.max(1, shell.getBoundingClientRect().height),
      };
    }
    setZoom(next);
    try {
      store().set(DEVICE_PREF_KEYS.previewZoom, JSON.stringify(next));
    } catch {
      // No app store configured: the zoom lasts for this session only.
    }
    const size = pageSize(visibleRef.current);
    onZoom?.(next, percentOf(pageWidth(next, pane, size), size));
  };
  const zoomBy = (a: ZoomAction) => setZoomMode(applyZoom(a, shownPercent));
  if (zoomActionRef) zoomActionRef.current = zoomBy;
  useLayoutEffect(() => {
    const a = anchorRef.current;
    const scroll = scrollRef.current;
    const shell = a ? shellRefs.current.get(a.page) : undefined;
    anchorRef.current = null;
    if (!a || !scroll || !shell) return;
    const top = shell.getBoundingClientRect().top - scroll.getBoundingClientRect().top;
    scroll.scrollTop += top + a.frac * shell.getBoundingClientRect().height;
  }, [zoom]);

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
  // Its width is the zoom's; bitmaps re-render at the new layout width, so a
  // zoom never stretches pixels. Auto margins center a narrow page and let a
  // wide one scroll sideways instead of clipping.
  const shellStyle = (n: number): React.CSSProperties => {
    const size = pageSize(n);
    return {
      position: 'relative',
      width: pageWidth(zoom, pane, size),
      marginInline: 'auto',
      flexShrink: 0,
      aspectRatio: `${size.width} / ${size.height}`,
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
        zoomLabel={zoomLabel(zoom, shownPercent)}
        onZoom={zoomBy}
        onZoomMode={setZoomMode}
      />
      {/* The phase line floats over the pages: it never changes the pane size. */}
      <Box sx={{ position: 'relative', flex: 1, minHeight: 0, display: 'flex' }}>
        {phase ? (
          <Typography
            variant="caption"
            sx={{
              position: 'absolute',
              top: 4,
              right: 16,
              zIndex: 1,
              px: 0.75,
              borderRadius: 1,
              pointerEvents: 'none',
              bgcolor: 'background.paper',
              opacity: 0.85,
            }}
          >
            {phase}
          </Typography>
        ) : null}
        <Box
          ref={scrollRef}
          onScroll={handleScroll}
          sx={{
            flex: 1,
            overflowY: 'auto',
            overflowX: 'auto',
            minHeight: 0,
            display: 'flex',
            flexDirection: 'column',
            alignItems: 'flex-start',
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
                style={{
                  position: 'absolute',
                  inset: 0,
                  width: '100%',
                  height: '100%',
                  filter: pageFilterCss,
                }}
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
    </Box>
  );
}
