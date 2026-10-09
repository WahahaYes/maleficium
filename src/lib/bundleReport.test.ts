import { describe, expect, it } from 'vitest';
import { groupWarnings, kindLabel, makeReport } from './bundleReport';

describe('bundleReport', () => {
  it('derives a label from any kind string', () => {
    expect(kindLabel('size-cap')).toBe('Size cap');
    expect(kindLabel('convert')).toBe('Convert');
    expect(kindLabel('figure_missing.x')).toBe('Figure missing x');
    expect(kindLabel('')).toBe('Other');
  });

  it('groups by kind in first-seen order and keeps every message', () => {
    const g = groupWarnings([
      { kind: 'figure', message: 'a.png missing' },
      { kind: 'convert', message: '441 undefined macros' },
      { kind: 'figure', message: 'b.png missing' },
    ]);
    expect(g.map((x) => x.kind)).toEqual(['figure', 'convert']);
    expect(g[0].messages).toEqual(['a.png missing', 'b.png missing']);
    expect(g[1].label).toBe('Convert');
  });

  it('counts warnings and carries the written path', () => {
    const r = makeReport('single-file', {
      path: '/x/p.html',
      bytes: 10,
      warnings: [
        { kind: 'join', message: 'm' },
        { kind: 'join', message: 'n' },
      ],
    });
    expect(r.count).toBe(2);
    expect(r.groups).toHaveLength(1);
    expect(r.path).toBe('/x/p.html');
    expect(makeReport('x', { path: 'p', bytes: 0, warnings: [] }).groups).toEqual([]);
  });
});
