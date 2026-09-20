// theme.ts — single source for palette, type scale, density.
//
// Scalars only. Dark default; `light` ships in the same factory. One
// density knob (comfortable/compact), no per-component density props.

import { createTheme } from '@mui/material';

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

/** App theme factory: mode + density in, MUI theme out. */
export function createAppTheme(mode: ThemeMode, density: Density = 'comfortable') {
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
    typography: { fontSize: 13, fontFamily: 'Inter, system-ui, sans-serif' },
    spacing: density === 'compact' ? 4 : 8,
    components: {
      MuiButton: { defaultProps: { size: 'small' } },
      MuiChip: { defaultProps: { size: 'small' } },
      MuiToolbar: { defaultProps: { variant: 'dense' } },
    },
  });
}
