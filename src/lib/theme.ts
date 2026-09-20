// theme.ts — single source for palette, type scale, density.
//
// Scalars only. Dark default; `light` ships in the same factory. One
// density knob (comfortable/compact), no per-component density props.

import { createTheme } from '@mui/material';
import type { Shadows } from '@mui/material/styles';
import type { AppearancePrefs } from './appearance';

export type Density = 'comfortable' | 'compact';
export type ThemeMode = 'dark' | 'light';

export const densitySpacing = (d: Density) => ({
  railPadding: d === 'comfortable' ? 8 : 4,
  toolbarGap: d === 'comfortable' ? 16 : 8,
  editorPadding: d === 'comfortable' ? 16 : 8,
  treeIndent: d === 'comfortable' ? 16 : 10,
});

export const typeScale = {
  editorMono: 14,
  ui: 13,
  caption: 11,
  editorFamily: "'JetBrains Mono', ui-monospace, SFMono-Regular, Menlo, monospace",
} as const;

/** Flattened elevation: soft border-shadow at rest, hairlines above. */
const flatShadows = Array.from({ length: 25 }, (_, i) =>
  i === 0 ? 'none' : i === 1 ? '0 0 0 1px rgba(0, 0, 0, 0.08)' : '0 0 0 1px rgba(0, 0, 0, 0.06)',
) as Shadows;

/** App theme factory: prefs in, MUI theme out. */
export function createAppTheme(prefs: AppearancePrefs) {
  const { mode, density } = prefs;
  return createTheme({
    palette: {
      mode,
      ...(mode === 'light'
        ? {
            primary: { main: '#34548a' },
            background: { default: '#fafafa', paper: '#ffffff' },
          }
        : {
            primary: { main: '#7aa2f7' },
            secondary: { main: '#bb9af7' },
            success: { main: '#9ece6a' },
            warning: { main: '#e0af68' },
            error: { main: '#f7768e' },
            background: { default: '#1a1b26', paper: '#24283b' },
          }),
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
    shape: { borderRadius: prefs.radius },
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
        styleOverrides: { root: { borderRadius: 12, backgroundImage: 'none' } },
      },
    },
  });
}
