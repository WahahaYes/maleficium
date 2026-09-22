import { describe, it, expect } from 'vitest';
import { filterOutline, searchOutline } from './outline.view';
import type { OutlineEntry } from './generated/structure';

// Rows as the structure crate emits them (parsing is tested in Rust).
const ROWS: OutlineEntry[] = [
  { level: 1, title: 'Intro', line: 1, kind: 'section' },
  { level: 1, title: 'sec:intro', line: 2, kind: 'label', detail: 'sec:intro' },
  { level: 1, title: 'A diagram.', line: 3, kind: 'figure', detail: 'figs/diagram' },
  { level: 1, title: '(table)', line: 7, kind: 'table' },
  { level: 1, title: 'b', line: 9, kind: 'input', detail: 'ch/b' },
];

describe('filterOutline', () => {
  it('segments by kind, all stays document order', () => {
    expect(filterOutline(ROWS, 'sections').map((e) => e.kind)).toEqual(['section']);
    expect(filterOutline(ROWS, 'labels').map((e) => e.kind)).toEqual(['label']);
    expect(filterOutline(ROWS, 'inputs').map((e) => e.kind)).toEqual(['input']);
    expect(filterOutline(ROWS, 'all')).toEqual(ROWS);
  });
  it('tables ride the figures segment', () => {
    expect(filterOutline(ROWS, 'figures').map((e) => e.kind)).toEqual(['figure', 'table']);
  });
});

describe('searchOutline', () => {
  it('matches titles and details, case-insensitive', () => {
    expect(searchOutline(ROWS, '').length).toBe(5);
    expect(searchOutline(ROWS, 'intro').map((e) => e.kind)).toEqual(['section', 'label']);
    expect(searchOutline(ROWS, 'SEC:INTRO').length).toBe(1);
    expect(searchOutline(ROWS, 'diagram').map((e) => e.kind)).toEqual(['figure']);
    expect(searchOutline(ROWS, 'ch/b').map((e) => e.kind)).toEqual(['input']);
    expect(searchOutline(ROWS, 'nothing-here').length).toBe(0);
  });
});
