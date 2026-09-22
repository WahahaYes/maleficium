// appearance.ts — appearance prefs shape and defaults.
//
// Single object describing the look; the theme factory consumes it and the
// settings dialog will edit it. Defaults reproduce the current visuals.

import { typeScale, type Density, type ThemeMode } from './theme';
import { DEVICE_PREF_KEYS, store } from './app-store';

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
};

/** Stored prefs merged over defaults; corrupt or missing storage wins nothing. */
export function loadAppearance(): AppearancePrefs {
  let parsed: Partial<AppearancePrefs> = {};
  try {
    const raw = store().get(DEVICE_PREF_KEYS.appearance);
    if (raw) parsed = JSON.parse(raw) as Partial<AppearancePrefs>;
  } catch {
    parsed = {};
  }
  const num = (v: unknown, fallback: number) =>
    typeof v === 'number' && Number.isFinite(v) ? v : fallback;
  return {
    mode: parsed.mode === 'light' ? 'light' : 'dark',
    density: parsed.density === 'compact' ? 'compact' : 'comfortable',
    uiScale: num(parsed.uiScale, DEFAULT_PREFS.uiScale),
    editorFont:
      typeof parsed.editorFont === 'string' && parsed.editorFont
        ? parsed.editorFont
        : DEFAULT_PREFS.editorFont,
    editorSize: num(parsed.editorSize, DEFAULT_PREFS.editorSize),
    lineHeight: num(parsed.lineHeight, DEFAULT_PREFS.lineHeight),
    ligatures: parsed.ligatures === true,
    accent: typeof parsed.accent === 'string' && parsed.accent ? parsed.accent : 'default',
    contrast: parsed.contrast === 'high' ? 'high' : 'standard',
  };
}

/** Persist the whole prefs object under one versioned key. */
export function saveAppearance(prefs: AppearancePrefs): void {
  store().set(DEVICE_PREF_KEYS.appearance, JSON.stringify(prefs));
}
