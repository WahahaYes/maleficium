import { describe, expect, it } from 'vitest';
import { cleanTokens, parseInit, parseTheme, tokenValueOk } from './bridge';

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
  it('keeps the allowed value forms', () => {
    for (const v of [
      '#fff',
      '#1E1B24',
      '#1e1b2480',
      'rgb(1, 2, 3)',
      'rgba(1,2,3,0.5)',
      'hsl(210 50% 40%)',
      '"Libertinus Serif", Georgia, serif',
      '1.0625rem',
      'min(100%, 64rem)',
      '1.2',
      '0 2px 8px rgba(30, 27, 36, 0.16)',
    ]) {
      expect(tokenValueOk(v), v).toBe(true);
    }
  });
  it('drops hostile values', () => {
    const hostile = [
      'url(https://example.org/x)',
      'url(#x)',
      'expression(alert(1))',
      '#fff;}body{display:none',
      '#fff}',
      "'Libertinus'",
      '"a";x:y',
      '"</style><script>alert(1)</script>"',
      '</style>',
      '#fff\n.x{}',
      '1rem\n',
      'rgb(1,2,url(x))',
      'var(--m-color-bg)',
      'calc(1px + 1px)',
      '/* */',
      '@import',
      '',
      '1'.repeat(201),
    ];
    for (const v of hostile) expect(tokenValueOk(v), v).toBe(false);
    const tokens = Object.fromEntries(hostile.map((v, i) => [`--m-x-${i}`, v]));
    expect(cleanTokens({ ...tokens, '--m-ok': '#000' })).toEqual({ '--m-ok': '#000' });
    const many = Object.fromEntries(Array.from({ length: 200 }, (_, i) => [`--m-t-${i}`, '#000']));
    expect(Object.keys(cleanTokens(many)).length).toBe(128);
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
