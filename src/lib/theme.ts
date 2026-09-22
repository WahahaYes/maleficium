// theme.ts — single source for palette, type scale, density.
//
// Scalars only. Dark default; `light` ships in the same factory. One
// density knob (comfortable/compact), no per-component density props.

import { createTheme, lighten } from '@mui/material';
import type { Shadows } from '@mui/material/styles';
import type { AppearancePrefs } from './appearance';
import { THEME_CATALOG } from './themeCatalog';
import { parseVscodeTheme, resolveTier1 } from './vscodeTheme';

export type Density = 'comfortable' | 'compact';
export type ThemeMode = 'dark' | 'light';

export const typeScale = {
  editorMono: 14,
  ui: 13,
  caption: 11,
  /** Dense rows: log lines, the outline filter field. */
  dense: 12,
  /** Chords, accelerators, and log lines outside the editor. */
  uiMonoFamily: 'monospace',
  editorFamily: "'JetBrains Mono', ui-monospace, SFMono-Regular, Menlo, monospace",
} as const;

/** Flattened elevation: soft border-shadow at rest, hairlines above. */
const flatShadows = Array.from({ length: 25 }, (_, i) =>
  i === 0 ? 'none' : i === 1 ? '0 0 0 1px rgba(0, 0, 0, 0.08)' : '0 0 0 1px rgba(0, 0, 0, 0.06)',
) as Shadows;

/** Vendored hues for a named accent; null keeps the built-in palette. */
function tier1For(accent: string) {
  const entry = THEME_CATALOG.find((e) => e.id === accent);
  if (!entry) return null;
  try {
    return resolveTier1(parseVscodeTheme(entry.source));
  } catch {
    return null;
  }
}

/** App theme factory: prefs in, MUI theme out. */
export function createAppTheme(prefs: AppearancePrefs) {
  const { density } = prefs;
  const t1 = tier1For(prefs.accent);
  const dark = t1 ? t1.isDark : prefs.mode === 'dark';
  const background = {
    default: t1?.background ?? (dark ? '#1a1b26' : '#fafafa'),
    paper: t1 ? lighten(t1.background, dark ? 0.12 : 0.35) : dark ? '#24283b' : '#ffffff',
  };
  return createTheme({
    palette: {
      mode: dark ? 'dark' : 'light',
      primary: { main: t1?.blue ?? (dark ? '#7aa2f7' : '#34548a') },
      ...(dark
        ? {
            secondary: { main: '#bb9af7' },
            success: { main: t1?.green ?? '#9ece6a' },
            warning: { main: t1?.yellow ?? '#e0af68' },
            error: { main: t1?.red ?? '#f7768e' },
            ...(t1 ? { info: { main: t1.blue } } : {}),
          }
        : {
            ...(t1
              ? {
                  success: { main: t1.green },
                  warning: { main: t1.yellow },
                  error: { main: t1.red },
                  info: { main: t1.blue },
                }
              : {}),
          }),
      background,
      ...(t1 ? { text: { primary: t1.foreground } } : {}),
    },
    typography: {
      fontSize: 13 * prefs.uiScale,
      fontFamily: 'Inter, system-ui, sans-serif',
      fontWeightLight: 400,
      fontWeightRegular: 400,
      fontWeightMedium: 500,
      fontWeightBold: 700,
    },
    spacing: density === 'compact' ? 4 : 8,
    shape: { borderRadius: 8 },
    shadows: flatShadows,
    transitions: {
      duration: {
        shortest: 100,
        shorter: 120,
        short: 150,
        standard: 150,
        complex: 200,
        enteringScreen: 150,
        leavingScreen: 120,
      },
    },
    components: {
      MuiButtonBase: { defaultProps: { disableRipple: true } },
      MuiButton: {
        defaultProps: { size: 'small' },
        styleOverrides: { root: { borderRadius: 6 } },
      },
      MuiChip: { defaultProps: { size: 'small' } },
      MuiToolbar: { defaultProps: { variant: 'dense' } },
      MuiPaper: {
        styleOverrides: {
          root: { borderRadius: 12, backgroundImage: 'none' },
        },
      },
    },
  });
}
