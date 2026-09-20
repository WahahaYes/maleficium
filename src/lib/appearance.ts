// appearance.ts — appearance prefs shape and defaults.
//
// Single object describing the look; the theme factory consumes it and the
// settings dialog will edit it. Defaults reproduce the current visuals.

import { typeScale, type Density, type ThemeMode } from './theme';

export type ContrastStep = 'standard' | 'high';

export interface AppearancePrefs {
  mode: ThemeMode;
  density: Density;
  uiScale: number;
  editorFont: string;
  editorSize: number;
  lineHeight: number;
  ligatures: boolean;
  accent: string;
  contrast: ContrastStep;
  radius: number;
}

export const DEFAULT_PREFS: AppearancePrefs = {
  mode: 'dark',
  density: 'comfortable',
  uiScale: 1,
  editorFont: typeScale.editorFamily,
  editorSize: typeScale.editorMono,
  lineHeight: 1.5,
  ligatures: false,
  accent: 'default',
  contrast: 'standard',
  radius: 8,
};
