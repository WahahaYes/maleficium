import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import { compileSpec, createView, csvRows, parseSpec, resolveData } from './core';
import type { SourceBytes } from '../bridge';

const bytes = (s: string) => new TextEncoder().encode(s).buffer as ArrayBuffer;
const src = (name: string, text: string, mime = ''): SourceBytes => ({
  name,
  mime,
  sha256: '',
  bytes: bytes(text),
});
// The ip.5 fixture spec: one inline value, a bar mark.
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

  it('turns theme tokens into axis and background colours', async () => {
    const out = await svg(fixture(), { '--m-color-text': '#abcdef', '--m-figure-bg': '#102030' });
    expect(out).toContain('#abcdef');
    expect(out).toContain('#102030');
  });

  it('reads CSV rows with numbers typed', () => {
    expect(csvRows('a,b\n1,x\n')).toEqual([{ a: 1, b: 'x' }]);
  });
});
