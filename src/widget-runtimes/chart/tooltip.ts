// The chart's hover tooltip, given to Vega as `view.tooltip(handler)`: one
// persistent element (`#tip`, role=tooltip) beside the chart host, since a
// redraw empties the host. It is filled with text nodes only (the values are
// the widget's data), positioned fixed beside the pointer and kept on
// screen, and hidden when the pointer leaves a mark, on a redraw and on a
// theme change. The pure parts are exported for tests.

/** The most fields shown, and the longest key and value, in characters. */
export const MAX_ROWS = 12;
export const MAX_KEY = 40;
export const MAX_VALUE = 120;

export interface TipContent {
  title: string | null;
  rows: [string, string][];
}

function clip(s: string, n: number): string {
  return s.length > n ? `${s.slice(0, n - 1)}…` : s;
}

function text(v: unknown): string | null {
  if (typeof v === 'string') return v;
  if (typeof v === 'number') return Number.isFinite(v) ? String(v) : null;
  if (typeof v === 'boolean') return String(v);
  if (v instanceof Date) return v.toISOString();
  return null;
}

/**
 * What a Vega tooltip value shows: nothing for null, undefined or an empty
 * string; a lone value as the title; an object's fields as rows (its
 * `title` field, if any, as the title). Rows past MAX_ROWS and fields
 * that are not plain values are left out; long text is cut.
 */
export function tipContent(value: unknown): TipContent | null {
  if (value === null || value === undefined || value === '') return null;
  const lone = text(value);
  if (lone !== null) return { title: clip(lone, MAX_VALUE), rows: [] };
  if (typeof value !== 'object' || Array.isArray(value)) return null;
  let title: string | null = null;
  const rows: [string, string][] = [];
  for (const [k, v] of Object.entries(value as Record<string, unknown>)) {
    const t = text(v);
    if (t === null) continue;
    if (k === 'title' && title === null) {
      title = clip(t, MAX_VALUE);
      continue;
    }
    if (rows.length >= MAX_ROWS) break;
    rows.push([clip(k, MAX_KEY), clip(t, MAX_VALUE)]);
  }
  return title === null && rows.length === 0 ? null : { title, rows };
}

/**
 * Where a `w`x`h` tooltip goes for a pointer at (`x`, `y`) in a `vw`x`vh`
 * viewport: below right of the pointer, flipped to the other side when it
 * would run off, and clamped `margin` inside the viewport.
 */
export function placeTip(
  x: number,
  y: number,
  w: number,
  h: number,
  vw: number,
  vh: number,
  gap = 12,
  margin = 4,
): { left: number; top: number } {
  let left = x + gap;
  let top = y + gap;
  if (left + w > vw - margin) left = x - gap - w;
  if (top + h > vh - margin) top = y - gap - h;
  const clamp = (v: number, size: number, max: number) =>
    Math.max(margin, Math.min(v, max - margin - size));
  return { left: clamp(left, w, vw), top: clamp(top, h, vh) };
}

export interface Tip {
  /** The handler `view.tooltip()` takes. */
  handler(handler: unknown, event: MouseEvent, item: unknown, value: unknown): void;
  hide(): void;
}

/** Drives the tooltip element `el`. */
export function makeTip(el: HTMLElement): Tip {
  const hide = () => {
    el.hidden = true;
    el.textContent = '';
  };
  const show = (event: MouseEvent, c: TipContent) => {
    el.textContent = '';
    if (c.title !== null) {
      const t = document.createElement('div');
      t.className = 'tip-title';
      t.textContent = c.title;
      el.append(t);
    }
    for (const [k, v] of c.rows) {
      const row = document.createElement('div');
      row.className = 'tip-row';
      const key = document.createElement('span');
      key.className = 'tip-key';
      key.textContent = k;
      const val = document.createElement('span');
      val.className = 'tip-value';
      val.textContent = v;
      row.append(key, val);
      el.append(row);
    }
    el.hidden = false;
    const r = el.getBoundingClientRect();
    const p = placeTip(
      event.clientX,
      event.clientY,
      r.width,
      r.height,
      window.innerWidth,
      window.innerHeight,
    );
    el.style.left = `${p.left}px`;
    el.style.top = `${p.top}px`;
  };
  return {
    handler(_h, event, _item, value) {
      const c = tipContent(value);
      if (!c || !event) hide();
      else show(event, c);
    },
    hide,
  };
}
