// The table runtime (`table@1`): a sortable, filterable view of the widget's
// CSV source, delivered as bytes in `init`. See ./model.ts for the logic.
import { applyTheme, decodeText, startBridge, status, type Init } from '../bridge';
import {
  buildTable,
  formatCell,
  formatsByColumn,
  resolveOptions,
  visibleColumns,
  visibleRows,
  type Column,
  type Format,
  type SortKey,
  type Table,
} from './model';

const app = document.getElementById('app') as HTMLElement;
const scroll = document.querySelector('.scroll') as HTMLElement;
const filterInput = document.getElementById('filter') as HTMLInputElement;
const count = document.getElementById('count') as HTMLElement;

let table: Table | null = null;
let columns: Column[] = [];
let formats = new Map<number, Format>();
let maxRows = 0;
let sort: SortKey | null = null;
let shown: number[] = [];

function fail(message: string): void {
  scroll.textContent = '';
  const d = document.createElement('div');
  d.className = 'msg';
  d.textContent = message;
  scroll.append(d);
  count.textContent = '';
  status('error', message);
}

function render(): void {
  if (!table) return;
  shown = visibleRows(table, { sort, filter: filterInput.value });
  const rows = maxRows > 0 ? shown.slice(0, maxRows) : shown;
  const t = document.createElement('table');
  const head = t.createTHead().insertRow();
  for (const c of columns) {
    const th = document.createElement('th');
    th.textContent = c.name;
    th.scope = 'col';
    th.tabIndex = 0;
    if (c.numeric) th.className = 'num';
    const active = sort && sort.column === c.name;
    th.setAttribute('aria-sort', active ? (sort!.descending ? 'descending' : 'ascending') : 'none');
    const toggle = () => {
      sort = !active
        ? { column: c.name, descending: false }
        : !sort!.descending
          ? { column: c.name, descending: true }
          : null;
      render();
    };
    th.addEventListener('click', toggle);
    th.addEventListener('keydown', (e) => {
      if (e.key === 'Enter' || e.key === ' ') {
        e.preventDefault();
        toggle();
      }
    });
    head.append(th);
  }
  const body = t.createTBody();
  for (const i of rows) {
    const tr = body.insertRow();
    for (const c of columns) {
      const td = tr.insertCell();
      td.textContent = formatCell(table.rows[i][c.index], formats.get(c.index));
      if (c.numeric) td.className = 'num';
    }
  }
  scroll.textContent = '';
  scroll.append(t);
  count.textContent = `${shown.length} of ${table.rows.length} rows`;
}

function snapshot(): string | null {
  if (!table) return null;
  const cv = document.createElement('canvas');
  const w = Math.max(1, Math.round(app.clientWidth));
  const h = Math.max(1, Math.round(app.clientHeight));
  cv.width = w;
  cv.height = h;
  const g = cv.getContext('2d');
  if (!g) return null;
  // The live view's colours: the plate, its surface for the header, its ink.
  const cs = getComputedStyle(document.documentElement);
  const token = (n: string) => cs.getPropertyValue(n).trim();
  const plate = token('--m-figure-bg');
  const ink = token('--m-figure-ink');
  if (plate) {
    g.fillStyle = plate;
    g.fillRect(0, 0, w, h);
  }
  const surface = token('--m-figure-surface');
  if (surface) {
    g.fillStyle = surface;
    g.fillRect(0, 0, w, 26);
  }
  if (ink) g.fillStyle = ink;
  const font = token('--m-font-body') || 'sans-serif';
  const colW = Math.floor(w / Math.max(1, columns.length));
  g.font = `600 13px ${font}`;
  columns.forEach((c, x) => g.fillText(c.name, 8 + x * colW, 18));
  g.font = `13px ${font}`;
  (maxRows > 0 ? shown.slice(0, maxRows) : shown)
    .slice(0, Math.floor((h - 24) / 20))
    .forEach((i, y) => {
      columns.forEach((c, x) =>
        g.fillText(
          formatCell(table!.rows[i][c.index], formats.get(c.index)),
          8 + x * colW,
          40 + y * 20,
        ),
      );
    });
  return cv.toDataURL('image/png');
}

function init(msg: Init): void {
  applyTheme(msg.theme);
  app.setAttribute('aria-label', msg.alt);
  status('loading');
  const src = msg.sources.data;
  if (!src) return fail('the table has no "data" source');
  const opts = resolveOptions(msg.options);
  if (!opts.ok) return fail(opts.error);
  table = buildTable(decodeText(src));
  if (table.columns.length === 0) return fail('the CSV is empty');
  const vis = visibleColumns(table, opts.options.columns);
  if (!vis.ok) return fail(vis.error);
  const fmt = formatsByColumn(table, opts.options.format);
  if (!fmt.ok) return fail(fmt.error);
  if (
    opts.options.sort &&
    !table.columns.some((c) => c.name.toLowerCase() === opts.options.sort!.column.toLowerCase())
  ) {
    return fail(`no column "${opts.options.sort.column}" to sort by`);
  }
  columns = vis.columns;
  formats = fmt.byIndex;
  maxRows = opts.options.maxRows;
  const named = opts.options.sort;
  sort = named
    ? {
        ...named,
        column: table.columns.find((c) => c.name.toLowerCase() === named.column.toLowerCase())!
          .name,
      }
    : null;
  filterInput.value = opts.options.filter;
  render();
  status('loaded');
}

filterInput.addEventListener('input', render);
startBridge({ onInit: init, onTheme: applyTheme, onSnapshot: snapshot });
