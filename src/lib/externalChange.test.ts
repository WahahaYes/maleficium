import { describe, expect, it, vi, beforeEach } from 'vitest';
import { acknowledgeDisk, getOrCreateBuffer, markSaved, updateBuffer } from './buffers';
import { decideExternal, holdWrites, writesHeld } from './externalChange';
import { saveTex } from './files';
import { bindProjectRoot, clearProjectRoots } from './fs-provider';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

import { invoke } from '@tauri-apps/api/core';

function loaded(text: string) {
  const m = new Map();
  getOrCreateBuffer(m, '/p/a.tex', text);
  return m;
}

beforeEach(() => {
  vi.resetAllMocks();
  clearProjectRoots();
  bindProjectRoot('proj1', '/p');
  vi.mocked(invoke).mockResolvedValue({
    op: 'fileSave',
    result: { stored: true, rev: 'r1', deduped: false, reason: null },
  });
});

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

describe('core save', () => {
  it('refuses to save over an unresolved disk change without calling core', async () => {
    holdWrites('/p/a.tex', true);
    expect(writesHeld('/p/a.tex')).toBe(true);
    await expect(saveTex('/p/a.tex', 'mine')).rejects.toThrow(/changed on disk/);
    expect(invoke).not.toHaveBeenCalled();
    holdWrites('/p/a.tex', false);
    await saveTex('/p/a.tex', 'mine');
    expect(invoke).toHaveBeenLastCalledWith('core_request', {
      req: {
        op: 'fileSave',
        params: { rootId: 'proj1', rel: 'a.tex', text: 'mine', base: null },
      },
    });
  });

  it('carries the synced base for the conflict check and returns the outcome', async () => {
    const outcome = await saveTex('/p/b.tex', 'mine', 'synced');
    expect(invoke).toHaveBeenLastCalledWith('core_request', {
      req: {
        op: 'fileSave',
        params: { rootId: 'proj1', rel: 'b.tex', text: 'mine', base: 'synced' },
      },
    });
    expect(outcome).toMatchObject({ stored: true, rev: 'r1' });
  });

  it('surfaces a core conflict refusal', async () => {
    vi.mocked(invoke).mockRejectedValueOnce('changed on disk: reload or keep your edits first');
    await expect(saveTex('/p/c.tex', 'mine', 'synced')).rejects.toThrow(/changed on disk/);
  });

  it('refuses a path outside every open project', async () => {
    await expect(saveTex('/q/a.tex', 'mine')).rejects.toThrow(/outside any open project/);
    expect(invoke).not.toHaveBeenCalled();
  });
});
