// The plotly runtime (`plotly@1`): draws the widget's Plotly figure, delivered
// as bytes in `init`, with Plotly's own hover. See ./core.ts. Options: `scale`,
// the snapshot's pixels per CSS pixel, and `template` (paper or figure).
import Plotly from './plotly';
import { applyTheme, decodeText, startBridge, status, type Init, type Theme } from '../bridge';
import { parseFigure, parseScale, parseTemplate, themed, type Figure, type Template } from './core';

const host = document.getElementById('plot') as HTMLElement;
let figure: Figure | null = null;
let theme: Theme | null = null;
let scale = 1;
let template: Template = 'paper';
// The plot keeps the frame's shape at start: the poster's in the reader, the
// poster box when rendering one.
let aspect = 0;
let drawnWidth = 0;
let lastPng: string | null = null;

function fail(message: string): void {
  host.textContent = '';
  const d = document.createElement('div');
  d.className = 'msg';
  d.textContent = message;
  host.append(d);
  status('error', message);
}

async function draw(): Promise<void> {
  if (!figure || !theme) return;
  const width = host.clientWidth;
  drawnWidth = width;
  const fig = themed(figure, theme.tokens, template);
  const layout = { ...fig.layout, width, height: Math.round(width / aspect), autosize: false };
  await Plotly.react(host, fig.data, layout, { displaylogo: false, responsive: false });
  lastPng = await Plotly.toImage(host, { format: 'png', width, height: layout.height, scale });
}

async function init(msg: Init): Promise<void> {
  const root = document.documentElement;
  if (!aspect) aspect = root.clientWidth / Math.max(root.clientHeight, 1) || 4 / 3;
  theme = msg.theme;
  scale = parseScale(msg.options.scale);
  template = parseTemplate(msg.options.template);
  applyTheme(theme);
  host.setAttribute('aria-label', msg.alt);
  status('loading');
  try {
    const src = msg.sources.figure;
    if (!src) return fail('the plot has no "figure" source');
    figure = parseFigure(decodeText(src));
    host.textContent = '';
    await draw();
    status('loaded');
  } catch (err) {
    fail(err instanceof Error ? err.message : 'the figure failed to render');
  }
}

startBridge({
  onInit: (m) => void init(m),
  onTheme: (t) => {
    theme = t;
    applyTheme(t);
    void draw();
  },
  // toImage is async; the bridge wants a string now, so serve the last frame.
  onSnapshot: () => lastPng,
});
let resizeTimer = 0;
window.addEventListener('resize', () => {
  if (!figure || host.clientWidth === drawnWidth) return;
  window.clearTimeout(resizeTimer);
  resizeTimer = window.setTimeout(() => void draw(), 100);
});
