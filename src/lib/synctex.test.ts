import { describe, it, expect } from 'vitest';
import {
  parseForwardSync,
  parseInverseSync,
  isForwardNoMatch,
  syncAvailable,
  texPathFor,
  shouldTurnPage,
  isCrossFileHit,
} from './synctex';

describe('parseForwardSync', () => {
  it('extracts the Page: number from synctex view output', () => {
    expect(parseForwardSync('SyncTeX result begin\nPage:3\nx:100\ny:200\nSyncTeX result end')).toBe(
      3,
    );
  });
  it('clamps Page:0 up to page 1', () => {
    expect(parseForwardSync('Page:0')).toBe(1);
  });
  it('returns null when no Page: line is present', () => {
    expect(parseForwardSync('SyncTeX result begin\nx:1\n')).toBe(null);
  });
  it('returns null for empty output', () => {
    expect(parseForwardSync('')).toBe(null);
  });
});

describe('isForwardNoMatch', () => {
  it('flags the no_match token', () => {
    expect(isForwardNoMatch('SyncTeX result begin\nno_match\nSyncTeX result end')).toBe(true);
  });
  it('flags the bare {} response', () => {
    expect(isForwardNoMatch('{}')).toBe(true);
  });
  it('flags the untagged-file warning', () => {
    expect(isForwardNoMatch('SyncTeX Warning: No tag for /proj/other.tex')).toBe(true);
  });
  it('passes real output through', () => {
    expect(isForwardNoMatch('SyncTeX result begin\nPage:1\nSyncTeX result end')).toBe(false);
  });
});

describe('parseInverseSync', () => {
  it('extracts Input: and Line: from synctex edit output', () => {
    const r = parseInverseSync(
      'SyncTeX result begin\nInput:/proj/hello.tex\nLine:7\nColumn:0\nSyncTeX result end',
    );
    expect(r).toEqual({ line: 7, hitFile: '/proj/hello.tex' });
  });
  it('returns null line when Line: is missing', () => {
    const r = parseInverseSync('SyncTeX result begin\nInput:/proj/hello.tex\nSyncTeX result end');
    expect(r).toEqual({ line: null, hitFile: '/proj/hello.tex' });
  });
  it('returns null hitFile when Input: is missing', () => {
    const r = parseInverseSync('SyncTeX result begin\nLine:12\nSyncTeX result end');
    expect(r).toEqual({ line: 12, hitFile: null });
  });
  it('returns nulls for empty output', () => {
    expect(parseInverseSync('')).toEqual({ line: null, hitFile: null });
  });
});

describe('syncAvailable', () => {
  it('needs a pdf', () => {
    expect(syncAvailable(null, false)).toBe(false);
    expect(syncAvailable('', false)).toBe(false);
    expect(syncAvailable('/out/main.pdf', false)).toBe(true);
  });
  it('is off while compiling — the gz is being rewritten', () => {
    expect(syncAvailable('/out/main.pdf', true)).toBe(false);
  });
});

describe('texPathFor', () => {
  it('passes an absolute path through', () => {
    expect(texPathFor('/proj/ch/a.tex', '/proj')).toBe('/proj/ch/a.tex');
  });
  it('resolves a bare name against the workdir', () => {
    expect(texPathFor('main.tex', '/proj')).toBe('/proj/main.tex');
  });
});

describe('shouldTurnPage', () => {
  it('moves only on a different page', () => {
    expect(shouldTurnPage(3, 1)).toBe(true);
    expect(shouldTurnPage(1, 1)).toBe(false);
    expect(shouldTurnPage(null, 1)).toBe(false);
  });
});

describe('isCrossFileHit', () => {
  it('detects a hit in another file', () => {
    expect(isCrossFileHit('/proj/ch/b.tex', '/proj/main.tex')).toBe(true);
  });
  it('treats null, empty, or the open file as same-file', () => {
    expect(isCrossFileHit(null, '/proj/main.tex')).toBe(false);
    expect(isCrossFileHit('', '/proj/main.tex')).toBe(false);
    expect(isCrossFileHit('/proj/main.tex', '/proj/main.tex')).toBe(false);
  });
});
