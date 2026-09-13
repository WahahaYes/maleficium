// theme.ts — single source for palette, type scale, density.
//
// Growth cap: scalars only. Dark default (Pitch-1 §light/dark, Linux-first);
// a `light` palette ships in the same factory so light later is a mode flip,
// never a rewrite. ONE density knob (comfortable/compact) — no per-component
// density props (ideation-slim §7).

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
