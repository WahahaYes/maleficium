// The table runtime's logic, free of the DOM so unit tests pin it: build a
// typed table from CSV, resolve the `.sty` options, sort, filter and format.
import { parseCsv, toNumber } from '../csv';

export interface Column {
  name: string;
  index: number;
  /** Every non-empty cell is a plain decimal number. */
  numeric: boolean;
}

export interface Cell {
  raw: string;
  num: number | null;
}

export interface Table {
  columns: Column[];
  rows: Cell[][];
}

export function buildTable(csv: string): Table {
  const { header, rows } = parseCsv(csv);
  const cells = rows.map((r) => r.map((raw) => ({ raw, num: toNumber(raw) })));
  const columns = header.map((name, index) => ({
    name,
    index,
    numeric:
      cells.some((r) => r[index].raw.trim() !== '') &&
      cells.every((r) => r[index].raw.trim() === '' || r[index].num !== null),
  }));
  return { columns, rows: cells };
}

/** The column a name refers to: an exact match first, then ignoring case. */
export function findColumn(columns: Column[], name: string): Column | null {
  const n = name.trim();
  return (
    columns.find((c) => c.name === n) ??
    columns.find((c) => c.name.toLowerCase() === n.toLowerCase()) ??
    null
  );
}

// ---- formatting -----------------------------------------------------------

export type Format =
  | { kind: 'text' }
  | { kind: 'int' }
  | { kind: 'fixed'; digits: number }
  | { kind: 'percent'; digits: number }
  | { kind: 'sci'; digits: number };

export function formatCell(cell: Cell, f: Format | undefined): string {
  if (!f || f.kind === 'text' || cell.num === null) return cell.raw;
  switch (f.kind) {
    case 'int':
      return Math.round(cell.num).toLocaleString('en-US');
    case 'fixed':
      return cell.num.toLocaleString('en-US', {
        minimumFractionDigits: f.digits,
        maximumFractionDigits: f.digits,
      });
    case 'percent':
      return `${(cell.num * 100).toFixed(f.digits)}%`;
    case 'sci':
      return cell.num.toExponential(f.digits);
  }
}

function parseFormat(spec: string): Format | null {
  const [kind, arg] = spec.trim().split(':');
  const digits = arg === undefined ? undefined : Number(arg);
  if (digits !== undefined && !(Number.isInteger(digits) && digits >= 0 && digits <= 12)) {
    return null;
  }
  switch (kind) {
    case 'text':
    case 'int':
      return digits === undefined ? { kind } : null;
    case 'fixed':
    case 'percent':
    case 'sci':
      return { kind, digits: digits ?? 2 };
    default:
      return null;
  }
}

// ---- options --------------------------------------------------------------

export interface SortKey {
  column: string;
  descending: boolean;
}

export interface TableOptions {
  sort: SortKey | null;
  filter: string;
  /** Column names to show, in order; null shows all. */
  columns: string[] | null;
  /** Column name to format, as written in the option. */
  format: Record<string, Format>;
  /** Most rows shown; 0 shows all. */
  maxRows: number;
}

export type Resolved = { ok: true; options: TableOptions } | { ok: false; error: string };

/**
 * Reads the `.sty` options (all strings or numbers, flat):
 *   sort="score desc"   filter="alpha score>20"   columns="name,score"
 *   format="score=fixed:1;rate=percent:1"   maxrows=50
 * `pdfrows` is read by the exporter to crop the PDF poster and is ignored here.
 */
export function resolveOptions(raw: Record<string, unknown>): Resolved {
  const known = new Set(['sort', 'filter', 'columns', 'format', 'maxrows', 'pdfrows']);
  for (const k of Object.keys(raw)) {
    if (!known.has(k)) return { ok: false, error: `unknown table option "${k}"` };
  }
  const str = (k: string): string | null => {
    const v = raw[k];
    return v === undefined || v === null || v === '' ? null : String(v);
  };
  let sort: SortKey | null = null;
  const s = str('sort');
  if (s) {
    const m = /^(.+?)(?:\s+(asc|desc))?$/i.exec(s.trim());
    if (!m) return { ok: false, error: `bad sort option "${s}"` };
    sort = { column: m[1].trim(), descending: (m[2] ?? 'asc').toLowerCase() === 'desc' };
  }
  const cols = str('columns');
  const format: Record<string, Format> = {};
  const f = str('format');
  if (f) {
    for (const part of f.split(';')) {
      if (part.trim() === '') continue;
      const eq = part.lastIndexOf('=');
      const fmt = eq < 0 ? null : parseFormat(part.slice(eq + 1));
      if (!fmt) return { ok: false, error: `bad format entry "${part.trim()}"` };
      format[part.slice(0, eq).trim()] = fmt;
    }
  }
  const mr = raw.maxrows === undefined ? 0 : Number(raw.maxrows);
  if (!Number.isInteger(mr) || mr < 0)
    return { ok: false, error: 'maxrows must be an integer >= 0' };
  return {
    ok: true,
    options: {
      sort,
      filter: str('filter') ?? '',
      columns: cols
        ? cols
            .split(',')
            .map((c) => c.trim())
            .filter(Boolean)
        : null,
      format,
      maxRows: mr,
    },
  };
}

/** The columns to show, in order, or an error naming a column that does not exist. */
export function visibleColumns(
  table: Table,
  names: string[] | null,
): { ok: true; columns: Column[] } | { ok: false; error: string } {
  if (!names) return { ok: true, columns: table.columns };
  const out: Column[] = [];
  for (const n of names) {
    const c = findColumn(table.columns, n);
    if (!c) return { ok: false, error: `no column "${n}"` };
    out.push(c);
  }
  return { ok: true, columns: out };
}

/** Maps each option's column name to the real column's name. */
export function formatsByColumn(
  table: Table,
  format: Record<string, Format>,
): { ok: true; byIndex: Map<number, Format> } | { ok: false; error: string } {
  const byIndex = new Map<number, Format>();
  for (const [name, f] of Object.entries(format)) {
    const c = findColumn(table.columns, name);
    if (!c) return { ok: false, error: `format names no column "${name}"` };
    byIndex.set(c.index, f);
  }
  return { ok: true, byIndex };
}

// ---- filtering ------------------------------------------------------------

export type Predicate = (row: Cell[]) => boolean;

const TERM = /^(.+?)(>=|<=|!=|>|<|=|:)(.*)$/;

/**
 * Compiles filter text. Terms split on spaces (use quotes to keep one together):
 *   alpha          any cell contains "alpha" (case-insensitive)
 *   name:alp       the name column contains "alp"
 *   name=alpha     the name column equals "alpha" (numbers compare as numbers)
 *   score>20       numeric comparison; also >=, <, <=, !=
 * A term whose left side is no column is plain text. All terms must match.
 */
export function compileFilter(text: string, columns: Column[]): Predicate {
  const terms = text.match(/"[^"]*"|\S+/g) ?? [];
  const preds: Predicate[] = [];
  for (const t of terms) {
    const term = t.startsWith('"') && t.endsWith('"') && t.length >= 2 ? t.slice(1, -1) : t;
    const m = t === term ? TERM.exec(term) : null;
    const col = m ? findColumn(columns, m[1]) : null;
    if (m && col) {
      const op = m[2];
      const rhs = m[3].trim();
      const rn = toNumber(rhs);
      const i = col.index;
      preds.push((row) => {
        const cell = row[i];
        if (op === ':') return cell.raw.toLowerCase().includes(rhs.toLowerCase());
        if (cell.num !== null && rn !== null) {
          switch (op) {
            case '>':
              return cell.num > rn;
            case '>=':
              return cell.num >= rn;
            case '<':
              return cell.num < rn;
            case '<=':
              return cell.num <= rn;
            case '!=':
              return cell.num !== rn;
            default:
              return cell.num === rn;
          }
        }
        const a = cell.raw.trim().toLowerCase();
        const b = rhs.toLowerCase();
        if (op === '=') return a === b;
        if (op === '!=') return a !== b;
        return false;
      });
    } else {
      const needle = term.toLowerCase();
      preds.push((row) => row.some((c) => c.raw.toLowerCase().includes(needle)));
    }
  }
  return (row) => preds.every((p) => p(row));
}

// ---- view -----------------------------------------------------------------

export interface ViewState {
  sort: SortKey | null;
  filter: string;
}

/** Row indices after filtering and sorting. The sort is stable; empty cells go last. */
export function visibleRows(table: Table, state: ViewState): number[] {
  const keep = compileFilter(state.filter, table.columns);
  const idx: number[] = [];
  table.rows.forEach((r, i) => {
    if (keep(r)) idx.push(i);
  });
  const col = state.sort ? findColumn(table.columns, state.sort.column) : null;
  if (state.sort && col) {
    const dir = state.sort.descending ? -1 : 1;
    const collator = new Intl.Collator('en', { numeric: true, sensitivity: 'base' });
    idx.sort((a, b) => {
      const x = table.rows[a][col.index];
      const y = table.rows[b][col.index];
      const xe = x.raw.trim() === '';
      const ye = y.raw.trim() === '';
      if (xe || ye) return xe === ye ? a - b : xe ? 1 : -1;
      const c = col.numeric
        ? (x.num as number) - (y.num as number)
        : collator.compare(x.raw, y.raw);
      return c !== 0 ? c * dir : a - b;
    });
  }
  return idx;
}
