import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import {
  buildTable,
  compileFilter,
  formatCell,
  formatsByColumn,
  resolveOptions,
  visibleColumns,
  visibleRows,
} from './model';

// The fixture: name,score with alpha..epsilon scoring 10..50.
const csv = readFileSync('e2e/fixtures/interactive/data/results.csv', 'utf8');
const table = buildTable(csv);
const names = (idx: number[]) => idx.map((i) => table.rows[i][0].raw);
const opts = (raw: Record<string, unknown>) => {
  const r = resolveOptions(raw);
  if (!r.ok) throw new Error(r.error);
  return r.options;
};

describe('the fixture table', () => {
  it('has a text column and a numeric column', () => {
    expect(table.columns.map((c) => [c.name, c.numeric])).toEqual([
      ['name', false],
      ['score', true],
    ]);
    expect(table.rows).toHaveLength(5);
  });

  it('keeps file order with no sort', () => {
    expect(names(visibleRows(table, { sort: null, filter: '' }))).toEqual([
      'alpha',
      'beta',
      'gamma',
      'delta',
      'epsilon',
    ]);
  });

  it('sorts a text column ascending and descending', () => {
    const sort = (descending: boolean) =>
      names(visibleRows(table, { sort: { column: 'name', descending }, filter: '' }));
    expect(sort(false)).toEqual(['alpha', 'beta', 'delta', 'epsilon', 'gamma']);
    expect(sort(true)).toEqual(['gamma', 'epsilon', 'delta', 'beta', 'alpha']);
  });

  it('sorts a numeric column by value, not text', () => {
    const t = buildTable('n\n9\n10\n100\n');
    const order = visibleRows(t, { sort: { column: 'n', descending: false }, filter: '' });
    expect(order.map((i) => t.rows[i][0].raw)).toEqual(['9', '10', '100']);
  });

  it('finds the sort column ignoring case', () => {
    expect(
      names(visibleRows(table, { sort: { column: 'SCORE', descending: true }, filter: '' }))[0],
    ).toBe('epsilon');
  });

  it('filters by plain text across all cells', () => {
    expect(names(visibleRows(table, { sort: null, filter: 'ta' }))).toEqual(['beta', 'delta']);
    expect(names(visibleRows(table, { sort: null, filter: '30' }))).toEqual(['gamma']);
  });

  it('filters by column comparisons and combines terms', () => {
    const f = (filter: string) => names(visibleRows(table, { sort: null, filter }));
    expect(f('score>20')).toEqual(['gamma', 'delta', 'epsilon']);
    expect(f('score<=20')).toEqual(['alpha', 'beta']);
    expect(f('score=30')).toEqual(['gamma']);
    expect(f('score!=30 score>=20')).toEqual(['beta', 'delta', 'epsilon']);
    expect(f('name:ta score>20')).toEqual(['delta']);
    expect(f('name=ALPHA')).toEqual(['alpha']);
    expect(f('')).toHaveLength(5);
    expect(f('zzz')).toEqual([]);
  });

  it('treats a term naming no column as plain text, and quotes keep a term whole', () => {
    expect(names(visibleRows(table, { sort: null, filter: 'nope>1' }))).toEqual([]);
    const t = buildTable('k\nfoo bar\nfoo\n');
    const keep = compileFilter('"foo bar"', t.columns);
    expect(t.rows.map((r) => keep(r))).toEqual([true, false]);
  });

  it('puts empty cells last in either direction', () => {
    const t = buildTable('k,n\na,2\nb,\nc,1\n');
    for (const descending of [false, true]) {
      const order = visibleRows(t, { sort: { column: 'n', descending }, filter: '' });
      expect(t.rows[order[2]][0].raw).toBe('b');
    }
  });
});

describe('options', () => {
  it('reads sort, filter, columns, format and maxrows', () => {
    const o = opts({
      sort: 'score desc',
      filter: 'a',
      columns: 'score, name',
      format: 'score=fixed:1;name=text',
      maxrows: '3',
    });
    expect(o.sort).toEqual({ column: 'score', descending: true });
    expect(o.columns).toEqual(['score', 'name']);
    expect(o.format.score).toEqual({ kind: 'fixed', digits: 1 });
    expect(o.maxRows).toBe(3);
  });

  it('defaults to nothing, and ignores the exporter-only pdfrows', () => {
    expect(opts({ pdfrows: 10 })).toEqual({
      sort: null,
      filter: '',
      columns: null,
      format: {},
      maxRows: 0,
    });
  });

  it('rejects unknown options and malformed values with a message', () => {
    for (const bad of [
      { colour: 'red' },
      { format: 'score=bogus' },
      { format: 'score=fixed:99' },
      { format: 'score' },
      { maxrows: -1 },
      { maxrows: 'x' },
    ]) {
      expect(resolveOptions(bad).ok).toBe(false);
    }
  });

  it('selects and orders columns, and names a missing one', () => {
    const v = visibleColumns(table, ['SCORE', 'name']);
    expect(v.ok && v.columns.map((c) => c.name)).toEqual(['score', 'name']);
    expect(visibleColumns(table, ['nope'])).toEqual({ ok: false, error: 'no column "nope"' });
    expect(formatsByColumn(table, { nope: { kind: 'int' } }).ok).toBe(false);
  });
});

describe('formatting', () => {
  const cell = (raw: string) => buildTable(`x\n${raw}\n`).rows[0][0];
  it('formats numbers by option', () => {
    expect(formatCell(cell('1234.5'), { kind: 'fixed', digits: 1 })).toBe('1,234.5');
    expect(formatCell(cell('1234.5'), { kind: 'int' })).toBe('1,235');
    expect(formatCell(cell('0.256'), { kind: 'percent', digits: 1 })).toBe('25.6%');
    expect(formatCell(cell('12345'), { kind: 'sci', digits: 2 })).toBe('1.23e+4');
    expect(formatCell(cell('7'), { kind: 'fixed', digits: 2 })).toBe('7.00');
  });
  it('leaves text and unformatted cells as written', () => {
    expect(formatCell(cell('abc'), { kind: 'fixed', digits: 2 })).toBe('abc');
    expect(formatCell(cell('7'), undefined)).toBe('7');
    expect(formatCell(cell('7'), { kind: 'text' })).toBe('7');
  });
  it('applies a column format from options to the fixture', () => {
    const o = opts({ format: 'score=fixed:1' });
    const f = formatsByColumn(table, o.format);
    expect(f.ok && formatCell(table.rows[0][1], f.byIndex.get(1))).toBe('10.0');
  });
});
