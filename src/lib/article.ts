// article.ts — view logic for the in-app Article tab.
//
// The article renders its bundle bytes in a sandboxed frame: opaque origin,
// scripts only, no Tauri bridge inside. The main window already runs with
// speculative loading off, which the frame inherits; there is no second
// window and so no capability set to grant. The bytes keep the reader's own
// CSP meta intact.
//
// Editor-to-article anchor sync, inverse sync, and the approve round-trip
// arrive later. For now every frame message is dropped after the denial
// check below, including well-formed ones.

/** The only sandbox the article frame ever gets (mirrors the reader probes). */
export const ARTICLE_SANDBOX = 'allow-scripts';

/** The sandbox attribute value for the article frame, exactly. */
export function articleSandbox(): string {
  return ARTICLE_SANDBOX;
}

/**
 * The reader CSP carried in the bytes, or null when the bytes carry none.
 * A null means the bundle did not come from the exporter intact.
 */
export function articleCspOf(html: string): string | null {
  const m = /<meta http-equiv="Content-Security-Policy" content="([^"]*)">/.exec(html);
  return m ? m[1] : null;
}

/** Bridge tokens that must never appear in article bytes. */
const TAURI_TOKENS = [
  '__TAURI_INTERNALS__',
  '__TAURI_METADATA__',
  '__TAURI__',
  'tauri://',
  'ipc.localhost',
];

/**
 * Whether the bytes expose the desktop bridge. The frame is opaque-origin
 * and capability-free, so any such token is a leak, not a feature.
 */
export function hasTauriInternals(html: string): boolean {
  return TAURI_TOKENS.some((t) => html.includes(t));
}

export interface ArticleMessage {
  widgetId: string;
}

/**
 * Whether a window message is the article frame's approval ask. Drops
 * everything hostile: a foreign source, a non-null origin (the frame is
 * opaque-origin, so its messages arrive with origin "null"), or a body
 * that is not exactly `{ mfw: 1, type: 'approve-widget', widgetId }`.
 * The approve handling itself arrives later; this slice drops even the
 * well-formed ones.
 */
export function isArticleMessage(
  data: unknown,
  origin: string,
  source: unknown,
  frame: unknown,
): data is ArticleMessage {
  if (source == null || source !== frame) return false;
  if (origin !== 'null') return false;
  if (typeof data !== 'object' || data === null) return false;
  const d = data as Record<string, unknown>;
  return (
    d['mfw'] === 1 &&
    d['type'] === 'approve-widget' &&
    typeof d['widgetId'] === 'string' &&
    (d['widgetId'] as string).length > 0
  );
}

/**
 * Whether the Article tab can show anything: bytes loaded, and no compile
 * running (the bundle would describe the previous document).
 */
export function articleAvailable(html: string | null, compiling: boolean): boolean {
  return html != null && html !== '' && !compiling;
}

/** Zoom bounds for the article frame, in percent. */
export const ARTICLE_ZOOM_MIN = 50;
export const ARTICLE_ZOOM_MAX = 200;
const ARTICLE_ZOOM_STEP = 25;

/** Clamp a zoom percent into the article range. */
export function clampArticleZoom(percent: number): number {
  if (!Number.isFinite(percent)) return 100;
  return Math.min(ARTICLE_ZOOM_MAX, Math.max(ARTICLE_ZOOM_MIN, Math.round(percent)));
}

/** Step the zoom up or down one notch, clamped. */
export function stepArticleZoom(percent: number, dir: 'in' | 'out'): number {
  return clampArticleZoom(percent + (dir === 'in' ? ARTICLE_ZOOM_STEP : -ARTICLE_ZOOM_STEP));
}

/** The zoom label the toolbar shows. */
export function articleZoomLabel(percent: number): string {
  return `${clampArticleZoom(percent)}%`;
}
