// The chart runtime (`chart@1`): renders the widget's Vega-Lite spec, with the
// data files it names, from bytes delivered in `init`. See ./core.ts. Option:
// `scale`, the snapshot's pixels per CSS pixel. Marks hover a tooltip (see
// ./tooltip.ts).
import { applyTheme, startBridge, status, type Init, type Theme } from '../bridge';
import { compileSpec, createView, parseScale, parseSpec, resolveData } from './core';
import { makeTip } from './tooltip';
import type { View } from 'vega';

const host = document.getElementById('chart') as HTMLElement;
const tip = makeTip(document.getElementById('tip') as HTMLElement);
host.addEventListener('mouseleave', tip.hide);
let spec: Record<string, unknown> | null = null;
let theme: Theme | null = null;
let view: View | null = null;
let scale = 1;
// The plot keeps the shape the frame had when the widget started: the
// poster's in the reader, the poster box itself when rendering a poster.
// The bound inputs flow under it and the frame grows to hold them.
let aspect = 0;
let drawnWidth = 0;
const MIN_PLOT = 160;

function fail(message: string): void {
  host.textContent = '';
  const d = document.createElement('div');
  d.className = 'msg';
  d.textContent = message;
  host.append(d);
  status('error', message);
}

async function draw(): Promise<boolean> {
  if (!spec || !theme) return false;
  tip.hide();
  view?.finalize();
  host.textContent = '';
  const width = host.clientWidth;
  drawnWidth = width;
  const size = { width, height: Math.max(MIN_PLOT, Math.round(width / aspect)) };
  view = createView(compileSpec(spec, theme.tokens, size), 'svg');
  view.tooltip(tip.handler);
  view.initialize(host);
  await view.runAsync();
  return true;
}

async function init(msg: Init): Promise<void> {
  const root = document.documentElement;
  if (!aspect) aspect = root.clientWidth / Math.max(root.clientHeight, 1) || 4 / 3;
  theme = msg.theme;
  scale = parseScale(msg.options.scale);
  applyTheme(theme);
  host.setAttribute('aria-label', msg.alt);
  status('loading');
  try {
    const src = msg.sources.spec;
    if (!src) return fail('the chart has no "spec" source');
    spec = resolveData(parseSpec(src), msg.sources);
    await draw();
    // A snapshot can be asked for as soon as `loaded` arrives: have one ready.
    await capture();
    status('loaded');
  } catch (err) {
    fail(err instanceof Error ? err.message : 'the chart failed to render');
  }
}

function snapshot(): string | null {
  // toCanvas is async; the bridge wants a string now, so serve the last frame.
  return lastPng;
}
let lastPng: string | null = null;
async function capture(): Promise<void> {
  if (view) lastPng = await view.toImageURL('png', scale);
}

startBridge({
  onInit: (m) => void init(m),
  onTheme: (t) => {
    theme = t;
    tip.hide();
    applyTheme(t);
    void draw().then(capture);
  },
  onSnapshot: snapshot,
});
let resizeTimer = 0;
window.addEventListener('resize', () => {
  // Only a new width redraws: the height is ours, and follows from it.
  if (!spec || host.clientWidth === drawnWidth) return;
  window.clearTimeout(resizeTimer);
  resizeTimer = window.setTimeout(() => void draw().then(capture), 100);
});
