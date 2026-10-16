import { describe, expect, it } from 'vitest';
import { mix, parseFigure, parseScale, themed, themeTemplate } from './core';

describe('plotly figure', () => {
  it('reads what fig.write_json() writes', () => {
    const f = parseFigure(
      '{"data":[{"type":"scatter","x":[1,2],"y":[3,4]}],"layout":{"title":{"text":"t"},"template":{"data":{}}}}',
    );
    expect(f.data).toHaveLength(1);
    expect(f.layout.title).toEqual({ text: 't' });
  });

  it('refuses what is not a figure', () => {
    expect(() => parseFigure('not json')).toThrow('not JSON');
    expect(() => parseFigure('{"layout":{}}')).toThrow('"data" array');
    expect(() => parseFigure('{"data":[1]}')).toThrow('"data" array');
    expect(() => parseFigure('{"data":[],"layout":[]}')).toThrow('"layout"');
  });

  it("replaces the figure's template with the theme and keeps its own layout", () => {
    const tokens: Record<string, string> = {
      '--m-figure-bg': '#ffffff',
      '--m-figure-ink': '#102030',
      '--m-font-body': 'Inter',
    };
    for (let i = 1; i <= 8; i++) tokens[`--m-cat-${i}`] = `#00000${i}`;
    const f = themed(parseFigure('{"data":[],"layout":{"template":{"x":1},"title":"t"}}'), tokens);
    expect(f.layout.title).toBe('t');
    const t = (f.layout.template as { layout: Record<string, unknown> }).layout;
    expect(t.paper_bgcolor).toBe('#ffffff');
    expect(t.font).toEqual({ color: '#102030', family: 'Inter' });
    expect((t.colorway as string[])[7]).toBe('#000008');
    expect((t.xaxis as Record<string, string>).gridcolor).toBe('rgba(16, 32, 48, 0.12)');
  });

  it('leaves colorway to plotly when the palette is incomplete', () => {
    expect(themeTemplate({ '--m-cat-1': '#000000' }).layout).not.toHaveProperty('colorway');
  });

  it('mixes only #rrggbb and bounds scale', () => {
    expect(mix('rebeccapurple', 0.5)).toBe('rebeccapurple');
    expect(parseScale('2')).toBe(2);
    expect(parseScale(9)).toBe(1);
  });
});

describe('plotly traces', () => {
  it('registers exactly the supported types', async () => {
    const { readFileSync } = await import('node:fs');
    const src = readFileSync('embed-runtime/src/plotly/plotly.ts', 'utf8');
    const registered = [...src.matchAll(/from 'plotly\.js\/lib\/([a-z0-9]+)'/g)]
      .map((m) => m[1])
      .filter((n) => n !== 'core');
    const { TRACES } = await import('./core');
    expect(new Set([...registered, 'scatter'])).toEqual(TRACES);
  });

  it('draws scattergl as scatter and refuses WebGL, 3D and maps', () => {
    const f = parseFigure('{"data":[{"type":"scattergl","x":[1],"y":[1]},{"y":[2]}]}');
    expect(f.data[0].type).toBe('scatter');
    for (const t of ['scatter3d', 'scattermap', 'splom']) {
      expect(() => parseFigure(`{"data":[{"type":"${t}"}]}`)).toThrow(`"${t}" is not supported`);
    }
  });
});
