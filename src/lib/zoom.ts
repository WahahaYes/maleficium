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
