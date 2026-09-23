// keymap.ts — single source for keyboard chords.
//
// Static table, no state. Chords here are app-level (window keydown) and
// must not collide with editing keys.

export interface KeyChord {
  id: string;
  label: string;
  /** e.g. "Ctrl+R". Display only. */
  keys: string;
}

export const KEYMAP: KeyChord[] = [
  { id: 'compile', label: 'Compile', keys: 'Ctrl+R' },
  { id: 'save', label: 'Save', keys: 'Ctrl+S' },
  { id: 'open-project', label: 'Open Project…', keys: 'Ctrl+O' },
  { id: 'close-file', label: 'Close File', keys: 'Ctrl+W' },
  { id: 'toggle-tree', label: 'Toggle file tree', keys: 'Ctrl+B' },
  { id: 'next-buffer', label: 'Next open file', keys: 'Ctrl+Tab' },
  { id: 'prev-buffer', label: 'Previous open file', keys: 'Ctrl+Shift+Tab' },
  { id: 'forward-sync', label: 'SyncTeX: editor → PDF', keys: 'Ctrl+Alt+J' },
  { id: 'find', label: 'Find in File', keys: 'Ctrl+F' },
  { id: 'find-in-project', label: 'Find in Project', keys: 'Ctrl+Shift+F' },
  { id: 'inverse-sync', label: 'SyncTeX: PDF → editor (click PDF)', keys: 'Click' },
  { id: 'resize-pane', label: 'Resize editor/preview (splitter focused)', keys: '← / →' },
  { id: 'select-all', label: 'Select All', keys: 'Ctrl+A' },
  { id: 'expand-selection', label: 'Expand Selection', keys: 'Shift+Alt+Right' },
  { id: 'shrink-selection', label: 'Shrink Selection', keys: 'Shift+Alt+Left' },
  { id: 'go-to-line', label: 'Go to Line…', keys: 'Ctrl+G' },
  { id: 'file-history', label: 'File History…', keys: 'Ctrl+H' },
  { id: 'quick-open', label: 'Go to File…', keys: 'Ctrl+P' },
  { id: 'command-palette', label: 'Command Palette…', keys: 'Ctrl+Shift+P' },
  { id: 'zoom-in', label: 'Zoom preview in', keys: 'Ctrl+=' },
  { id: 'zoom-out', label: 'Zoom preview out', keys: 'Ctrl+-' },
  { id: 'zoom-fit-width', label: 'Fit preview to width', keys: 'Ctrl+0' },
  { id: 'shortcuts', label: 'This shortcuts list', keys: '?' },
];

export function matchesCompile(e: KeyboardEvent): boolean {
  return (e.ctrlKey || e.metaKey) && !e.shiftKey && !e.altKey && e.key.toLowerCase() === 'r';
}

export function matchesForwardSync(e: KeyboardEvent): boolean {
  return (e.ctrlKey || e.metaKey) && e.altKey && !e.shiftKey && e.key.toLowerCase() === 'j';
}

/** Preview zoom chords: Ctrl+= (or Ctrl++) in, Ctrl+- out, Ctrl+0 fit width. */
export function zoomChord(e: KeyboardEvent): 'in' | 'out' | 'fit-width' | null {
  if (!(e.ctrlKey || e.metaKey) || e.altKey) return null;
  if (e.key === '=' || e.key === '+') return 'in';
  if (e.key === '-' || e.key === '_') return 'out';
  if (e.key === '0' && !e.shiftKey) return 'fit-width';
  return null;
}

export type MenuChordId =
  | 'file.open-project'
  | 'file.close-file'
  | 'file.save'
  | 'history.show'
  | 'file.quick-open'
  | 'view.command-palette'
  | 'selection.select-all'
  | 'selection.expand'
  | 'selection.shrink'
  | 'selection.go-to-line'
  | 'edit.find'
  | 'search.find-in-project';

/** Menu-owned chords. Returns the registry id, or null. CodeMirror text inputs
 *  keep Ctrl+A (native select-all) and Ctrl+F (its own find panel) — the menu
 *  command and the native behavior coincide, so we only claim them when focus
 *  is OUTSIDE the editor. */
export function menuChordId(e: KeyboardEvent): MenuChordId | null {
  const mod = e.ctrlKey || e.metaKey;
  const t = e.target as HTMLElement | null;
  const inEditor = !!t?.closest?.('.cm-editor');
  if (!mod || e.altKey) return null;
  const k = e.key.toLowerCase();
  if (k === 'o' && !e.shiftKey) return 'file.open-project';
  if (k === 'w' && !e.shiftKey) return 'file.close-file';
  if (k === 's' && !e.shiftKey) return 'file.save';
  if (k === 'g' && !e.shiftKey) return 'selection.go-to-line';
  if (k === 'h' && !e.shiftKey) return 'history.show';
  if (k === 'p') return e.shiftKey ? 'view.command-palette' : 'file.quick-open';
  if (k === 'a' && !e.shiftKey && !inEditor) return 'selection.select-all';
  if (k === 'f' && !e.shiftKey && !inEditor) return 'edit.find';
  if (k === 'f' && e.shiftKey) return 'search.find-in-project';
  if (e.shiftKey && (e.key === 'ArrowRight' || e.key === 'ArrowLeft') && e.altKey) {
    return e.key === 'ArrowRight' ? 'selection.expand' : 'selection.shrink';
  }
  return null;
}

export function matchesMenuChord(e: KeyboardEvent): boolean {
  return menuChordId(e) != null;
}
