import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
import { invoke } from '@tauri-apps/api/core';
import { desktopWatch, WATCH_POLL_MS } from './watch-backend.tauri';
import { bindProjectRoot, clearProjectRoots } from './fs-provider';

function respond(op: string, result: unknown) {
  vi.mocked(invoke).mockResolvedValueOnce({ op, result });
}

beforeEach(() => {
  vi.resetAllMocks();
  clearProjectRoots();
  bindProjectRoot('r1', '/p');
  vi.useFakeTimers();
});

afterEach(() => {
  vi.useRealTimers();
});

describe('core watch backend', () => {
  it('starts the core watch and polls it on a timer', async () => {
    respond('watchStart', null);
    const seen: unknown[] = [];
    const unwatch = await desktopWatch.watch('/p', (c) => seen.push(c));
    expect(invoke).toHaveBeenLastCalledWith('core_request', {
      req: { op: 'watchStart', params: { rootId: 'r1' } },
    });
    respond('watchPoll', [{ rel: 'a.tex', change: 'modify' }]);
    await vi.advanceTimersByTimeAsync(WATCH_POLL_MS);
    expect(seen).toEqual([[{ kind: 'modify', path: '/p/a.tex' }]]);
    unwatch();
  });
  it('ignores empty polls and stops polling after unwatch', async () => {
    respond('watchStart', null);
    const seen: unknown[] = [];
    const unwatch = await desktopWatch.watch('/p', (c) => seen.push(c));
    respond('watchPoll', []);
    await vi.advanceTimersByTimeAsync(WATCH_POLL_MS);
    expect(seen).toEqual([]);
    respond('watchStop', null);
    unwatch();
    expect(invoke).toHaveBeenLastCalledWith('core_request', {
      req: { op: 'watchStop', params: { rootId: 'r1' } },
    });
    const calls = vi.mocked(invoke).mock.calls.length;
    await vi.advanceTimersByTimeAsync(WATCH_POLL_MS * 3);
    expect(vi.mocked(invoke).mock.calls.length).toBe(calls);
  });
  it('refuses roots outside any open project', async () => {
    await expect(desktopWatch.watch('/elsewhere', () => {})).rejects.toThrow(
      'outside any open project',
    );
    expect(invoke).not.toHaveBeenCalled();
  });
});
