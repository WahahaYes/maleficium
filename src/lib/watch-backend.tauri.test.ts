import { describe, it, expect, vi } from 'vitest';
vi.mock('@tauri-apps/plugin-fs', () => ({ watch: vi.fn() }));
import { watch } from '@tauri-apps/plugin-fs';
import { classify, desktopWatch } from './watch-backend.tauri';

describe('plugin-fs event classification', () => {
  it('maps create/remove/modify kinds to changes', () => {
    expect(classify({ type: { create: { kind: 'file' } }, paths: ['/a'] })).toEqual([
      { kind: 'create', path: '/a' },
    ]);
    expect(classify({ type: { remove: { kind: 'any' } }, paths: ['/a'] })).toEqual([
      { kind: 'delete', path: '/a' },
    ]);
    expect(
      classify({ type: { modify: { kind: 'data', mode: 'content' } }, paths: ['/a'] }),
    ).toEqual([{ kind: 'modify', path: '/a' }]);
    expect(classify({ type: 'any', paths: ['/a'] })).toEqual([{ kind: 'modify', path: '/a' }]);
    expect(classify({ type: 'other', paths: ['/a'] })).toEqual([{ kind: 'modify', path: '/a' }]);
  });
  it('drops access noise and fans out multiple paths', () => {
    expect(classify({ type: { access: { kind: 'any' } }, paths: ['/a'] })).toEqual([]);
    expect(classify({ type: { modify: { kind: 'any' } }, paths: ['/a', '/b'] })).toEqual([
      { kind: 'modify', path: '/a' },
      { kind: 'modify', path: '/b' },
    ]);
  });
  it('watches recursively and forwards only non-empty batches', async () => {
    const stop = vi.fn();
    let handler: ((ev: unknown) => void) | undefined;
    vi.mocked(watch).mockImplementation(async (_root, cb) => {
      handler = cb as (ev: unknown) => void;
      return stop;
    });
    const seen: unknown[] = [];
    const unwatch = await desktopWatch.watch('/p', (c) => seen.push(c));
    expect(vi.mocked(watch).mock.calls[0][0]).toBe('/p');
    expect(vi.mocked(watch).mock.calls[0][2]).toMatchObject({ recursive: true });
    handler?.({ type: { access: { kind: 'any' } }, paths: ['/p/a'], attrs: null });
    handler?.({ type: { create: { kind: 'file' } }, paths: ['/p/b'], attrs: null });
    expect(seen).toEqual([[{ kind: 'create', path: '/p/b' }]]);
    unwatch();
    expect(stop).toHaveBeenCalled();
  });
});
