import { describe, it, expect, vi } from 'vitest';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

import { invoke } from '@tauri-apps/api/core';
import { grantProjectAccess } from './projectAccess';

describe('grantProjectAccess', () => {
  it('carries the backend-minted root id with the canonical path', async () => {
    vi.mocked(invoke).mockResolvedValueOnce({ path: '/home/u/paper', rootId: '1a2b3c4d' });
    expect(await grantProjectAccess('/home/u/paper/')).toEqual({
      ok: true,
      path: '/home/u/paper',
      rootId: '1a2b3c4d',
      error: null,
    });
    expect(invoke).toHaveBeenCalledWith('grant_project_access', { root: '/home/u/paper/' });
  });

  it('maps a refusal to ok:false without throwing', async () => {
    vi.mocked(invoke).mockRejectedValueOnce('forbidden path (not absolute): x');
    expect(await grantProjectAccess('x')).toEqual({
      ok: false,
      path: null,
      rootId: null,
      error: 'forbidden path (not absolute): x',
    });
  });
});
