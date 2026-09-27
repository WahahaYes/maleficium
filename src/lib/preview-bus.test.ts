import { describe, it, expect } from 'vitest';
import { emitPdf, onPdf, sourceFor, type PreviewDoc } from './preview-bus';

const roots = [
  { rootId: 'proj', path: '/home/u/paper' },
  { rootId: 'scratch', path: '/data/untitled' },
];

describe('sourceFor', () => {
  it('names the containing root and the path relative to it', () => {
    expect(sourceFor('/home/u/paper/ch/main.tex', roots)).toEqual({
      rootId: 'proj',
      rootPath: '/home/u/paper',
      mainRel: 'ch/main.tex',
    });
    expect(sourceFor('/data/untitled/untitled.tex', roots)?.rootId).toBe('scratch');
  });

  it('respects the path boundary and returns null outside every root', () => {
    expect(sourceFor('/home/u/paper2/main.tex', roots)).toBeNull();
    expect(sourceFor('/tmp/main.tex', roots)).toBeNull();
  });
});

describe('sourceFor on Windows roots', () => {
  const win = [{ rootId: 'proj', path: 'C:\\Users\\u\\paper' }];
  it('rebases either separator and refuses a prefix sibling', () => {
    expect(sourceFor('C:\\Users\\u\\paper\\ch\\main.tex', win)?.mainRel).toBe('ch/main.tex');
    expect(sourceFor('C:/Users/u/paper/main.tex', win)?.mainRel).toBe('main.tex');
    expect(sourceFor('C:\\Users\\u\\paper2\\main.tex', win)).toBeNull();
  });
});

describe('emitPdf', () => {
  it('derives docKey from the source', () => {
    const seen: PreviewDoc[] = [];
    const off = onPdf((d) => seen.push(d));
    emitPdf({
      url: '/o/main.pdf',
      source: sourceFor('/home/u/paper/main.tex', roots),
      revision: null,
    });
    emitPdf({ url: '/o/x.pdf', source: null, revision: null });
    off();
    expect(seen.map((d) => d.docKey)).toEqual(['proj:main.tex', null]);
    expect(seen[1].stamp).toBeGreaterThan(seen[0].stamp);
  });
});
