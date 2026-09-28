// editorTheme.test.ts — style record follows prefs + tokens.

import { describe, expect, it } from 'vitest';
import { alpha, getContrastRatio } from '@mui/material/styles';
import { DEFAULT_PREFS } from './appearance';
import { createAppTheme } from './theme';
import { editorStyle, editorTheme, MIN_CONTRAST, readable, syntaxColors } from './editorTheme';
import { THEME_CATALOG } from './themeCatalog';

describe('editor style record', () => {
  it('maps prefs shape and dark tokens', () => {
    const theme = createAppTheme(DEFAULT_PREFS);
    const s = editorStyle(DEFAULT_PREFS, theme);
    expect(s.background).toBe('#24283b');
    expect(s.fontFamily).toBe(DEFAULT_PREFS.editorFont);
    expect(s.fontSize).toBe('14px');
    expect(s.lineHeight).toBe('1.5');
    expect(s.flash).toBe(alpha(theme.palette.warning.main, 0.55));
    expect(s.hit).toBe(alpha(theme.palette.warning.main, 0.8));
  });

  it('applies font, size, and ligature overrides', () => {
    const theme = createAppTheme(DEFAULT_PREFS);
    const s = editorStyle(
      { ...DEFAULT_PREFS, editorFont: 'monospace', editorSize: 18, ligatures: true },
      theme,
    );
    expect(s.fontFamily).toBe('monospace');
    expect(s.fontSize).toBe('18px');
    expect(s.ligatures).toBe('"liga" 1');
  });

  it('builds a live extension', () => {
    expect(editorTheme(DEFAULT_PREFS, createAppTheme(DEFAULT_PREFS))).toBeTruthy();
  });
});

describe('syntax colors', () => {
  const variants = [
    { ...DEFAULT_PREFS, mode: 'dark' as const },
    { ...DEFAULT_PREFS, mode: 'light' as const },
    ...THEME_CATALOG.map((e) => ({ ...DEFAULT_PREFS, accent: e.id })),
  ];

  it.each(variants.map((p) => [p.accent + '/' + p.mode, p] as const))(
    '%s: every token meets WCAG AA on the editor background',
    (_, prefs) => {
      const theme = createAppTheme(prefs);
      const bg = theme.palette.background.paper;
      for (const [token, color] of Object.entries(syntaxColors(theme))) {
        expect(getContrastRatio(color, bg), `${token} ${color} on ${bg}`).toBeGreaterThanOrEqual(
          MIN_CONTRAST,
        );
      }
    },
  );

  it('keeps a color that already reads and lifts a dim one', () => {
    expect(readable('#ffffff', '#000000', true)).toBe('#ffffff');
    const lifted = readable('#5a3d7a', '#24283b', true);
    expect(getContrastRatio(lifted, '#24283b')).toBeGreaterThanOrEqual(MIN_CONTRAST);
  });
});
