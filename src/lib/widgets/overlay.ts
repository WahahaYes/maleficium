// overlay.ts — where a widget sits over a rendered PDF page. Pure and
// host-neutral: the app preview (native widget views) and the reader
// (sandboxed iframes) both map rects through this module.
//
// Rects arrive in PDF user units (points, origin bottom-left); a page is
// shown as a box `cssWidth` CSS px wide whose aspect is the page's.

/** A widget's rect in PDF points, `x0 < x1`, `y0 < y1`, origin bottom-left. */
export interface PdfRect {
  x0: number;
  y0: number;
  x1: number;
  y1: number;
}

/** A page size in PDF points. */
export interface PageSize {
  width: number;
  height: number;
}

/** A box in CSS px, origin top-left. */
export interface CssBox {
  left: number;
  top: number;
  width: number;
  height: number;
}

/** A rect in integer device px of the host window (the wire `Rect`). */
export interface DeviceRect {
  x: number;
  y: number;
  w: number;
  h: number;
}

/** Where a live widget shows: its whole slot and the visible part (the wire `Placement`). */
export interface Placement {
  slot: DeviceRect;
  clip: DeviceRect;
  visible: boolean;
}

/** The widget's box inside its page box, in CSS px from the page's top-left corner. */
export function slotInPage(rect: PdfRect, page: PageSize, cssWidth: number): CssBox {
  const s = Math.max(1, cssWidth) / Math.max(1, page.width);
  return {
    left: rect.x0 * s,
    top: (page.height - rect.y1) * s,
    width: (rect.x1 - rect.x0) * s,
    height: (rect.y1 - rect.y0) * s,
  };
}

/** The PDF rect a CSS box in the page covers: the inverse of `slotInPage`. */
export function rectOfSlot(box: CssBox, page: PageSize, cssWidth: number): PdfRect {
  const s = Math.max(1, cssWidth) / Math.max(1, page.width);
  return {
    x0: box.left / s,
    y1: page.height - box.top / s,
    x1: (box.left + box.width) / s,
    y0: page.height - (box.top + box.height) / s,
  };
}

/** A CSS box in whole device px at `dpr`: each edge rounds to the nearest pixel. */
export function toDevice(box: CssBox, dpr: number): DeviceRect {
  const d = Number.isFinite(dpr) && dpr > 0 ? dpr : 1;
  const x0 = Math.round(box.left * d);
  const y0 = Math.round(box.top * d);
  const x1 = Math.round((box.left + box.width) * d);
  const y1 = Math.round((box.top + box.height) * d);
  return { x: x0, y: y0, w: Math.max(0, x1 - x0), h: Math.max(0, y1 - y0) };
}

export function intersect(a: DeviceRect, b: DeviceRect): DeviceRect {
  const x0 = Math.max(a.x, b.x);
  const y0 = Math.max(a.y, b.y);
  const x1 = Math.min(a.x + a.w, b.x + b.w);
  const y1 = Math.min(a.y + a.h, b.y + b.h);
  return { x: x0, y: y0, w: Math.max(0, x1 - x0), h: Math.max(0, y1 - y0) };
}

/**
 * The placement of a slot whose viewport box is `slot`, clipped to the pane
 * (`pane`, a viewport box). Both are CSS px relative to the window's
 * viewport; `dpr` maps them to the host window's device px.
 */
export function placement(slot: CssBox, pane: CssBox, dpr: number): Placement {
  const s = toDevice(slot, dpr);
  const clip = intersect(s, toDevice(pane, dpr));
  return { slot: s, clip, visible: clip.w > 0 && clip.h > 0 };
}

/** Whether a slot is within `margin` CSS px of the pane: live widgets start a little early. */
export function nearPane(slot: CssBox, pane: CssBox, margin: number): boolean {
  return (
    slot.left + slot.width > pane.left - margin &&
    slot.left < pane.left + pane.width + margin &&
    slot.top + slot.height > pane.top - margin &&
    slot.top < pane.top + pane.height + margin
  );
}

/** Distance from the pane's centre line to the slot's, for ranking in-view widgets. */
export function centreDistance(slot: CssBox, pane: CssBox): number {
  return Math.abs(slot.top + slot.height / 2 - (pane.top + pane.height / 2));
}
