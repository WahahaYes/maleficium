import { describe, it, expect } from 'vitest';
import { createOwnWrites } from './own-writes';

/** An in-memory disk the recorder reads through. */
function disk(initial: Record<string, string> = {}) {
  const files = new Map(Object.entries(initial));
  const enc = new TextEncoder();
  return {
    files,
    io: {
      read: async (p: string) => (files.has(p) ? enc.encode(files.get(p)!) : null),
    },
  };
}

describe('own-write echo matching', () => {
  it('suppresses echoes of a save, however many arrive', async () => {
    const d = disk();
    const own = createOwnWrites(d.io);
    d.files.set('/p/a.tex', 'hello');
    own.wrote('/p/a.tex', 'hello');
    expect(await own.isEcho('/p/a.tex')).toBe(true);
    expect(await own.isEcho('/p/a.tex')).toBe(true);
  });

  it('reports an external edit made right after a save', async () => {
    const d = disk();
    const own = createOwnWrites(d.io);
    d.files.set('/p/a.tex', 'mine');
    own.wrote('/p/a.tex', 'mine');
    d.files.set('/p/a.tex', 'theirs');
    expect(await own.isEcho('/p/a.tex')).toBe(false);
    // The record is gone: later events on this path are external too.
    d.files.set('/p/a.tex', 'mine');
    expect(await own.isEcho('/p/a.tex')).toBe(false);
  });

  it('never treats an unrecorded path as an echo', async () => {
    const d = disk({ '/p/b.tex': 'x' });
    const own = createOwnWrites(d.io);
    expect(await own.isEcho('/p/b.tex')).toBe(false);
  });

  it('matches a removal against an absent path', async () => {
    const d = disk({ '/p/old.tex': 'x' });
    const own = createOwnWrites(d.io);
    d.files.delete('/p/old.tex');
    own.wrote('/p/old.tex', null);
    expect(await own.isEcho('/p/old.tex')).toBe(true);
    d.files.set('/p/old.tex', 'recreated elsewhere');
    expect(await own.isEcho('/p/old.tex')).toBe(false);
  });

  it('records a settled path from what the disk holds', async () => {
    const d = disk({ '/p/new.tex': 'moved content' });
    const own = createOwnWrites(d.io);
    own.settled('/p/new.tex');
    expect(await own.isEcho('/p/new.tex')).toBe(true);
  });

  it('keeps a newer own write when an older one is judged stale', async () => {
    const d = disk();
    const own = createOwnWrites(d.io);
    d.files.set('/p/a.tex', 'v1');
    own.wrote('/p/a.tex', 'v1');
    d.files.set('/p/a.tex', 'v2');
    const judged = own.isEcho('/p/a.tex');
    own.wrote('/p/a.tex', 'v2');
    expect(await judged).toBe(false);
    expect(await own.isEcho('/p/a.tex')).toBe(true);
  });
});
