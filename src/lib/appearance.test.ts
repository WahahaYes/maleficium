// appearance.test.ts — prefs defaults reproduce current tokens.

import { describe, expect, it, vi, beforeEach } from 'vitest';
import { DEFAULT_PREFS, loadAppearance, saveAppearance } from './appearance';
import type { AppearancePrefs } from './appearance';
import { setAppStore } from './app-store';
import { localAppStore } from './app-store.web';
import { createAppTheme } from './theme';

setAppStore(localAppStore);

const mem: Record<string, string> = {};
vi.stubGlobal('localStorage', {
  getItem: (k: string) => mem[k] ?? null,
  setItem: (k: string, v: string) => {
    mem[k] = v;
  },
  removeItem: (k: string) => {
    delete mem[k];
  },
});

beforeEach(() => {
  for (const k of Object.keys(mem)) delete mem[k];
});

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

describe('appearance persistence', () => {
  it('round-trips prefs through the store', () => {
    const prefs: AppearancePrefs = { ...DEFAULT_PREFS, radius: 12, accent: 'dracula' };
    saveAppearance(prefs);
    expect(loadAppearance()).toEqual(prefs);
  });

  it('falls back to defaults on missing or corrupt data', () => {
    expect(loadAppearance()).toEqual(DEFAULT_PREFS);
    mem['maleficium.appearance.v1'] = '{broken';
    expect(loadAppearance()).toEqual(DEFAULT_PREFS);
    mem['maleficium.appearance.v1'] = JSON.stringify({ mode: 'neon', radius: 99 });
    const loaded = loadAppearance();
    expect(loaded.mode).toBe('dark');
    expect(loaded.radius).toBe(99);
  });
});
