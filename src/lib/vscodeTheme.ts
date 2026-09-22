// vscodeTheme.ts — tier-1 palette resolution from VSCode theme sources.
//
// Accepts the JSONC dialects shipped upstream (comments, trailing
// commas). Missing hues fall back to built-ins; a missing type field
// resolves dark/light from background luminance.

export interface Tier1 {
  background: string;
  foreground: string;
  red: string;
  green: string;
  blue: string;
  yellow: string;
  isDark: boolean;
}

interface TokenColor {
  scope?: string | string[];
  settings?: { foreground?: string };
}

interface ThemeDoc {
  type?: string;
  colors?: Record<string, string>;
  tokenColors?: TokenColor[];
}

const FALLBACK: Tier1 = {
  background: '#1a1b26',
  foreground: '#a9b1d1',
  red: '#f7768e',
  green: '#9ece6a',
  blue: '#7aa2f7',
  yellow: '#e0af68',
  isDark: true,
};

/** Strip line/block comments and trailing commas outside strings. */
export function stripJsonc(text: string): string {
  const out: string[] = [];
  let i = 0;
  const n = text.length;
  let instr = false;
  let esc = false;
  while (i < n) {
    const ch = text[i];
    if (instr) {
      out.push(ch);
      if (esc) esc = false;
      else if (ch === '\\') esc = true;
      else if (ch === '"') instr = false;
      i += 1;
      continue;
    }
    if (ch === '"') {
      instr = true;
      out.push(ch);
      i += 1;
      continue;
    }
    if (ch === '/' && i + 1 < n && text[i + 1] === '/') {
      const j = text.indexOf('\n', i);
      i = j < 0 ? n : j;
      continue;
    }
    if (ch === '/' && i + 1 < n && text[i + 1] === '*') {
      const j = text.indexOf('*/', i + 2);
      i = j < 0 ? n : j + 2;
      continue;
    }
    out.push(ch);
    i += 1;
  }
  return out.join('').replace(/,(\s*[}\]])/g, '$1');
}

/** Parse a theme source; throws on invalid JSON. */
export function parseVscodeTheme(text: string): ThemeDoc {
  return JSON.parse(stripJsonc(text)) as ThemeDoc;
}

function luminance(hex: string): number {
  const m = /^#([0-9a-fA-F]{6})/.exec(hex);
  if (!m) return 0;
  const c = [0, 2, 4].map((o) => {
    const v = parseInt(m[1].slice(o, o + 2), 16) / 255;
    return v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
}

/** First token foreground whose scope list names a string scope. */
function stringHue(tokens: TokenColor[] | undefined): string | undefined {
  for (const t of tokens ?? []) {
    const scopes = Array.isArray(t.scope) ? t.scope : String(t.scope ?? '').split(',');
    if (scopes.map((s) => s.trim()).some((s) => s === 'string' || s.startsWith('string.'))) {
      if (t.settings?.foreground) return t.settings.foreground;
    }
  }
  return undefined;
}

/** Resolve hue slots with built-in fallbacks. */
export function resolveTier1(doc: ThemeDoc): Tier1 {
  const c = doc.colors ?? {};
  const background = c['editor.background'] ?? FALLBACK.background;
  const type = (doc.type ?? '').toLowerCase();
  return {
    background,
    foreground: c['editor.foreground'] ?? FALLBACK.foreground,
    red: c['editorError.foreground'] ?? FALLBACK.red,
    green: stringHue(doc.tokenColors) ?? FALLBACK.green,
    blue: c['textLink.foreground'] ?? c['editorInfo.foreground'] ?? FALLBACK.blue,
    yellow: c['editorWarning.foreground'] ?? FALLBACK.yellow,
    isDark: type ? type.includes('dark') : luminance(background) < 0.3,
  };
}
