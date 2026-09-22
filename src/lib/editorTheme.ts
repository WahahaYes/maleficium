// editorTheme.ts — CodeMirror styling synced to prefs + MUI tokens.
//
// Font, size, spacing, and flash colors follow the app theme. Syntax
// token colors stay default until the tier-1 reader lands.

import { EditorView } from '@codemirror/view';
import { alpha, type Theme } from '@mui/material/styles';
import type { AppearancePrefs } from './appearance';

export interface EditorStyle {
  background: string;
  foreground: string;
  fontFamily: string;
  fontSize: string;
  lineHeight: string;
  ligatures: string;
  cursor: string;
  selection: string;
  flash: string;
  hit: string;
}

/** Plain style record: prefs shape + theme tokens in, CSS values out. */
export function editorStyle(prefs: AppearancePrefs, theme: Theme): EditorStyle {
  return {
    background: theme.palette.background.paper,
    foreground: theme.palette.text.primary,
    fontFamily: prefs.editorFont,
    fontSize: `${prefs.editorSize}px`,
    lineHeight: String(prefs.lineHeight),
    ligatures: prefs.ligatures ? '"liga" 1' : '"liga" 0',
    cursor: theme.palette.text.primary,
    selection: alpha(theme.palette.primary.main, 0.25),
    flash: alpha(theme.palette.warning.main, 0.55),
    hit: alpha(theme.palette.warning.main, 0.8),
  };
}

/** Live editor extension built from the style record. */
export function editorTheme(prefs: AppearancePrefs, theme: Theme) {
  const s = editorStyle(prefs, theme);
  return EditorView.theme({
    '&': {
      backgroundColor: s.background,
      color: s.foreground,
      fontFamily: s.fontFamily,
      fontSize: s.fontSize,
    },
    '.cm-content': {
      lineHeight: s.lineHeight,
      fontFeatureSettings: s.ligatures,
    },
    '.cm-cursor': { borderLeftColor: s.cursor },
    '.cm-selectionBackground': { backgroundColor: s.selection },
  });
}
