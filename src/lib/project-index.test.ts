import { describe, it, expect, vi } from 'vitest';
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
import { invoke } from '@tauri-apps/api/core';
import { overlayDelta, projectIndex, setProjectIndex } from './project-index';
import { desktopProjectIndex } from './project-index.tauri';

describe('project index seam', () => {
  it('desktop impl forwards to the Rust commands', async () => {
    setProjectIndex(desktopProjectIndex);
    vi.mocked(invoke).mockResolvedValueOnce({ op: 'indexOpen', result: 3 });
    expect(await projectIndex().open('1a2b')).toBe(3);
    expect(invoke).toHaveBeenLastCalledWith('core_request', {
      req: { op: 'indexOpen', params: { rootId: '1a2b' } },
    });
    vi.mocked(invoke).mockResolvedValueOnce({ op: 'indexOverlay', result: null });
    await projectIndex().overlay('1a2b', 'main.tex', null);
    expect(invoke).toHaveBeenLastCalledWith('core_request', {
      req: {
        op: 'indexOverlay',
        params: {
          rootId: '1a2b',
          rel: 'main.tex',
          text: null,
        },
      },
    });
  });
});

describe('overlay delta', () => {
  it('sends changed dirty text and lifts overlays of clean or closed buffers', () => {
    const sent = new Map([
      ['a.tex', 'old'],
      ['b.tex', 'same'],
      ['c.tex', 'gone'],
    ]);
    const dirty = new Map([
      ['a.tex', 'new'],
      ['b.tex', 'same'],
      ['d.tex', 'fresh'],
    ]);
    expect(overlayDelta(sent, dirty)).toEqual([
      ['a.tex', 'new'],
      ['d.tex', 'fresh'],
      ['c.tex', null],
    ]);
    expect(overlayDelta(new Map(), new Map())).toEqual([]);
  });
});
