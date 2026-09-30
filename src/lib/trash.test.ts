import { describe, it, expect, vi, beforeEach } from 'vitest';
import { FileHistory } from './file-history';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

import { invoke } from '@tauri-apps/api/core';
import { moveToTrash, undoTrash } from './trash';

beforeEach(() => {
  vi.resetAllMocks();
});

describe('moveToTrash', () => {
  it('trashes through core and records the trash path for undo', async () => {
    vi.mocked(invoke).mockResolvedValueOnce({
      op: 'fileTrash',
      result: '/data/trash/a.tex__sub__a.tex__123',
    });

    const history = new FileHistory();
    const result = await moveToTrash(history, '1a2b3c4d', '/root', '/root/sub/a.tex');

    expect(result).toMatchObject({ ok: true });
    expect(invoke).toHaveBeenLastCalledWith('core_request', {
      req: {
        op: 'fileTrash',
        params: { rootId: '1a2b3c4d', rel: 'sub/a.tex', confirm: '/root/sub/a.tex' },
      },
    });
    expect(history.list()).toMatchObject([
      { originalPath: '/root/sub/a.tex', trashPath: '/data/trash/a.tex__sub__a.tex__123' },
    ]);
  });

  it('refuses a path outside the project root without calling core', async () => {
    const history = new FileHistory();
    const result = await moveToTrash(history, '1a2b3c4d', '/root', '/other/a.tex');

    expect(result.ok).toBe(false);
    expect(invoke).not.toHaveBeenCalled();
    expect(history.size).toBe(0);
  });

  it('reports a core refusal without throwing', async () => {
    vi.mocked(invoke).mockRejectedValueOnce('not a file: missing.tex');

    const history = new FileHistory();
    const result = await moveToTrash(history, '1a2b3c4d', '/root', '/root/missing.tex');

    expect(result).toMatchObject({ ok: false, error: 'not a file: missing.tex' });
    expect(history.size).toBe(0);
  });
});

describe('undoTrash', () => {
  it('reports failure without throwing when there is nothing to undo', async () => {
    const result = await undoTrash(new FileHistory(), '1a2b3c4d');
    expect(result).toMatchObject({ ok: false, error: 'nothing to undo' });
    expect(invoke).not.toHaveBeenCalled();
  });

  it('restores through core and drops the entry', async () => {
    vi.mocked(invoke).mockResolvedValueOnce({ op: 'fileUndoTrash', result: '/root/main.tex' });

    const history = new FileHistory();
    history.record({ originalPath: '/root/main.tex', trashPath: '/data/trash/x' });
    const result = await undoTrash(history, '1a2b3c4d');

    expect(result).toMatchObject({ ok: true });
    expect(invoke).toHaveBeenLastCalledWith('core_request', {
      req: {
        op: 'fileUndoTrash',
        params: { rootId: '1a2b3c4d', trashPath: '/data/trash/x' },
      },
    });
    expect(history.size).toBe(0);
  });

  it('re-records the entry when core refuses', async () => {
    vi.mocked(invoke).mockRejectedValueOnce('forbidden path (not a trash entry)');

    const history = new FileHistory();
    history.record({ originalPath: '/root/main.tex', trashPath: '/data/trash/x' });
    const result = await undoTrash(history, '1a2b3c4d');

    expect(result.ok).toBe(false);
    expect(history.list()).toMatchObject([
      { originalPath: '/root/main.tex', trashPath: '/data/trash/x' },
    ]);
  });
});
