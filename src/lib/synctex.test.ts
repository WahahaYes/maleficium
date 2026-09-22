import { describe, it, expect, vi } from 'vitest';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

import { invoke } from '@tauri-apps/api/core';
import {
  forward_sync,
  inverse_sync,
  relTo,
  syncAvailable,
  texPathFor,
  shouldTurnPage,
  isCrossFileHit,
} from './synctex';

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

describe('relTo', () => {
  it('strips the root, refusing paths outside it', () => {
    expect(relTo('/proj', '/proj/ch/a.tex')).toBe('ch/a.tex');
    expect(relTo('/proj', '/project2/a.tex')).toBeNull();
    expect(relTo('/proj', '/proj')).toBeNull();
  });
});

describe('synctex IPC', () => {
  it('forward speaks root id and relative paths', async () => {
    vi.mocked(invoke).mockResolvedValueOnce({ page: 4 });
    expect(await forward_sync('r1', 'main.tex', 'ch/a.tex', 12)).toEqual({
      ok: true,
      page: 4,
      error: null,
    });
    expect(invoke).toHaveBeenLastCalledWith('forward_sync', {
      rootId: 'r1',
      mainRel: 'main.tex',
      texRel: 'ch/a.tex',
      line: 12,
    });
  });

  it('inverse returns a relative hit and maps refusals to ok:false', async () => {
    vi.mocked(invoke).mockResolvedValueOnce({ relPath: 'ch/a.tex', line: 7 });
    expect(await inverse_sync('r1', 'main.tex', 2, 10, 20)).toEqual({
      ok: true,
      relPath: 'ch/a.tex',
      line: 7,
      error: null,
    });
    vi.mocked(invoke).mockRejectedValueOnce('no compiled output for main.tex');
    expect(await inverse_sync('r1', 'main.tex', 1)).toMatchObject({
      ok: false,
      error: 'no compiled output for main.tex',
    });
  });
});
