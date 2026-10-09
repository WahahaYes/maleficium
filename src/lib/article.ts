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
//
// Editor-to-article sync is forward-only: the caret's section maps to an
// anchor (below) and the frame scrolls there. There is no inverse sync:
// the article never reports back (no tex-line granularity either).

/** One editor section that can own the caret, with its buffer line. */
export interface SectionLine {
  line: number;
  title: string;
}

/** Collapse whitespace so latexml numbers compare against plain titles. */
function flatTitle(s: string): string {
  return s.split(/\s+/).join(' ').trim();
}

/**
 * Whether an anchor's heading names an editor section: exact, or the
 * anchor's latexml number prefix plus the title ("1 First" owns "First").
 */
export function anchorMatchesSection(anchorText: string, title: string): boolean {
  const a = flatTitle(anchorText);
  const t = flatTitle(title);
  if (!a || !t) return false;
  return a === t || a.endsWith(' ' + t);
}

/**
 * The anchor the caret's line belongs to: the last section at or above the
 * line, walked back until one names an anchor (a deeper subsection the
 * article flattened away falls back to its parent). Occurrence-aware: the
 * Nth same-titled section takes the Nth same-titled anchor. Null when
 * nothing matches (no anchors, no sections, or titles the article lacks).
 */
export function anchorForLine(
  line: number,
  sections: readonly SectionLine[],
  anchors: readonly { id: string; text: string }[],
): string | null {
  if (!anchors.length || !sections.length) return null;
  let at = -1;
  for (let k = 0; k < sections.length; k++) {
    if (sections[k].line <= line) at = k;
    else break;
  }
  if (at < 0) at = 0;
  for (let j = at; j >= 0; j--) {
    const title = flatTitle(sections[j].title);
    if (!title) continue;
    let occ = 0;
    for (let k = 0; k <= j; k++) {
      if (flatTitle(sections[k].title) === title) occ++;
    }
    let seen = 0;
    for (const a of anchors) {
      if (!anchorMatchesSection(a.text, title)) continue;
      seen++;
      if (seen === occ) return a.id;
    }
  }
  return null;
}

/** The parent-to-frame scroll the bytes answer (reader.js scrolls + flashes). */
export function articleScrollMessage(id: string): { mfw: 1; type: 'article-scroll'; id: string } {
  return { mfw: 1, type: 'article-scroll', id };
}

/**
 * Whether a new compile stamp should reload the article: bytes shown, a
 * newer output stamp, no compile running, and something to load from.
 * Never loads on its own: with no bytes shown the first visit stays manual.
 */
export function shouldRefreshArticle(
  html: string | null,
  loadedStamp: number,
  docStamp: number | null,
  compiling: boolean,
  canLoad: boolean,
): boolean {
  return (
    html != null &&
    html !== '' &&
    canLoad &&
    !compiling &&
    docStamp != null &&
    docStamp > loadedStamp
  );
}

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

/** The Article tab's colour mode: the app's own (system), light or dark. */
export type ArticleMode = 'system' | 'light' | 'dark';

const ARTICLE_MODE_KEY = 'maleficium.articleMode';

/** The light or dark the article shows: `system` follows the app's theme. */
export function resolveArticleMode(choice: ArticleMode, appDark: boolean): 'light' | 'dark' {
  if (choice === 'system') return appDark ? 'dark' : 'light';
  return choice;
}

/** The message that sets the frame's mode (its own toggle then hides). */
export function articleModeMessage(mode: 'light' | 'dark'): {
  mfw: 1;
  type: 'article-mode';
  mode: 'light' | 'dark';
} {
  return { mfw: 1, type: 'article-mode', mode };
}

/** The remembered choice on this device (system when none or unreadable). */
export function loadArticleMode(): ArticleMode {
  try {
    const v = window.localStorage.getItem(ARTICLE_MODE_KEY);
    return v === 'light' || v === 'dark' || v === 'system' ? v : 'system';
  } catch {
    return 'system';
  }
}

/** Remember the choice on this device (best effort). */
export function saveArticleMode(mode: ArticleMode): void {
  try {
    window.localStorage.setItem(ARTICLE_MODE_KEY, mode);
  } catch {
    // Not kept; the choice holds for this run.
  }
}
