// The plotly runtime's pure parts: reading a figure and theming it.

export type Json = Record<string, unknown>;
export interface Figure {
  data: Json[];
  layout: Json;
}

/** The trace types ./plotly.ts registers (scatter is in plotly's core). */
export const TRACES: ReadonlySet<string> = new Set([
  'bar',
  'barpolar',
  'box',
  'candlestick',
  'carpet',
  'contour',
  'contourcarpet',
  'funnel',
  'funnelarea',
  'heatmap',
  'histogram',
  'histogram2d',
  'histogram2dcontour',
  'icicle',
  'indicator',
  'ohlc',
  'parcats',
  'pie',
  'sankey',
  'scatter',
  'scattercarpet',
  'scatterpolar',
  'scattersmith',
  'scatterternary',
  'sunburst',
  'table',
  'treemap',
  'violin',
  'waterfall',
]);

const isObject = (v: unknown): v is Json =>
  typeof v === 'object' && v !== null && !Array.isArray(v);

/** A figure from JSON text: an object whose `data` is an array of traces. */
export function parseFigure(text: string): Figure {
  let v: unknown;
  try {
    v = JSON.parse(text);
  } catch {
    throw new Error('the figure is not JSON');
  }
  if (!isObject(v) || !Array.isArray(v.data) || !v.data.every(isObject)) {
    throw new Error('the figure needs a "data" array of traces');
  }
  const data = v.data.map(webglAsSvg);
  for (const t of data) {
    const type = typeof t.type === 'string' ? t.type : 'scatter';
    if (!TRACES.has(type)) {
      throw new Error(
        `trace type "${type}" is not supported: WebGL, 3D and map traces cannot run in a paper bundle`,
      );
    }
  }
  if (v.layout !== undefined && !isObject(v.layout)) {
    throw new Error('the figure\'s "layout" is not an object');
  }
  return { data, layout: isObject(v.layout) ? v.layout : {} };
}

/** scattergl as scatter: Plotly Express picks WebGL for large data, and the
 *  two share their attributes. */
function webglAsSvg(t: Json): Json {
  return t.type === 'scattergl' ? { ...t, type: 'scatter' } : t;
}

const CATEGORICAL = [1, 2, 3, 4, 5, 6, 7, 8].map((i) => `--m-cat-${i}`);
const SEQUENTIAL = [1, 2, 3, 4, 5].map((i) => `--m-seq-${i}`);
const DIVERGING = ['--m-div-low', '--m-div-mid', '--m-div-high'];

type Scale = [number, string][];

/** The tokens `names` as an evenly spaced colourscale, or none when any is
 *  missing. */
function scale(tokens: Record<string, string>, names: string[]): Scale | undefined {
  const cs = names.map((n) => tokens[n]);
  if (!cs.every((c) => c)) return undefined;
  return cs.map((c, i) => [i / (cs.length - 1), c]);
}

/** Trace types whose own colourscale is not automatic: the template gives
 *  them the paper's sequential one. */
const SCALED = ['heatmap', 'contour', 'histogram2d', 'histogram2dcontour', 'contourcarpet'];

/**
 * A Plotly template from the theme tokens. It replaces the figure's own
 * template (Python writes its default one into every figure), so the plot
 * matches the paper; anything set in the figure's layout still wins.
 */
export function themeTemplate(tokens: Record<string, string>): Json {
  const bg = tokens['--m-figure-bg'];
  const surface = tokens['--m-figure-surface'];
  const ink = tokens['--m-figure-ink'];
  const font = tokens['--m-font-body'];
  // What Python's default template sets beyond colour, so a figure lays
  // out as it did there: margins that fit the tick labels, numbers in
  // strings read as categories, closest-point hover.
  const axis: Json = {
    automargin: true,
    ticks: '',
    title: { standoff: 15 },
    ...(ink
      ? { color: ink, linecolor: ink, gridcolor: mix(ink, 0.12), zerolinecolor: mix(ink, 0.3) }
      : {}),
  };
  const layout: Json = {
    autotypenumbers: 'strict',
    hovermode: 'closest',
    title: { x: 0.05 },
    coloraxis: { colorbar: { outlinewidth: 0, ticks: '' } },
    ...(ink
      ? {
          shapedefaults: { line: { color: ink } },
          annotationdefaults: { arrowcolor: ink, arrowhead: 0, arrowwidth: 1 },
        }
      : {}),
    paper_bgcolor: bg ?? 'transparent',
    plot_bgcolor: bg ?? 'transparent',
    font: { ...(ink ? { color: ink } : {}), ...(font ? { family: font } : {}) },
    xaxis: axis,
    yaxis: axis,
    polar: { bgcolor: bg ?? 'transparent', radialaxis: axis, angularaxis: axis },
    ternary: { bgcolor: bg ?? 'transparent', aaxis: axis, baxis: axis, caxis: axis },
  };
  const cat = CATEGORICAL.map((n) => tokens[n]);
  if (cat.every((c) => c)) layout.colorway = cat;
  const tip = ['--m-tooltip-bg', '--m-tooltip-fg', '--m-tooltip-border'].map((n) => tokens[n]);
  if (tip.every((c) => c)) {
    layout.hoverlabel = {
      align: 'left',
      bgcolor: tip[0],
      bordercolor: tip[2],
      font: { color: tip[1] },
    };
  }
  const seq = scale(tokens, SEQUENTIAL);
  const div = scale(tokens, DIVERGING);
  const data: Record<string, Json[]> = {};
  if (seq && div) {
    const minus: Scale = seq.map(([at], i) => [at, seq[seq.length - 1 - i][1]]);
    layout.colorscale = { sequential: seq, sequentialminus: minus, diverging: div };
    for (const t of SCALED) data[t] = [{ colorscale: seq }];
  }
  if (bg && surface && ink) {
    const line = { color: mix(ink, 0.2) };
    data.table = [
      { header: { fill: { color: surface }, line }, cells: { fill: { color: bg }, line } },
    ];
  }
  return { layout, data };
}

/** A `#rrggbb` token at `alpha` as `rgba()` (Plotly's colour parser has no
 *  color-mix); any other value as it is. */
export function mix(color: string, alpha: number): string {
  const m = /^#([0-9a-f]{2})([0-9a-f]{2})([0-9a-f]{2})$/i.exec(color.trim());
  if (!m) return color;
  const [r, g, b] = m.slice(1).map((h) => parseInt(h, 16));
  return `rgba(${r}, ${g}, ${b}, ${alpha})`;
}

/**
 * What the figure's own template gave it, by value: Plotly Express writes
 * the template's colorway into each trace and its colourscales into the
 * layout, so they look like the author's choices. Each maps to the theme's
 * counterpart; a value the template does not hold is the author's.
 */
function adoptions(own: unknown, theme: Json): Map<string, unknown> {
  const out = new Map<string, unknown>();
  const ownLayout = isObject(own) && isObject(own.layout) ? own.layout : {};
  const themeLayout = theme.layout as Json;
  const from = Array.isArray(ownLayout.colorway) ? ownLayout.colorway : [];
  const to = Array.isArray(themeLayout.colorway) ? themeLayout.colorway : [];
  if (to.length) {
    from.forEach((c, i) => {
      if (typeof c === 'string') out.set(`c:${c.toLowerCase()}`, to[i % to.length]);
    });
  }
  const scales = isObject(ownLayout.colorscale) ? ownLayout.colorscale : {};
  const ours = isObject(themeLayout.colorscale) ? themeLayout.colorscale : {};
  // Plotly's own templates give sequentialminus the sequential scale, so a
  // scale both name maps as sequential: the first match wins.
  for (const k of ['sequential', 'sequentialminus', 'diverging']) {
    const key = `s:${JSON.stringify(scales[k])}`;
    if (Array.isArray(scales[k]) && ours[k] && !out.has(key)) out.set(key, ours[k]);
  }
  return out;
}

/** `v` with every value `map` holds replaced (see `adoptions`). */
function adopt(v: unknown, map: Map<string, unknown>): unknown {
  if (typeof v === 'string') return map.get(`c:${v.toLowerCase()}`) ?? v;
  if (Array.isArray(v)) {
    const s = map.get(`s:${JSON.stringify(v)}`);
    return s ?? v.map((x) => adopt(x, map));
  }
  if (isObject(v)) {
    return Object.fromEntries(Object.entries(v).map(([k, x]) => [k, adopt(x, map)]));
  }
  return v;
}

/** How a figure is styled: `paper` (the default) puts the theme's template
 *  in place of its own; `figure` keeps its own, as Python drew it. */
export type Template = 'paper' | 'figure';

/** The figure with the theme's template in place of its own, and what its
 *  own template wrote into it mapped to the theme; as it is for `figure`. */
export function themed(
  fig: Figure,
  tokens: Record<string, string>,
  template: Template = 'paper',
): Figure {
  if (template === 'figure') return fig;
  const theme = themeTemplate(tokens);
  const { template: own, ...layout } = fig.layout;
  const map = adoptions(own, theme);
  return {
    data: fig.data.map((t) => adopt(t, map) as Json),
    layout: { ...(adopt(layout, map) as Json), template: theme },
  };
}

/** The `template` option: `figure`, else `paper`. */
export function parseTemplate(v: unknown): Template {
  return v === 'figure' ? 'figure' : 'paper';
}

/** The `scale` option: above 0 and at most 8, else 1. */
export function parseScale(v: unknown): number {
  const n = typeof v === 'number' ? v : typeof v === 'string' ? Number(v) : NaN;
  return Number.isFinite(n) && n > 0 && n <= 8 ? n : 1;
}
