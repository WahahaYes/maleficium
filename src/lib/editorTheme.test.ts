// editorTheme.test.ts — style record follows prefs + tokens.

import { describe, expect, it } from 'vitest';
import { alpha } from '@mui/material/styles';
import { DEFAULT_PREFS } from './appearance';
import { createAppTheme } from './theme';
import { editorStyle, editorTheme } from './editorTheme';

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
