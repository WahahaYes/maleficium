import { describe, expect, it } from 'vitest';
import { matchesShortcutsKey } from './keymap';

const key = (target: object | null, mods: Partial<KeyboardEvent> = {}) =>
  ({ key: '?', ctrlKey: false, metaKey: false, altKey: false, target, ...mods }) as KeyboardEvent;

describe('matchesShortcutsKey', () => {
  it('opens the list from outside a text field', () => {
    expect(matchesShortcutsKey(key({ tagName: 'DIV', isContentEditable: false }))).toBe(true);
    expect(matchesShortcutsKey(key(null))).toBe(true);
  });

  it('leaves ? to the editor and text fields', () => {
    expect(matchesShortcutsKey(key({ tagName: 'DIV', isContentEditable: true }))).toBe(false);
    expect(matchesShortcutsKey(key({ tagName: 'INPUT' }))).toBe(false);
    expect(matchesShortcutsKey(key({ tagName: 'TEXTAREA' }))).toBe(false);
  });

  it('ignores ? with a modifier', () => {
    expect(matchesShortcutsKey(key(null, { ctrlKey: true }))).toBe(false);
  });
});
