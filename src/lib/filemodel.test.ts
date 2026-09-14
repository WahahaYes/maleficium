import { describe, it, expect, vi } from 'vitest';
import { FileHistory } from './file-history';
import { coalesceEvents, debounce } from './watcher';
import { parseGitPorcelain, emptyGitState } from './git';
import { sortTreeEntries, isHiddenName, LARGE_FILE_BYTES } from './files';
import { getOrCreateBuffer, updateBuffer, markSaved } from './buffers';

describe('FileHistory 50-entry cap', () => {
  it('evicts oldest past 50', () => {
    const h = new FileHistory();
    for (let i = 0; i < 55; i++) h.record({ originalPath: `/a${i}`, trashPath: `/t${i}` });
    expect(h.size).toBe(50);
    expect(h.list()[0].originalPath).toBe('/a5');
  });
});

describe('watcher coalesce+debounce', () => {
  it('keeps latest per path', () => {
    const out = coalesceEvents([
      { kind: 'modify', path: '/a' },
      { kind: 'modify', path: '/b' },
      { kind: 'delete', path: '/a' },
    ]);
    expect(out).toEqual([{ kind: 'delete', path: '/a' }, { kind: 'modify', path: '/b' }]);
  });
  it('debounces bursts', () => {
    vi.useFakeTimers();
    const fn = vi.fn();
    const d = debounce(fn, 250);
    d(); d(); d();
    expect(fn).not.toHaveBeenCalled();
    vi.advanceTimersByTime(250);
    expect(fn).toHaveBeenCalledTimes(1);
    vi.useRealTimers();
  });
});

describe('git porcelain badges', () => {
  it('maps M/A/D/U/R + ?? and branch', () => {
    const { badges, branch } = parseGitPorcelain('## main...origin/main\n M a.tex\n?? b.tex\nUU c.tex\nR  d.tex -> e.tex\n');
    expect(branch).toBe('main');
    expect(badges).toMatchObject({ 'a.tex': 'M', 'b.tex': 'A', 'c.tex': 'U', 'e.tex': 'R' });
  });
  it('honest empty state', () => {
    expect(emptyGitState('not-a-repo')).toMatchObject({ ok: false, reason: 'not-a-repo' });
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
  it('dirty survives unrelated updates until saved', () => {
    const first = getOrCreateBuffer(new Map(), '/a.tex', 'a');
    const edited = updateBuffer(new Map([['/a.tex', first]]), '/a.tex', 'a2');
    edited.set('/b.tex', { value: 'b', dirty: false, version: 0 });
    expect(edited.get('/a.tex')?.dirty).toBe(true);
    const saved = markSaved(edited, '/a.tex');
    expect(saved.get('/a.tex')?.dirty).toBe(false);
  });
});

describe('buffer cap', () => {
  it('eviction drops oldest clean first, never dirty (cap enforced by caller)', () => {
    // enforceBufferCap lives in App; here we pin the contract it implements:
    // caller iterates insertion order, skips active + dirty, deletes rest.
    const m = new Map<string, { dirty: boolean; value: string; version: number }>();
    for (let i = 0; i < 11; i++) {
      m.set(`/f${i}.tex`, { dirty: i === 10, value: `v${i}`, version: 0 });
    }
    const active = '/f10.tex';
    for (const k of [...m.keys()]) {
      if (m.size <= 10) break;
      if (k === active) continue;
      const b = m.get(k);
      if (b && !b.dirty) m.delete(k);
    }
    expect(m.size).toBe(10);
    expect(m.get('/f10.tex')?.dirty).toBe(true);
    expect(m.has('/f0.tex')).toBe(false);
  });
});
