import { describe, it, expect } from 'vitest';
import { hitParts, searchSummary } from './search.view';
import type { SearchResult } from './generated/index';

const result = (over: Partial<SearchResult> = {}): SearchResult => ({
  files: [],
  hits: 0,
  truncated: 0,
  searched: 3,
  unsearched: 0,
  ...over,
});

describe('search view', () => {
  it('splits a preview around its match, including a clipped preview', () => {
    expect(
      hitParts({ line: 1, col: 4, len: 6, preview: 'see needle here', previewCol: 0 }),
    ).toEqual(['see ', 'needle', ' here']);
    expect(hitParts({ line: 1, col: 104, len: 3, preview: 'ab xyz', previewCol: 101 })).toEqual([
      'ab ',
      'xyz',
      '',
    ]);
  });

  it('summarizes counts, the cap, and unsearchable files honestly', () => {
    expect(searchSummary(result())).toBe('No results');
    const file = { rel: 'a.tex', source: 'disk' as const, revision: 'r', hits: [] };
    expect(searchSummary(result({ hits: 1, files: [file] }))).toBe('1 result in 1 file');
    expect(
      searchSummary(result({ hits: 1000, truncated: 20, unsearched: 2, files: [file, file] })),
    ).toBe(
      '1000+ results in 2 files · 20 more not shown — narrow the search · 2 files not searchable',
    );
  });
});
