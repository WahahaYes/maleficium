import { describe, expect, it } from 'vitest';
import { cleanTokens, parseInit, parseTheme } from './bridge';

const theme = { mode: 'dark', tokens: { '--m-color-text': '#fff', color: 'red', '--m-x': 3 } };
const good = {
  type: 'init',
  protocol: 1,
  widgetId: 'tab-results',
  runtime: 'table@1',
  alt: 'Scores',
  options: { sort: 'score' },
  sources: {
    data: { name: 'results.csv', mime: 'text/csv', sha256: 'ab', bytes: new ArrayBuffer(2) },
  },
  theme,
};

describe('bridge parsing', () => {
  it('keeps only --m-* string tokens', () => {
    expect(cleanTokens(theme.tokens)).toEqual({ '--m-color-text': '#fff' });
    expect(cleanTokens(null)).toEqual({});
  });
  it('accepts a well-formed init', () => {
    const i = parseInit(good);
    expect(i?.widgetId).toBe('tab-results');
    expect(i?.sources.data.bytes.byteLength).toBe(2);
    expect(i?.theme.tokens).toEqual({ '--m-color-text': '#fff' });
  });
  it('rejects wrong protocol, missing bytes and a bad theme', () => {
    expect(parseInit({ ...good, protocol: 2 })).toBeNull();
    expect(parseInit({ ...good, sources: { data: { name: 'x', bytes: 'text' } } })).toBeNull();
    expect(parseInit({ ...good, theme: { mode: 'sepia', tokens: {} } })).toBeNull();
    expect(parseInit({ ...good, sources: null })).toBeNull();
    expect(parseInit('init')).toBeNull();
    expect(parseTheme({ mode: 'light' })).toEqual({ mode: 'light', tokens: {} });
  });
});
