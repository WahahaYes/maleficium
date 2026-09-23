import { describe, it, expect, vi } from 'vitest';
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
import { invoke } from '@tauri-apps/api/core';
import { historyStore, setHistoryStore } from './history';
import { desktopHistory } from './history.tauri';

describe('history seam', () => {
  it('desktop impl forwards (projectId, relPath) to the Rust store', async () => {
    setHistoryStore(desktopHistory);
    vi.mocked(invoke).mockResolvedValueOnce({ stored: true, rev: '1', deduped: false });
    const out = await historyStore().recordRevision('1a2b3c4d', 'main.tex', 'x');
    expect(out.stored).toBe(true);
    expect(invoke).toHaveBeenLastCalledWith('history_record', {
      rootId: '1a2b3c4d',
      rel: 'main.tex',
      text: 'x',
    });
  });

  it('returns raw revision bytes, and null when the store refuses', async () => {
    setHistoryStore(desktopHistory);
    vi.mocked(invoke).mockResolvedValueOnce(new Uint8Array([0xff, 0x00]).buffer);
    expect(await historyStore().getRevision('p', 'a.tex', '1')).toEqual(new Uint8Array([0xff, 0]));
    vi.mocked(invoke).mockRejectedValueOnce('revision unavailable');
    expect(await historyStore().restoreRevision('p', 'a.tex', '9')).toBeNull();
    expect(invoke).toHaveBeenLastCalledWith('history_restore', {
      rootId: 'p',
      rel: 'a.tex',
      rev: '9',
    });
  });
});
