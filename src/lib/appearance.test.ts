// appearance.test.ts — prefs defaults reproduce current tokens.

import { describe, expect, it } from 'vitest';
import { DEFAULT_PREFS } from './appearance';
import { createAppTheme } from './theme';

describe('default prefs preserve current visuals', () => {
  it('resolves dark tokens, base radius, and flat shadows', () => {
    const theme = createAppTheme(DEFAULT_PREFS);
    expect(theme.palette.mode).toBe('dark');
    expect(theme.palette.background?.paper).toBe('#24283b');
    expect(theme.palette.background?.default).toBe('#1a1b26');
    expect(theme.shape.borderRadius).toBe(8);
    expect(theme.typography.fontSize).toBe(13);
    expect(theme.shadows).toHaveLength(25);
    expect(theme.components?.MuiButtonBase?.defaultProps?.disableRipple).toBe(true);
  });

  it('resolves light surfaces', () => {
    const theme = createAppTheme({ ...DEFAULT_PREFS, mode: 'light' });
    expect(theme.palette.background?.paper).toBe('#ffffff');
    expect(theme.palette.background?.default).toBe('#fafafa');
  });

  it('applies radius and scale overrides', () => {
    const theme = createAppTheme({ ...DEFAULT_PREFS, radius: 12, uiScale: 2 });
    expect(theme.shape.borderRadius).toBe(12);
    expect(theme.typography.fontSize).toBe(26);
  });
});
