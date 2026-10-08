import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import {
  compileSpec,
  createView,
  csvRows,
  parseScale,
  parseSpec,
  resolveData,
  themeConfig,
} from './core';
import type { SourceBytes } from '../bridge';

const bytes = (s: string) => new TextEncoder().encode(s).buffer as ArrayBuffer;
const src = (name: string, text: string, mime = ''): SourceBytes => ({
  name,
  mime,
  sha256: '',
  bytes: bytes(text),
});
// The fixture spec: one inline value, a bar mark.
const fixtureText = readFileSync('e2e/fixtures/interactive/charts/ablation.vl.json', 'utf8');
const fixture = () => resolveData(parseSpec(src('ablation.vl.json', fixtureText)), {});

async function svg(spec: Record<string, unknown>, tokens = {}): Promise<string> {
  const view = createView(compileSpec(spec, tokens, { width: 400, height: 300 }));
  await view.runAsync();
  const out = await view.toSVG();
  view.finalize();
  return out;
}

describe('chart runtime', () => {
  it('renders the fixture spec offline to an SVG with a bar', async () => {
    const out = await svg(fixture());
    expect(out.startsWith('<svg')).toBe(true);
    expect(out).toContain('role-mark');
    expect(out).toMatch(/<path[^>]*d="M\d/); // the bar rectangle
    expect(out).toContain('>x<'); // the axis label for category "x"
  });

  it('runs expressions without eval or new Function (the sandbox CSP has no unsafe-eval)', async () => {
    const real = globalThis.Function;
    // Vega's expression compiler builds code with the Function constructor; the
    // interpreter must be used instead, so any call is a failure.
    globalThis.Function = new Proxy(real, {
      apply() {
        throw new Error('Function constructor used');
      },
      construct() {
        throw new Error('Function constructor used');
      },
    });
    try {
      const out = await svg({
        data: {
          values: [
            { a: 'x', b: 1 },
            { a: 'y', b: 5 },
            { a: 'z', b: 3 },
          ],
        },
        transform: [{ filter: 'datum.b > 1' }, { calculate: 'datum.b * 2', as: 'c' }],
        mark: 'bar',
        encoding: { x: { field: 'a' }, y: { field: 'c', type: 'quantitative' } },
      });
      expect(out).toContain('role-mark');
      expect(out).not.toContain('>x<'); // x was filtered out
    } finally {
      globalThis.Function = real;
    }
  });

  it('replaces data urls with the widget sources and fetches nothing', async () => {
    const spec = {
      data: { url: 'data/results.csv' },
      mark: 'bar',
      encoding: { x: { field: 'name' }, y: { field: 'score', type: 'quantitative' } },
    };
    const sources = {
      spec: src('s.vl.json', '{}'),
      table: src('results.csv', 'name,score\nalpha,10\nbeta,20\n'),
    };
    const resolved = resolveData(spec, sources) as { data: { values: unknown[]; url?: string } };
    expect(resolved.data.url).toBeUndefined();
    expect(resolved.data.values).toEqual([
      { name: 'alpha', score: 10 },
      { name: 'beta', score: 20 },
    ]);
    const out = await svg(resolved);
    expect(out).toContain('>alpha<');
    expect(out).toContain('>beta<');
  });

  it('matches a data url by role name, and reads JSON sources', () => {
    const spec = { layer: [{ data: { url: 'pts' }, mark: 'point' }] };
    const r = resolveData(spec, { pts: src('p.json', '[{"a":1}]', 'application/json') }) as {
      layer: { data: { values: unknown[] } }[];
    };
    expect(r.layer[0].data.values).toEqual([{ a: 1 }]);
  });

  it('refuses a data url that is not a source instead of loading it', () => {
    expect(() => resolveData({ data: { url: 'https://example.org/x.csv' } }, {})).toThrow(
      /not one of the widget's sources/,
    );
    expect(() => resolveData([], {})).toThrow(/not a JSON object/);
  });

  it('cannot load an image url through the view', async () => {
    const view = createView(
      compileSpec(
        {
          data: { values: [{ u: 'http://127.0.0.1:9/x.png', x: 1, y: 1 }] },
          mark: 'image',
          encoding: {
            url: { field: 'u' },
            x: { field: 'x', type: 'quantitative' },
            y: { field: 'y', type: 'quantitative' },
          },
        },
        {},
      ),
    );
    await view.runAsync();
    await expect(view.toSVG()).resolves.toContain('<svg');
    view.finalize();
  });

  it('paints on the figure plate in the plate ink', async () => {
    const out = await svg(fixture(), {
      '--m-figure-ink': '#abcdef',
      '--m-figure-bg': '#102030',
      '--m-figure-accent': '#a1b2c3',
      '--m-color-text': '#ff0000',
    });
    expect(out).toContain('#abcdef');
    expect(out).toContain('#102030');
    expect(out).toContain('#a1b2c3'); // the single-series mark
    expect(out).not.toContain('#ff0000'); // the page's text is not the plate's ink
  });

  it('colours series, ramps and diverging scales from the palette tokens', () => {
    const tokens: Record<string, string> = {
      '--m-div-low': '#000001',
      '--m-div-mid': '#000002',
      '--m-div-high': '#000003',
    };
    for (let i = 1; i <= 8; i++) tokens[`--m-cat-${i}`] = `#10000${i}`;
    for (let i = 1; i <= 5; i++) tokens[`--m-seq-${i}`] = `#20000${i}`;
    const c = themeConfig(tokens) as { range: Record<string, string[]>; mark: object };
    expect(c.range.category).toEqual([1, 2, 3, 4, 5, 6, 7, 8].map((i) => `#10000${i}`));
    expect(c.range.ramp).toEqual([1, 2, 3, 4, 5].map((i) => `#20000${i}`));
    expect(c.range.heatmap).toEqual(c.range.ramp);
    expect(c.range.diverging).toEqual(['#000001', '#000002', '#000003']);
    expect(c.mark).toEqual({ tooltip: { content: 'encoding' } });
    // An incomplete palette is left to Vega's own.
    delete tokens['--m-cat-8'];
    expect((themeConfig(tokens) as { range: object }).range).not.toHaveProperty('category');
  });

  it('keeps an author config over the theme', () => {
    const vega = compileSpec(
      { ...fixture(), config: { background: '#123456' } },
      { '--m-figure-bg': '#ffffff' },
    ) as { background?: string };
    expect(vega.background).toBe('#123456');
  });

  it('reads CSV rows with numbers typed', () => {
    expect(csvRows('a,b\n1,x\n')).toEqual([{ a: 1, b: 'x' }]);
  });
});

describe('chart poster options', () => {
  it('reads scale as a number or text, and falls back to 1', () => {
    expect(parseScale(2)).toBe(2);
    expect(parseScale('1.5')).toBe(1.5);
    for (const bad of [undefined, '', '0', -1, 9, 'two']) expect(parseScale(bad)).toBe(1);
  });
});
