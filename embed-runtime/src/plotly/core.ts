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

/**
 * A Plotly template from the theme tokens. It replaces the figure's own
 * template (Python writes its default one into every figure), so the plot
 * matches the paper; anything set in the figure's layout still wins.
 */
export function themeTemplate(tokens: Record<string, string>): Json {
  const bg = tokens['--m-figure-bg'];
  const ink = tokens['--m-figure-ink'];
  const font = tokens['--m-font-body'];
  const axis: Json = ink
    ? { color: ink, linecolor: ink, gridcolor: mix(ink, 0.12), zerolinecolor: mix(ink, 0.3) }
    : {};
  const layout: Json = {
    paper_bgcolor: bg ?? 'transparent',
    plot_bgcolor: bg ?? 'transparent',
    font: { ...(ink ? { color: ink } : {}), ...(font ? { family: font } : {}) },
    xaxis: axis,
    yaxis: axis,
  };
  const cat = CATEGORICAL.map((n) => tokens[n]);
  if (cat.every((c) => c)) layout.colorway = cat;
  return { layout };
}

/** A `#rrggbb` token at `alpha` as `rgba()` (Plotly's colour parser has no
 *  color-mix); any other value as it is. */
export function mix(color: string, alpha: number): string {
  const m = /^#([0-9a-f]{2})([0-9a-f]{2})([0-9a-f]{2})$/i.exec(color.trim());
  if (!m) return color;
  const [r, g, b] = m.slice(1).map((h) => parseInt(h, 16));
  return `rgba(${r}, ${g}, ${b}, ${alpha})`;
}

/** The figure with the theme's template in place of its own. */
export function themed(fig: Figure, tokens: Record<string, string>): Figure {
  return { data: fig.data, layout: { ...fig.layout, template: themeTemplate(tokens) } };
}

/** The `scale` option: above 0 and at most 8, else 1. */
export function parseScale(v: unknown): number {
  const n = typeof v === 'number' ? v : typeof v === 'string' ? Number(v) : NaN;
  return Number.isFinite(n) && n > 0 && n <= 8 ? n : 1;
}
