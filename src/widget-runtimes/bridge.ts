// Bridge protocol 1, the runtime side (notes/interactive-papers/
// 2026-09-30-custom-runtimes-spike.md section 4). The only channel between a
// widget frame and its host is postMessage. A runtime accepts a message only
// from window.parent, with mfw === 1 and a known type, and never trusts the
// shape of what it carries. Sources arrive as bytes in `init`; a runtime never
// fetches anything (connect-src 'none').

export interface SourceBytes {
  name: string;
  mime: string;
  sha256: string;
  bytes: ArrayBuffer;
}

export interface Theme {
  mode: 'light' | 'dark';
  tokens: Record<string, string>;
}

export interface Init {
  widgetId: string;
  runtime: string;
  alt: string;
  options: Record<string, unknown>;
  sources: Record<string, SourceBytes>;
  theme: Theme;
}

const isObject = (v: unknown): v is Record<string, unknown> =>
  typeof v === 'object' && v !== null && !Array.isArray(v);

/** Keeps only `--m-*` tokens with string values; anything else is dropped. */
export function cleanTokens(v: unknown): Record<string, string> {
  const out: Record<string, string> = {};
  if (!isObject(v)) return out;
  for (const [k, val] of Object.entries(v)) {
    if (/^--m-[a-z0-9-]+$/.test(k) && typeof val === 'string' && val.length <= 300) out[k] = val;
  }
  return out;
}

export function parseTheme(v: unknown): Theme | null {
  if (!isObject(v) || (v.mode !== 'light' && v.mode !== 'dark')) return null;
  return { mode: v.mode, tokens: cleanTokens(v.tokens) };
}

/** Validates an `init` message body; null when its shape is wrong. */
export function parseInit(data: unknown): Init | null {
  if (!isObject(data) || data.type !== 'init' || data.protocol !== 1) return null;
  const theme = parseTheme(data.theme);
  if (!theme || !isObject(data.sources)) return null;
  const sources: Record<string, SourceBytes> = {};
  for (const [role, s] of Object.entries(data.sources)) {
    if (!isObject(s) || !(s.bytes instanceof ArrayBuffer)) return null;
    sources[role] = {
      name: typeof s.name === 'string' ? s.name : role,
      mime: typeof s.mime === 'string' ? s.mime : '',
      sha256: typeof s.sha256 === 'string' ? s.sha256 : '',
      bytes: s.bytes,
    };
  }
  return {
    widgetId: typeof data.widgetId === 'string' ? data.widgetId : '',
    runtime: typeof data.runtime === 'string' ? data.runtime : '',
    alt: typeof data.alt === 'string' ? data.alt : '',
    options: isObject(data.options) ? data.options : {},
    sources,
    theme,
  };
}

export function decodeText(s: SourceBytes): string {
  return new TextDecoder('utf-8').decode(s.bytes);
}

/** Applies every token as a custom property on :root and records the colour mode. */
export function applyTheme(theme: Theme): void {
  const root = document.documentElement;
  for (const [k, v] of Object.entries(theme.tokens)) root.style.setProperty(k, v);
  root.dataset.mode = theme.mode;
  root.style.colorScheme = theme.mode;
}

function post(msg: Record<string, unknown>): void {
  window.parent.postMessage({ mfw: 1, ...msg }, '*');
}

export function status(state: 'loading' | 'loaded' | 'error', message?: string): void {
  post({ type: 'status', state, ...(message ? { message: message.slice(0, 200) } : {}) });
}

export interface Handlers {
  onInit(init: Init): void;
  onTheme(theme: Theme): void;
  /** A PNG data URL of the current view, or null when there is none. */
  onSnapshot(): string | null;
}

/** Installs the one message listener and announces `ready`. */
export function startBridge(h: Handlers): void {
  window.addEventListener('message', (e: MessageEvent) => {
    if (e.source !== window.parent) return;
    const d: unknown = e.data;
    if (!isObject(d) || d.mfw !== 1) return;
    try {
      if (d.type === 'init') {
        const init = parseInit(d);
        if (init) h.onInit(init);
        else status('error', 'the host sent a malformed init message');
      } else if (d.type === 'theme') {
        const theme = parseTheme(d);
        if (theme) h.onTheme(theme);
      } else if (d.type === 'snapshot-request' && typeof d.requestId === 'string') {
        const png = h.onSnapshot();
        if (png && png.length < 8 * 1024 * 1024) {
          post({ type: 'snapshot', requestId: d.requestId, png });
        }
      }
    } catch (err) {
      status('error', err instanceof Error ? err.message : 'the widget failed');
    }
  });
  post({ type: 'ready' });
}
