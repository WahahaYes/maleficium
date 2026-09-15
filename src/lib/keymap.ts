// keymap.ts — single source for keyboard chords.
//
// Growth cap: static table, no state. CodeMirror reserves its own editing keys;
// chords here are app-level (window keydown) and must not collide with them.

export interface KeyChord {
  id: string;
  label: string;
  /** e.g. "Ctrl+R". Display only; matching is explicit in App. */
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
  { id: 'forward-sync', label: 'SyncTeX: editor → PDF', keys: 'Ctrl+Shift+F' },
  { id: 'inverse-sync', label: 'SyncTeX: PDF → editor (click PDF)', keys: 'Click' },
  { id: 'resize-pane', label: 'Resize editor/preview (splitter focused)', keys: '← / →' },
  { id: 'select-all', label: 'Select All', keys: 'Ctrl+A' },
  { id: 'expand-selection', label: 'Expand Selection', keys: 'Shift+Alt+Right' },
  { id: 'shrink-selection', label: 'Shrink Selection', keys: 'Shift+Alt+Left' },
  { id: 'go-to-line', label: 'Go to Line…', keys: 'Ctrl+G' },
  { id: 'shortcuts', label: 'This shortcuts list', keys: '?' },
];

export function matchesCompile(e: KeyboardEvent): boolean {
  return (e.ctrlKey || e.metaKey) && !e.shiftKey && !e.altKey && e.key.toLowerCase() === 'r';
}

export function matchesForwardSync(e: KeyboardEvent): boolean {
  return (e.ctrlKey || e.metaKey) && e.shiftKey && e.key.toLowerCase() === 'f';
}

export type MenuChordId =
  | 'file.open-project' | 'file.close-file' | 'file.save'
  | 'selection.select-all' | 'selection.expand' | 'selection.shrink' | 'selection.go-to-line';

/** Menu-owned chords. Returns the registry id, or null. CodeMirror text inputs
 *  keep Ctrl+A (native select-all) — the menu command and the native behavior
 *  coincide, so we only claim it when focus is OUTSIDE the editor. */
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
  if (k === 'a' && !e.shiftKey && !inEditor) return 'selection.select-all';
  if (e.shiftKey && (e.key === 'ArrowRight' || e.key === 'ArrowLeft') && e.altKey) {
    return e.key === 'ArrowRight' ? 'selection.expand' : 'selection.shrink';
  }
  return null;
}

export function matchesMenuChord(e: KeyboardEvent): boolean {
  return menuChordId(e) != null;
}
