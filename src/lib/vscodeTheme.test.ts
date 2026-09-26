// vscodeTheme.test.ts — palette resolution over an inline fixture.

import { describe, expect, it } from 'vitest';
import { parseVscodeTheme, resolveTier1 } from './vscodeTheme';

const FIXTURE = `{
  // a comment the importer must tolerate
  "colors": {
    "editor.background": "#282A36",
    "editor.foreground": "#F8F8F2",
    "editorError.foreground": "#FF5555",
  },
  "tokenColors": [
    { "scope": "string, constant", "settings": { "foreground": "#50FA7B" } },
  ],
}`;

describe('tier-1 resolution', () => {
  it('reads hues and infers dark without a type field', () => {
    const t1 = resolveTier1(parseVscodeTheme(FIXTURE));
    expect(t1.background).toBe('#282A36');
    expect(t1.foreground).toBe('#F8F8F2');
    expect(t1.red).toBe('#FF5555');
    expect(t1.green).toBe('#50FA7B');
    expect(t1.isDark).toBe(true);
  });

  it('falls back on missing keys', () => {
    const t1 = resolveTier1(parseVscodeTheme('{"colors": {}}'));
    expect(t1.blue).toBe('#7aa2f7');
    expect(t1.yellow).toBe('#e0af68');
  });

  it('rejects invalid sources', () => {
    expect(() => parseVscodeTheme('not json')).toThrow();
  });
});
