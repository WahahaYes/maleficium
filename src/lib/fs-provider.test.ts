import { describe, it, expect, beforeEach } from 'vitest';
import { bindProjectRoot, clearProjectRoots, lookupProjectRoot } from './fs-provider';

beforeEach(() => {
  clearProjectRoots();
});

describe('lookupProjectRoot', () => {
  it('returns null when nothing is bound', () => {
    expect(lookupProjectRoot('/p/a.tex')).toBeNull();
  });

  it('maps a bound root to rels and to the root itself', () => {
    bindProjectRoot('r1', '/p');
    expect(lookupProjectRoot('/p/a.tex')).toEqual({ rootId: 'r1', rel: 'a.tex' });
    expect(lookupProjectRoot('/p/sub/a.tex')).toEqual({ rootId: 'r1', rel: 'sub/a.tex' });
    expect(lookupProjectRoot('/p')).toEqual({ rootId: 'r1', rel: '' });
    expect(lookupProjectRoot('/p/')).toEqual({ rootId: 'r1', rel: '' });
  });

  it('never claims siblings', () => {
    bindProjectRoot('r1', '/p');
    expect(lookupProjectRoot('/px/a.tex')).toBeNull();
    expect(lookupProjectRoot('/q/a.tex')).toBeNull();
  });

  it('matches Windows roots case-insensitively in either separator', () => {
    bindProjectRoot('r1', 'C:\\Paper');
    expect(lookupProjectRoot('c:/paper/sub/a.tex')).toEqual({ rootId: 'r1', rel: 'sub/a.tex' });
    expect(lookupProjectRoot('C:\\PAPER')).toEqual({ rootId: 'r1', rel: '' });
    expect(lookupProjectRoot('D:\\Paper\\a.tex')).toBeNull();
  });

  it('holds several roots at once', () => {
    bindProjectRoot('r1', '/p');
    bindProjectRoot('r2', '/data/untitled');
    expect(lookupProjectRoot('/p/a.tex')).toEqual({ rootId: 'r1', rel: 'a.tex' });
    expect(lookupProjectRoot('/data/untitled/a.tex')).toEqual({ rootId: 'r2', rel: 'a.tex' });
  });
});
