// The chart runtime's logic (`chart@1`), free of the DOM so unit tests render
// the fixture spec in node: resolve the spec's data against the widget's
// sources, compile Vega-Lite, and build a Vega view that never evals and
// never touches the network.
import { compile, type TopLevelSpec } from 'vega-lite';
import { View, parse, loader, type Spec } from 'vega';
import { expressionInterpreter } from 'vega-interpreter';
import { parseCsv, toNumber } from '../csv';
import type { SourceBytes } from '../bridge';

type Json = Record<string, unknown>;
const isObject = (v: unknown): v is Json =>
  typeof v === 'object' && v !== null && !Array.isArray(v);

/** Rows of a CSV/TSV text as objects, with plain decimals turned into numbers. */
export function csvRows(text: string): Json[] {
  const { header, rows } = parseCsv(text);
  return rows.map((r) => {
    const o: Json = {};
    header.forEach((h, i) => {
      const n = toNumber(r[i]);
      o[h] = n !== null ? n : r[i];
    });
    return o;
  });
}

function rowsFor(src: SourceBytes, format: unknown): Json[] {
  const text = new TextDecoder('utf-8').decode(src.bytes);
  const type = isObject(format) && typeof format.type === 'string' ? format.type : '';
  const name = src.name.toLowerCase();
  if (type === 'json' || (type === '' && (name.endsWith('.json') || src.mime.includes('json')))) {
    const v: unknown = JSON.parse(text);
    if (!Array.isArray(v)) throw new Error(`"${src.name}" is not a JSON array`);
    return v.filter(isObject);
  }
  if (type === 'tsv' || name.endsWith('.tsv')) {
    const { header, rows } = parseCsv(text, '\t');
    return rows.map((r) => Object.fromEntries(header.map((h, i) => [h, toNumber(r[i]) ?? r[i]])));
  }
  return csvRows(text);
}

/**
 * Returns the spec with every `data: {url}` replaced by the matching source's
 * rows, so nothing is fetched. A url matches a source by role or by file name
 * (its last path segment). A url that matches none is an error: a runtime has
 * no network and cannot read files.
 */
export function resolveData(spec: unknown, sources: Record<string, SourceBytes>): Json {
  if (!isObject(spec)) throw new Error('the chart spec is not a JSON object');
  const find = (url: string): SourceBytes => {
    const base = url.split('/').pop() ?? url;
    const hit = Object.entries(sources).find(
      ([role, s]) => role !== 'spec' && (role === url || s.name === url || s.name === base),
    );
    if (!hit) throw new Error(`data "${url}" is not one of the widget's sources`);
    return hit[1];
  };
  const walk = (v: unknown): unknown => {
    if (Array.isArray(v)) return v.map(walk);
    if (!isObject(v)) return v;
    const out: Json = {};
    for (const [k, val] of Object.entries(v)) {
      if (k === 'data' && isObject(val) && typeof val.url === 'string') {
        const rest: Json = { ...val };
        delete rest.url;
        delete rest.format;
        out[k] = { ...rest, values: rowsFor(find(val.url), val.format) };
      } else out[k] = walk(val);
    }
    return out;
  };
  return walk(spec) as Json;
}

export interface Tokens {
  [name: string]: string;
}

/** A Vega-Lite `config` from the theme tokens, so the chart matches the paper. */
export function themeConfig(tokens: Tokens): Json {
  const text = tokens['--m-color-text'];
  const muted = tokens['--m-color-muted'];
  const rule = tokens['--m-color-rule'];
  const accent = tokens['--m-color-accent'];
  const font = tokens['--m-font-body'];
  const axis: Json = {};
  if (muted) Object.assign(axis, { gridColor: muted, gridOpacity: 0.3 });
  if (rule) Object.assign(axis, { domainColor: rule, tickColor: rule });
  if (text) Object.assign(axis, { labelColor: text, titleColor: text });
  if (font) Object.assign(axis, { labelFont: font, titleFont: font });
  const config: Json = { axis };
  const bg = tokens['--m-figure-bg'] ?? tokens['--m-color-bg'];
  config.background = bg && bg !== 'transparent' ? bg : null;
  if (text) config.legend = { labelColor: text, titleColor: text };
  if (text) config.title = { color: text };
  if (accent) {
    config.mark = { color: accent };
    config.range = { category: { scheme: 'tableau10' } };
  }
  return config;
}

export interface Size {
  width: number;
  height: number;
}

const COMPOSED = ['facet', 'repeat', 'concat', 'hconcat', 'vconcat'];

/** Compiles Vega-Lite to Vega, filling the frame when the spec pins no size. */
export function compileSpec(spec: Json, tokens: Tokens, size?: Size): Spec {
  const vl: Json = { ...spec };
  const composed = COMPOSED.some((k) => k in vl);
  if (size && !composed) {
    if (vl.width === undefined) vl.width = Math.max(50, Math.floor(size.width - 80));
    if (vl.height === undefined) vl.height = Math.max(50, Math.floor(size.height - 80));
    if (vl.autosize === undefined) vl.autosize = { type: 'fit', contains: 'padding' };
  }
  const config = { ...themeConfig(tokens), ...(isObject(vl.config) ? vl.config : {}) };
  return compile(vl as unknown as TopLevelSpec, { config }).spec;
}

/** A loader that refuses everything: a widget has no network (connect-src 'none'). */
function offlineLoader() {
  const l = loader();
  l.load = () => Promise.reject(new Error('this widget cannot load files or URLs'));
  l.sanitize = (uri: string) =>
    uri.startsWith('data:')
      ? Promise.resolve({ href: uri })
      : Promise.reject(new Error('this widget cannot load files or URLs'));
  return l;
}

/**
 * A Vega view for the compiled spec. `ast: true` plus the interpreter means
 * expressions run without `eval` or `new Function`, which the sandbox's CSP
 * forbids.
 */
export function createView(vega: Spec, renderer: 'none' | 'svg' | 'canvas' = 'none'): View {
  const runtime = parse(vega, undefined, { ast: true });
  return new View(runtime, {
    expr: expressionInterpreter,
    loader: offlineLoader(),
    renderer,
  });
}

/** Parses the spec source bytes. */
export function parseSpec(src: SourceBytes): unknown {
  return JSON.parse(new TextDecoder('utf-8').decode(src.bytes));
}

/** The `scale` option: snapshot pixels per CSS pixel, above 0 and at most 8; 1 otherwise. */
export function parseScale(v: unknown): number {
  const n = typeof v === 'number' ? v : typeof v === 'string' ? Number(v) : NaN;
  return Number.isFinite(n) && n > 0 && n <= 8 ? n : 1;
}
