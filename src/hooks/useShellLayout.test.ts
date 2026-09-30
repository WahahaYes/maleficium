import { describe, expect, it } from 'vitest';
import { parseLayout } from './useShellLayout';

describe('parseLayout', () => {
  it('defaults on missing or corrupt prefs', () => {
    const d = { editorRatio: 0.6, previewRatio: 0.4, logHeight: 160 };
    expect(parseLayout(null)).toEqual(d);
    expect(parseLayout('{nope')).toEqual(d);
  });
  it('clamps stored values and defaults the missing ones', () => {
    expect(parseLayout('{"editorRatio":0.99,"logHeight":5}')).toEqual({
      editorRatio: 0.8,
      previewRatio: 0.4,
      logHeight: 80,
    });
  });
});
