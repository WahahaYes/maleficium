import { describe, it, expect, vi, beforeEach } from 'vitest';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

import { invoke } from '@tauri-apps/api/core';
import { grantProjectAccess, grantUntitledAccess } from './projectAccess';
import { clearProjectRoots, lookupProjectRoot } from './fs-provider';

beforeEach(() => {
  clearProjectRoots();
});

describe('grantProjectAccess', () => {
  it('carries the backend-minted root id with the canonical path', async () => {
    vi.mocked(invoke).mockResolvedValueOnce({
      op: 'grantProjectAccess',
      result: { path: '/home/u/paper', rootId: '1a2b3c4d' },
    });
    expect(await grantProjectAccess('/home/u/paper/')).toEqual({
      ok: true,
      path: '/home/u/paper',
      rootId: '1a2b3c4d',
      error: null,
    });
    expect(invoke).toHaveBeenCalledWith('core_request', {
      req: { op: 'grantProjectAccess', params: { root: '/home/u/paper/' } },
    });
  });

  it('maps a refusal to ok:false without throwing', async () => {
    vi.mocked(invoke).mockRejectedValueOnce('forbidden path (not absolute): x');
    expect(await grantProjectAccess('x')).toEqual({
      ok: false,
      path: null,
      rootId: null,
      error: 'forbidden path (not absolute): x',
    });
    expect(lookupProjectRoot('/home/u/paper/main.tex')).toBeNull();
  });

  it('binds the granted root for the desktop file seam', async () => {
    vi.mocked(invoke).mockResolvedValueOnce({
      op: 'grantProjectAccess',
      result: { path: '/home/u/paper', rootId: '1a2b3c4d' },
    });
    await grantProjectAccess('/home/u/paper/');
    expect(lookupProjectRoot('/home/u/paper/main.tex')).toEqual({
      rootId: '1a2b3c4d',
      rel: 'main.tex',
    });
  });
});

describe('grantUntitledAccess', () => {
  it('asks the backend for its scratch root, never naming a path', async () => {
    vi.mocked(invoke).mockResolvedValueOnce({
      op: 'grantUntitledAccess',
      result: { path: '/data/untitled', rootId: 'aa11bb22' },
    });
    expect(await grantUntitledAccess()).toMatchObject({ ok: true, rootId: 'aa11bb22' });
    expect(invoke).toHaveBeenLastCalledWith('core_request', {
      req: { op: 'grantUntitledAccess', params: {} },
    });
    expect(lookupProjectRoot('/data/untitled/a.tex')).toEqual({
      rootId: 'aa11bb22',
      rel: 'a.tex',
    });
  });
});
