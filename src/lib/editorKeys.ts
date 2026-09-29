// Editor keys the app owns. CodeMirror's basicSetup binds its search keymap,
// whose bindings open the stock search panel; the app's find is the MUI bar,
// and Ctrl+G is the app's Go to Line. A binding here that returns true stops
// CodeMirror's own handler, while the window listener still sees the key.
import type { KeyBinding } from '@codemirror/view';

export function appOwnedEditorKeys(openFindBar: () => void): KeyBinding[] {
  const find = () => {
    openFindBar();
    return true;
  };
  return [
    { key: 'Mod-f', run: find },
    { key: 'F3', run: find },
    { key: 'Shift-F3', run: find },
    // Go to Line, opened by the app: not CodeMirror's find-next/previous.
    { key: 'Mod-g', run: () => true },
    { key: 'Shift-Mod-g', run: () => true },
  ];
}
