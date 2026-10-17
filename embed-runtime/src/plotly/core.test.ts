import { describe, expect, it } from 'vitest';
import { mix, parseFigure, parseScale, parseTemplate, themed, themeTemplate } from './core';

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

  const full = (): Record<string, string> => {
    const t: Record<string, string> = {
      '--m-figure-bg': '#ffffff',
      '--m-figure-surface': '#f0f0f0',
      '--m-figure-ink': '#102030',
      '--m-tooltip-bg': '#fefefe',
      '--m-tooltip-fg': '#111111',
      '--m-tooltip-border': '#cccccc',
      '--m-div-low': '#0000aa',
      '--m-div-mid': '#eeeeee',
      '--m-div-high': '#aa0000',
    };
    for (let i = 1; i <= 8; i++) t[`--m-cat-${i}`] = `#00000${i}`;
    for (let i = 1; i <= 5; i++) t[`--m-seq-${i}`] = `#1${i}1${i}1${i}`;
    return t;
  };

  it('themes colourscales, hover labels, polar axes and tables', () => {
    const t = themeTemplate(full());
    const l = t.layout as Record<string, Record<string, unknown>>;
    expect(l.colorscale.sequential).toEqual([
      [0, '#111111'],
      [0.25, '#121212'],
      [0.5, '#131313'],
      [0.75, '#141414'],
      [1, '#151515'],
    ]);
    expect((l.colorscale.sequentialminus as unknown[])[0]).toEqual([0, '#151515']);
    expect(l.colorscale.diverging).toEqual([
      [0, '#0000aa'],
      [0.5, '#eeeeee'],
      [1, '#aa0000'],
    ]);
    expect(l.autotypenumbers).toBe('strict');
    expect(l.yaxis.automargin).toBe(true);
    expect(l.hoverlabel).toEqual({
      align: 'left',
      bgcolor: '#fefefe',
      bordercolor: '#cccccc',
      font: { color: '#111111' },
    });
    expect((l.polar.radialaxis as Record<string, string>).gridcolor).toBe('rgba(16, 32, 48, 0.12)');
    const d = t.data as Record<string, Record<string, unknown>[]>;
    expect(d.heatmap[0].colorscale).toBe(l.colorscale.sequential);
    expect(d.table[0].header).toEqual({
      fill: { color: '#f0f0f0' },
      line: { color: 'rgba(16, 32, 48, 0.2)' },
    });
  });

  it('maps what a Plotly Express template wrote into the figure to the theme', () => {
    // As px.bar(color=...) and px.density_heatmap() write them.
    const plasma = [
      [0, '#0d0887'],
      [1, '#f0f921'],
    ];
    const fig = parseFigure(
      JSON.stringify({
        data: [
          { type: 'bar', marker: { color: '#636efa' }, y: [1] },
          { type: 'bar', marker: { color: '#EF553B', line: { color: '#123456' } }, y: [2] },
        ],
        layout: {
          coloraxis: { colorscale: plasma },
          template: {
            layout: {
              colorway: ['#636efa', '#EF553B'],
              colorscale: {
                sequential: plasma,
                sequentialminus: plasma,
                diverging: [[0, '#8e0152']],
              },
            },
          },
        },
      }),
    );
    const f = themed(fig, full());
    const marker = (i: number) => f.data[i].marker as Record<string, unknown>;
    expect(marker(0).color).toBe('#000001');
    expect(marker(1).color).toBe('#000002');
    // A colour the template does not hold is the author's.
    expect(marker(1).line).toEqual({ color: '#123456' });
    expect((f.layout.coloraxis as Record<string, unknown>).colorscale).toEqual(
      (themeTemplate(full()).layout as Record<string, Record<string, unknown>>).colorscale
        .sequential,
    );
  });

  it('keeps the figure as Python drew it with template=figure', () => {
    const fig = parseFigure(
      '{"data":[{"type":"bar","marker":{"color":"#636efa"}}],"layout":{"template":{"layout":{"colorway":["#636efa"]}}}}',
    );
    expect(themed(fig, full(), 'figure')).toBe(fig);
    expect(parseTemplate('figure')).toBe('figure');
    expect(parseTemplate(undefined)).toBe('paper');
    expect(parseTemplate('nonsense')).toBe('paper');
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
