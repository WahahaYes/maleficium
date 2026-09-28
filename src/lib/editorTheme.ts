// editorTheme.ts — CodeMirror styling synced to prefs + MUI tokens.
//
// Font, size, spacing, flash, and syntax token colors follow the app theme.
// Token colors take the theme's hues, adjusted until each one reaches WCAG
// AA contrast against the editor background.

import { HighlightStyle, syntaxHighlighting } from '@codemirror/language';
import { EditorView } from '@codemirror/view';
import { tags } from '@lezer/highlight';
import { alpha, darken, getContrastRatio, lighten, type Theme } from '@mui/material/styles';
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

/** WCAG AA for normal-size text. */
export const MIN_CONTRAST = 4.5;

/** `color`, lightened on a dark background or darkened on a light one until
 *  it reaches MIN_CONTRAST against `background`. */
export function readable(color: string, background: string, dark: boolean): string {
  let c = color;
  for (let step = 0; getContrastRatio(c, background) < MIN_CONTRAST && step < 20; step += 1) {
    c = dark ? lighten(c, 0.1) : darken(c, 0.1);
  }
  return c;
}

export interface SyntaxColors {
  command: string;
  math: string;
  comment: string;
  brace: string;
}

/** Token colors for the LaTeX grammar (src/lib/texMode.ts): commands in the
 *  theme's primary hue, math in its string hue, comments dimmed. */
export function syntaxColors(theme: Theme): SyntaxColors {
  const bg = theme.palette.background.paper;
  const fg = theme.palette.text.primary;
  const dark = theme.palette.mode === 'dark';
  return {
    command: readable(theme.palette.primary.main, bg, dark),
    math: readable(theme.palette.success.main, bg, dark),
    comment: readable(dark ? darken(fg, 0.4) : lighten(fg, 0.4), bg, dark),
    brace: readable(fg, bg, dark),
  };
}

/** Live editor extensions built from the style record. */
export function editorTheme(prefs: AppearancePrefs, theme: Theme) {
  const s = editorStyle(prefs, theme);
  const view = EditorView.theme({
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
    '.cm-tooltip': {
      backgroundColor: s.background,
      color: s.foreground,
      borderColor: theme.palette.divider,
    },
  });
  const c = syntaxColors(theme);
  const highlight = HighlightStyle.define([
    { tag: tags.keyword, color: c.command },
    { tag: tags.string, color: c.math },
    { tag: tags.comment, color: c.comment, fontStyle: 'italic' },
    { tag: tags.bracket, color: c.brace },
  ]);
  return [view, syntaxHighlighting(highlight)];
}
