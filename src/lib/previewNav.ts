// previewNav.ts — pure navigation math for the windowed PDF preview.
//
// The Preview body keeps one lightweight shell per document page (stable
// scroll height) and renders bitmaps only for a small window around the
// visible page. These helpers own the window shape, the visible-page pick,
// and scroll compensation. All pure and unit-tested; the component owns I/O.

/** Shells rendered above the anchor (neighbors kept as bitmaps). */
export const WINDOW_ABOVE = 1;
/** Shells rendered below the anchor (neighbors kept as bitmaps). */
export const WINDOW_BELOW = 2;

/** Clamp a page number into [1, numPages] (non-finite → 1). */
export function clampPage(n: number, numPages: number): number {
  if (!Number.isFinite(n)) return 1;
  const total = Math.max(1, Math.floor(numPages));
  return Math.min(total, Math.max(1, Math.floor(n)));
}

export interface PreviewWindow {
  lo: number;
  hi: number;
  pages: number[];
}

/**
 * Bitmap window around the anchor, clamped to the document.
 * Shells for ALL pages stay mounted (cheap); only these pages hold bitmaps.
 */
export function windowFor(
  anchor: number,
  numPages: number,
  above: number = WINDOW_ABOVE,
  below: number = WINDOW_BELOW,
): PreviewWindow {
  const total = Math.max(1, Math.floor(numPages));
  const a = clampPage(anchor, total);
  const lo = Math.max(1, a - Math.max(0, Math.floor(above)));
  const hi = Math.min(total, a + Math.max(0, Math.floor(below)));
  const pages: number[] = [];
  for (let n = lo; n <= hi; n++) pages.push(n);
  return { lo, hi, pages };
}

export interface VisibleEntry {
  page: number;
  ratio: number;
}

/**
 * Pick the visible page from IntersectionObserver entries (largest
 * intersectionRatio wins; ties go to the smallest page so page 1 is always
 * reachable). Returns null when nothing intersects (fast fling between
 * callbacks) so the caller keeps the current page instead of guessing.
 */
export function pickVisible(entries: VisibleEntry[]): number | null {
  if (entries.length === 0) return null;
  let best = entries[0];
  for (const e of entries.slice(1)) {
    if (e.ratio > best.ratio || (e.ratio === best.ratio && e.page < best.page)) {
      best = e;
    }
  }
  if (!(best.ratio > 0)) return null;
  return best.page;
}

/**
 * Keep the viewport stable when content mounts above it: the container must
 * move down by exactly the added height. Negative growth never pulls up.
 */
export function compensateScrollTop(currentTop: number, addedAbove: number): number {
  return currentTop + Math.max(0, addedAbove);
}
