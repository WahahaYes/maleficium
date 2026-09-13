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
  { id: 'toggle-tree', label: 'Toggle file tree', keys: 'Ctrl+B' },
  { id: 'next-buffer', label: 'Next open file', keys: 'Ctrl+Tab' },
  { id: 'prev-buffer', label: 'Previous open file', keys: 'Ctrl+Shift+Tab' },
  { id: 'forward-sync', label: 'SyncTeX: editor → PDF', keys: 'Ctrl+Shift+F' },
  { id: 'inverse-sync-hint', label: 'SyncTeX: PDF → editor (click PDF)', keys: 'Click' },
  { id: 'resize-pane', label: 'Resize editor/preview (splitter focused)', keys: '← / →' },
  { id: 'shortcuts', label: 'This shortcuts list', keys: '?' },
];

export function matchesCompile(e: KeyboardEvent): boolean {
  return (e.ctrlKey || e.metaKey) && !e.shiftKey && !e.altKey && e.key.toLowerCase() === 'r';
}

export function matchesForwardSync(e: KeyboardEvent): boolean {
  return (e.ctrlKey || e.metaKey) && e.shiftKey && e.key.toLowerCase() === 'f';
}
