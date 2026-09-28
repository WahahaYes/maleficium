import { describe, expect, it } from 'vitest';
import { acknowledgeDisk, getOrCreateBuffer, markSaved, updateBuffer } from './buffers';
import { decideExternal, holdWrites, writesHeld } from './externalChange';
import { saveTex } from './files';
import { setProviders, type DialogProvider, type FsProvider } from './fs-provider';

const disk = new Map<string, string>();
setProviders({
  fs: {
    readText: async (p: string) => disk.get(p) ?? '',
    writeText: async (p: string, c: string) => void disk.set(p, c),
  } as unknown as FsProvider,
  dialog: {} as DialogProvider,
});

function loaded(text: string) {
  const m = new Map();
  getOrCreateBuffer(m, '/p/a.tex', text);
  return m;
}

describe('decideExternal', () => {
  it('ignores a disk that still holds the synced text', () => {
    const b = updateBuffer(loaded('a'), '/p/a.tex', 'a edited').get('/p/a.tex')!;
    expect(decideExternal(b, 'a')).toBe('none');
  });
  it('reloads a clean buffer, in-place or rename-over alike (content decides)', () => {
    expect(decideExternal(loaded('a').get('/p/a.tex')!, 'b')).toBe('reload');
  });
  it('raises a conflict for unsaved edits against a different disk', () => {
    const b = updateBuffer(loaded('a'), '/p/a.tex', 'mine').get('/p/a.tex')!;
    expect(decideExternal(b, 'theirs')).toBe('conflict');
  });
  it('syncs when the disk now matches the edits', () => {
    const b = updateBuffer(loaded('a'), '/p/a.tex', 'same').get('/p/a.tex')!;
    expect(decideExternal(b, 'same')).toBe('sync');
  });
});

describe('disk tracking', () => {
  it('a save records the written text as the disk', () => {
    const m = markSaved(updateBuffer(loaded('a'), '/p/a.tex', 'b'), '/p/a.tex');
    expect(m.get('/p/a.tex')).toMatchObject({ value: 'b', dirty: false, disk: 'b' });
  });
  it('keeping edits acknowledges the disk and leaves them dirty', () => {
    const m = acknowledgeDisk(updateBuffer(loaded('a'), '/p/a.tex', 'mine'), '/p/a.tex', 'theirs');
    const b = m.get('/p/a.tex')!;
    expect(b).toMatchObject({ value: 'mine', dirty: true, disk: 'theirs' });
    expect(decideExternal(b, 'theirs')).toBe('none');
  });
});

describe('held writes', () => {
  it('refuses to save over an unresolved disk change, then allows it', async () => {
    disk.set('/p/a.tex', 'theirs');
    holdWrites('/p/a.tex', true);
    expect(writesHeld('/p/a.tex')).toBe(true);
    await expect(saveTex('/p/a.tex', 'mine')).rejects.toThrow(/changed on disk/);
    expect(disk.get('/p/a.tex')).toBe('theirs');
    holdWrites('/p/a.tex', false);
    await saveTex('/p/a.tex', 'mine');
    expect(disk.get('/p/a.tex')).toBe('mine');
  });

  it('refuses to save over an outside edit the watcher has not reported yet', async () => {
    disk.set('/p/b.tex', 'theirs');
    await expect(saveTex('/p/b.tex', 'mine', 'synced')).rejects.toThrow(/changed on disk/);
    expect(disk.get('/p/b.tex')).toBe('theirs');
  });
  it('saves over the text the buffer last synced, or over its own text', async () => {
    disk.set('/p/c.tex', 'synced');
    await saveTex('/p/c.tex', 'mine', 'synced');
    expect(disk.get('/p/c.tex')).toBe('mine');
    await saveTex('/p/c.tex', 'mine', 'synced');
    expect(disk.get('/p/c.tex')).toBe('mine');
  });
});
