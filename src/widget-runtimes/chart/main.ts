// The chart runtime (`chart@1`): renders the widget's Vega-Lite spec, with the
// data files it names, from bytes delivered in `init`. See ./core.ts. Option:
// `scale`, the snapshot's pixels per CSS pixel.
import { applyTheme, startBridge, status, type Init, type Theme } from '../bridge';
import { compileSpec, createView, parseScale, parseSpec, resolveData } from './core';
import type { View } from 'vega';

const host = document.getElementById('chart') as HTMLElement;
let spec: Record<string, unknown> | null = null;
let theme: Theme | null = null;
let view: View | null = null;
let scale = 1;

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
  view?.finalize();
  host.textContent = '';
  const size = { width: host.clientWidth, height: host.clientHeight };
  view = createView(compileSpec(spec, theme.tokens, size), 'svg');
  view.initialize(host);
  await view.runAsync();
  return true;
}

async function init(msg: Init): Promise<void> {
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
    applyTheme(t);
    void draw().then(capture);
  },
  onSnapshot: snapshot,
});
let resizeTimer = 0;
window.addEventListener('resize', () => {
  window.clearTimeout(resizeTimer);
  resizeTimer = window.setTimeout(() => void draw().then(capture), 100);
});
