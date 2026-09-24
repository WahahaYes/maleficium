// zoom.ts — preview zoom: the page width for a zoom mode, and stepping.
//
// Pure. 100% shows a PDF point at 96/72 CSS px (the size a page prints at).
// Fit modes follow the pane; percent modes follow the page's own width.

export type ZoomMode =
  { kind: 'fit-width' } | { kind: 'fit-page' } | { kind: 'percent'; percent: number };

export type ZoomAction = 'in' | 'out' | 'fit-width' | 'fit-page';

export const ZOOM_MIN = 25;
export const ZOOM_MAX = 400;
export const ZOOM_STEP = 10;
/** Pane padding kept clear around a page in fit modes (CSS px). */
export const ZOOM_GUTTER = 16;
const PX_PER_PT = 96 / 72;

export const DEFAULT_ZOOM: ZoomMode = { kind: 'fit-width' };

export type Size = { width: number; height: number };

/** CSS width of one page shell. `page` is the page size in PDF points. */
export function pageWidth(mode: ZoomMode, pane: Size, page: Size): number {
  const w = Math.max(1, page.width);
  const h = Math.max(1, page.height);
  const fitW = Math.max(1, pane.width - ZOOM_GUTTER);
  switch (mode.kind) {
    case 'fit-width':
      return fitW;
    case 'fit-page':
      return Math.max(1, Math.min(fitW, (pane.height - ZOOM_GUTTER) * (w / h)));
    case 'percent':
      return (w * PX_PER_PT * clampPercent(mode.percent)) / 100;
  }
}

/** Device pixels per CSS px a page canvas is backed at, at most. */
export const MAX_RENDER_DPR = 2;

/** Whole backing pixels across a page shown `cssWidth` CSS px wide. */
export function backingWidth(cssWidth: number, dpr: number): number {
  const d = Number.isFinite(dpr) && dpr > 0 ? Math.min(MAX_RENDER_DPR, dpr) : 1;
  return Math.max(1, Math.round(Math.max(1, cssWidth) * d));
}

/** A painted page: its shell's CSS width and its canvas width in device px. */
export type PaintedPage = { page: number; cssWidth: number; backing: number };

/** Pages whose canvas no longer matches their shell at this DPR; others keep their pixels. */
export function stalePages(painted: PaintedPage[], dpr: number): number[] {
  return painted.filter((p) => backingWidth(p.cssWidth, dpr) !== p.backing).map((p) => p.page);
}

export type RenderGeometry = { scale: number; width: number; height: number };

/**
 * Canvas backing size for a page shown `cssWidth` CSS px wide, and the pdf.js
 * scale (backing px per PDF point) that fills it edge to edge.
 */
export function renderGeometry(cssWidth: number, page: Size, dpr: number): RenderGeometry {
  const width = backingWidth(cssWidth, dpr);
  const scale = width / Math.max(1, page.width);
  return { scale, width, height: Math.max(1, Math.round(Math.max(1, page.height) * scale)) };
}

/**
 * CSS px per PDF point for a page shown `cssWidth` wide: the text-layer
 * viewport scale (the canvas backing folds DPR on top of this).
 */
export function cssScaleFor(cssWidth: number, page: Size): number {
  return Math.max(1, cssWidth) / Math.max(1, page.width);
}

/** The percent a page shown `width` CSS px wide is at. */
export function percentOf(width: number, page: Size): number {
  return (width / (Math.max(1, page.width) * PX_PER_PT)) * 100;
}

export function clampPercent(p: number): number {
  if (!Number.isFinite(p)) return 100;
  return Math.min(ZOOM_MAX, Math.max(ZOOM_MIN, p));
}

/**
 * Apply an action. Stepping starts from the percent currently shown (so a
 * fit mode steps from where it is) and lands on the next multiple of the
 * step in that direction.
 */
export function applyZoom(action: ZoomAction, shownPercent: number): ZoomMode {
  if (action === 'fit-width') return { kind: 'fit-width' };
  if (action === 'fit-page') return { kind: 'fit-page' };
  const p = clampPercent(shownPercent);
  const next =
    action === 'in'
      ? Math.floor(p / ZOOM_STEP + 1e-9) * ZOOM_STEP + ZOOM_STEP
      : Math.ceil(p / ZOOM_STEP - 1e-9) * ZOOM_STEP - ZOOM_STEP;
  return { kind: 'percent', percent: clampPercent(next) };
}

export function zoomLabel(mode: ZoomMode, shownPercent: number): string {
  const pct = `${Math.round(shownPercent)}%`;
  if (mode.kind === 'fit-width') return `Fit width (${pct})`;
  if (mode.kind === 'fit-page') return `Fit page (${pct})`;
  return pct;
}

/** A stored zoom mode, or the default when absent or malformed. */
export function parseZoom(raw: string | null): ZoomMode {
  if (!raw) return DEFAULT_ZOOM;
  try {
    const v = JSON.parse(raw) as Partial<ZoomMode> & { percent?: unknown };
    if (v.kind === 'fit-width' || v.kind === 'fit-page') return { kind: v.kind };
    if (v.kind === 'percent' && typeof v.percent === 'number') {
      return { kind: 'percent', percent: clampPercent(v.percent) };
    }
  } catch {
    // Unparseable pref: the default stands.
  }
  return DEFAULT_ZOOM;
}
