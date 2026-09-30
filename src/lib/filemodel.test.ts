import { describe, it, expect } from 'vitest';
import { FileHistory } from './file-history';
import { sortTreeEntries, isHiddenName, LARGE_FILE_BYTES } from './files';
import {
  getOrCreateBuffer,
  updateBuffer,
  markSaved,
  enforceBufferCap,
  renameBuffer,
  dropBuffer,
  reloadBuffer,
  type BufferState,
} from './buffers';

describe('FileHistory 50-entry cap', () => {
  it('evicts oldest past 50', () => {
    const h = new FileHistory();
    for (let i = 0; i < 55; i++) h.record({ originalPath: `/a${i}`, trashPath: `/t${i}` });
    expect(h.size).toBe(50);
    expect(h.list()[0].originalPath).toBe('/a5');
  });
});

describe('tree sort + hidden', () => {
  it('dirs-first case-insensitive', () => {
    const rows = sortTreeEntries([
      { name: 'b.tex', type: 'file' },
      { name: 'A.tex', type: 'file' },
      { name: 'dir', type: 'dir' },
    ]);
    expect(rows.map((r) => r.name)).toEqual(['dir', 'A.tex', 'b.tex']);
  });
  it('filters build artifacts but keeps sources', () => {
    expect(isHiddenName('.git')).toBe(true);
    expect(isHiddenName('out')).toBe(true);
    expect(isHiddenName('x.aux')).toBe(true);
    expect(isHiddenName('main.tex')).toBe(false);
    expect(LARGE_FILE_BYTES).toBe(500_000);
  });
});

describe('buffers dirty tracking', () => {
  it('an echo of the loaded value is not an edit', () => {
    // The editor re-emits its text when the doc is set externally (file
    // switch, reload). That echo must not dirty the buffer, or autosave
    // rewrites a file the user never touched.
    const m = new Map([['/a.tex', { value: 'loaded', dirty: false, disk: 'loaded', version: 0 }]]);
    const echoed = updateBuffer(m, '/a.tex', 'loaded');
    expect(echoed.get('/a.tex')?.dirty).toBe(false);
    expect(echoed.get('/a.tex')?.version).toBe(0);
    expect(echoed).toBe(m);
    // A real change still dirties.
    const edited = updateBuffer(m, '/a.tex', 'loaded!');
    expect(edited.get('/a.tex')?.dirty).toBe(true);
    expect(edited.get('/a.tex')?.version).toBe(1);
    expect(edited).not.toBe(m);
  });
  it('dirty survives unrelated updates until saved', () => {
    const first = getOrCreateBuffer(new Map(), '/a.tex', 'a');
    const edited = updateBuffer(new Map([['/a.tex', first]]), '/a.tex', 'a2');
    edited.set('/b.tex', { value: 'b', dirty: false, disk: 'b', version: 0 });
    expect(edited.get('/a.tex')?.dirty).toBe(true);
    const saved = markSaved(edited, '/a.tex');
    expect(saved.get('/a.tex')?.dirty).toBe(false);
  });
});

describe('buffer cap', () => {
  it('eviction drops oldest clean first, never dirty', () => {
    const m = new Map<string, BufferState>();
    for (let i = 0; i < 11; i++) {
      m.set(`/f${i}.tex`, { dirty: i === 10, value: `v${i}`, disk: `v${i}`, version: 0 });
    }
    const kept = enforceBufferCap(m, '/f10.tex');
    expect(kept.size).toBe(10);
    expect(kept.get('/f10.tex')?.dirty).toBe(true);
    expect(kept.has('/f0.tex')).toBe(false);
  });
});

describe('buffer file-op transforms', () => {
  const b = { value: 'x', dirty: true, disk: '', version: 2 };

  it('renameBuffer re-keys and keeps the state object', () => {
    const next = renameBuffer(new Map([['/a', b]]), '/a', '/b');
    expect(next.get('/b')).toBe(b);
    expect(next.has('/a')).toBe(false);
  });

  it('renameBuffer and dropBuffer return the same map when nothing matches', () => {
    const m = new Map([['/a', b]]);
    expect(renameBuffer(m, '/z', '/y')).toBe(m);
    expect(dropBuffer(m, '/z')).toBe(m);
    expect(dropBuffer(m, '/a').has('/a')).toBe(false);
  });

  it('reloadBuffer replaces with clean content and bumps the version', () => {
    const next = reloadBuffer(new Map([['/a', b]]), '/a', 'disk');
    expect(next.get('/a')).toEqual({ value: 'disk', dirty: false, disk: 'disk', version: 3 });
    expect(reloadBuffer(new Map(), '/n', 'd').get('/n')?.version).toBe(1);
  });
});
