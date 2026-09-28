import { describe, expect, it, vi } from 'vitest';
import { searchKeymap } from '@codemirror/search';
import type { EditorView } from '@codemirror/view';
import { appOwnedEditorKeys } from './editorKeys';

describe('appOwnedEditorKeys', () => {
  const view = {} as EditorView;

  it('claims every search key that would open the stock panel', () => {
    const owned = new Set(appOwnedEditorKeys(() => {}).map((b) => b.key));
    const stock = searchKeymap
      .flatMap((b) => [b.key, b.mac, b.win, b.linux])
      .filter((k): k is string => !!k && /^(Mod-f|Mod-g|Shift-Mod-g|F3|Shift-F3)$/.test(k));
    expect(stock.length).toBeGreaterThan(0);
    for (const k of stock) expect(owned, k).toContain(k);
  });

  it('routes find keys to the app bar and swallows Go to Line', () => {
    const openBar = vi.fn();
    const keys = new Map(appOwnedEditorKeys(openBar).map((b) => [b.key, b.run!]));
    expect(keys.get('F3')!(view)).toBe(true);
    expect(keys.get('Mod-f')!(view)).toBe(true);
    expect(openBar).toHaveBeenCalledTimes(2);
    expect(keys.get('Mod-g')!(view)).toBe(true);
    expect(keys.get('Shift-Mod-g')!(view)).toBe(true);
    expect(openBar).toHaveBeenCalledTimes(2);
  });
});
