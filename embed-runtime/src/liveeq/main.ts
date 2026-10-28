// The live-equation widget (`liveeq@1`): a `liveequation` with mode=widget,
// run in a frame by the reader, which mounts it from the manifest's
// `equations` (it is not a widget kind an author places). `init` carries the
// equation's manifest record and its MathML as options; the widget shows the
// MathML on its plate with a slider, a number box and a reset per equation,
// and recomputes the results with the formula interpreter (../expr.ts) the
// inline host uses.
import { applyTheme, startBridge, status, type Init } from '../bridge';
import { evaluate, format, parse, type Expr } from '../expr';

interface Param {
  name: string;
  default: number;
  min: number;
  max: number;
  step: number;
  places: number;
}

interface Result {
  n: number;
  places: number;
  formula: string;
}

const host = document.getElementById('equation') as HTMLElement;
const controls = document.getElementById('controls') as HTMLElement;
const live = document.getElementById('live') as HTMLElement;

const num = (v: unknown): number | null => (typeof v === 'number' && Number.isFinite(v) ? v : null);

function params(v: unknown): Param[] | null {
  if (!Array.isArray(v) || !v.length) return null;
  const out: Param[] = [];
  for (const p of v as Record<string, unknown>[]) {
    const [d, lo, hi, step, places] = [p.default, p.min, p.max, p.step, p.places].map(num);
    if (typeof p.name !== 'string' || d === null || lo === null || hi === null) return null;
    if (step === null || step <= 0 || places === null) return null;
    out.push({ name: p.name, default: d, min: lo, max: hi, step, places });
  }
  return out;
}

function results(v: unknown): Result[] {
  if (!Array.isArray(v)) return [];
  return (v as Record<string, unknown>[]).flatMap((r) =>
    typeof r.n === 'number' && typeof r.places === 'number' && typeof r.formula === 'string'
      ? [{ n: r.n, places: r.places, formula: r.formula }]
      : [],
  );
}

/** The equation's <math> element alone, without any event attribute. */
function mathOf(markup: string): Element | null {
  const doc = new DOMParser().parseFromString(`<body>${markup}</body>`, 'text/html');
  const math = doc.querySelector('math');
  if (!math) return null;
  for (const el of [math, ...math.querySelectorAll('*')]) {
    for (const a of [...el.attributes]) if (/^on/i.test(a.name)) el.removeAttribute(a.name);
  }
  return document.importNode(math, true);
}

const shown = (v: number, places: number): string => (format(v, places) ?? '?').replace(/^-/, '−');

function start(m: Init): void {
  const eq = m.options.equation as Record<string, unknown> | undefined;
  const found = params(eq?.params);
  const mathml = typeof m.options.mathml === 'string' ? mathOf(m.options.mathml) : null;
  if (!eq || !found || !mathml) {
    status('error', 'the equation record or its MathML is missing');
    return;
  }
  const ps: Param[] = found;
  const math: Element = mathml;
  host.replaceChildren(math);
  controls.setAttribute('aria-label', m.alt || 'Live equation');
  const names = ps.map((p) => p.name);
  const rs = results(eq.results).map((r) => {
    const parsed = parse(r.formula, names);
    return { ...r, expr: parsed.ok ? parsed.expr : (null as Expr | null) };
  });
  const values: Record<string, number> = Object.fromEntries(ps.map((p) => [p.name, p.default]));
  const inputs = new Map<string, HTMLInputElement[]>();

  function update(announce: boolean): void {
    for (const p of ps) {
      for (const t of math.querySelectorAll(`.m-param-${p.name}`)) {
        t.textContent = shown(values[p.name], p.places);
      }
      for (const i of inputs.get(p.name) ?? []) i.value = String(values[p.name]);
    }
    const said: string[] = [];
    for (const r of rs) {
      if (!r.expr) continue;
      const text = shown(evaluate(r.expr, values), r.places);
      for (const t of math.querySelectorAll(`.m-result-${r.n}`)) t.textContent = text;
      said.push(text);
    }
    if (announce) live.textContent = said.join(', ');
  }

  for (const p of ps) {
    const row = document.createElement('label');
    const name = document.createElement('span');
    name.className = 'name';
    name.textContent = p.name;
    const range = document.createElement('input');
    range.type = 'range';
    const box = document.createElement('input');
    box.type = 'number';
    for (const i of [range, box]) {
      i.min = String(p.min);
      i.max = String(p.max);
      i.step = String(p.step);
    }
    range.setAttribute('aria-label', p.name);
    box.setAttribute('aria-label', `${p.name} value`);
    range.addEventListener('input', () => {
      values[p.name] = Number(range.value);
      update(true);
    });
    box.addEventListener('change', () => {
      const v = Number(box.value);
      if (box.value !== '' && Number.isFinite(v))
        values[p.name] = Math.min(p.max, Math.max(p.min, v));
      update(true);
    });
    inputs.set(p.name, [range, box]);
    row.append(name, range, box);
    controls.append(row);
  }
  const reset = document.createElement('button');
  reset.type = 'button';
  reset.textContent = 'Reset';
  reset.addEventListener('click', () => {
    for (const p of ps) values[p.name] = p.default;
    update(true);
  });
  controls.append(reset);
  update(false);
  applyTheme(m.theme);
  status('loaded');
}

startBridge({
  onInit: start,
  onTheme: applyTheme,
  onSnapshot: () => null,
});
