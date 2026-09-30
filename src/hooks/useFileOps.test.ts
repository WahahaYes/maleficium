import { describe, it, expect, vi, beforeEach } from 'vitest';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

vi.mock('@tauri-apps/api/path', () => ({
  appDataDir: vi.fn(async () => '/app/data'),
  appCacheDir: vi.fn(async () => '/app/cache'),
}));

import { invoke } from '@tauri-apps/api/core';
import { useFileOps, type UseFileOpsDeps } from './useFileOps';
import { setProviders, type DirEntry, type FsProvider } from '../lib/fs-provider';
import { FileHistory } from '../lib/file-history';
import { transport } from '../lib/event-transport';
import type { BufferState } from '../lib/buffers';

/** Flat in-memory filesystem: path → text. Directories are implicit. */
function memoryFs(files: Map<string, string>): FsProvider {
  const missing = (p: string) => new Error('ENOENT: ' + p);
  return {
    async readText(p) {
      const v = files.get(p);
      if (v === undefined) throw missing(p);
      return v;
    },
    async writeText(p, c) {
      files.set(p, c);
    },
    async readBytes(p) {
      const v = files.get(p);
      if (v === undefined) throw missing(p);
      return new TextEncoder().encode(v);
    },
    async writeBytes(p, c) {
      files.set(p, new TextDecoder().decode(c));
    },
    async listDir(dir) {
      const out: DirEntry[] = [];
      for (const p of files.keys()) {
        if (p.startsWith(dir + '/') && !p.slice(dir.length + 1).includes('/')) {
          out.push({ name: p.slice(dir.length + 1), isDirectory: false, isFile: true });
        }
      }
      return out;
    },
    async rename(from, to) {
      const v = files.get(from);
      if (v === undefined) throw missing(from);
      files.delete(from);
      files.set(to, v);
    },
    async remove(p) {
      if (!files.delete(p)) throw missing(p);
    },
    async mkdir() {},
    async stat(p) {
      return files.has(p) ? { size: files.get(p)!.length, isDirectory: false, isFile: true } : null;
    },
  };
}

const noDialog = { openDirectory: async () => null, saveFile: async () => null };

/** Deps backed by plain variables so each handler's effects are observable. */
function harness(over: Partial<UseFileOpsDeps> = {}) {
  const state = {
    fileName: '/p/main.tex',
    previewFile: null as string | null,
    buffers: new Map<string, BufferState>(),
    selected: [] as string[],
    reloads: 0,
  };
  const trash = new FileHistory();
  const deps = (): UseFileOpsDeps => ({
    root: '/p',
    projectId: 'p1',
    scratch: null,
    fileName: state.fileName,
    previewFile: state.previewFile,
    setFileName: (v) => (state.fileName = v),
    mainFile: '/p/main.tex',
    setPreviewFile: (v) => (state.previewFile = v),
    setBuffers: (u) => (state.buffers = typeof u === 'function' ? u(state.buffers) : u),
    trash,
    reloadTree: async () => {
      state.reloads++;
    },
    handleSelect: async (p) => {
      state.selected.push(p);
    },
    ...over,
  });
  // useFileOps holds no React state, so its handlers are driven directly.
  // eslint-disable-next-line react-hooks/rules-of-hooks
  return { state, trash, ops: () => useFileOps(deps()) };
}

const data = (i: number) => transport().snapshot()[i].event as Record<string, unknown>;
const actions = () =>
  transport()
    .snapshot()
    .map((_, i) => data(i).action);

let files: Map<string, string>;

beforeEach(() => {
  transport().clear();
  files = new Map([
    ['/p/main.tex', 'main'],
    ['/p/fig.tex', 'fig body'],
  ]);
  setProviders({ fs: memoryFs(files), dialog: noDialog });
});

describe('useFileOps delete → undo', () => {
  it('deletes through core, then restores it', async () => {
    vi.mocked(invoke)
      .mockResolvedValueOnce({ op: 'fileTrash', result: '/data/trash/fig.tex__fig.tex__7' })
      .mockResolvedValueOnce({ op: 'fileUndoTrash', result: '/p/fig.tex' });
    const { state, trash, ops } = harness();
    state.buffers.set('/p/fig.tex', {
      value: 'fig body',
      dirty: false,
      disk: 'fig body',
      version: 1,
    });
    state.previewFile = '/p/fig.tex';

    await ops().handleDelete('/p/fig.tex');

    expect(invoke).toHaveBeenLastCalledWith('core_request', {
      req: {
        op: 'fileTrash',
        params: { rootId: 'p1', rel: 'fig.tex', confirm: '/p/fig.tex' },
      },
    });
    const [entry] = trash.list();
    expect(entry).toMatchObject({
      originalPath: '/p/fig.tex',
      trashPath: '/data/trash/fig.tex__fig.tex__7',
    });
    expect(state.buffers.has('/p/fig.tex')).toBe(false);
    expect(state.previewFile).toBeNull();

    await ops().handleUndo();

    expect(invoke).toHaveBeenLastCalledWith('core_request', {
      req: {
        op: 'fileUndoTrash',
        params: { rootId: 'p1', trashPath: '/data/trash/fig.tex__fig.tex__7' },
      },
    });
    expect(trash.size).toBe(0);
    expect(state.reloads).toBe(2);
    expect(actions()).toEqual(['file.delete', 'file.undo-delete']);
    expect(data(1).path).toBe('/p/fig.tex');
  });

  it('reports a failed delete and leaves the file and buffer alone', async () => {
    vi.mocked(invoke).mockRejectedValueOnce('forbidden path (outside project): gone.tex');
    const { state, trash, ops } = harness();
    state.buffers.set('/p/gone.tex', { value: 'x', dirty: true, disk: '', version: 1 });

    await ops().handleDelete('/p/gone.tex');

    expect(trash.size).toBe(0);
    expect(state.buffers.has('/p/gone.tex')).toBe(true);
    expect(actions()).toEqual(['file.delete-failed']);
  });

  it('reports undo with an empty trash as a failure', async () => {
    const { ops } = harness();
    await ops().handleUndo();
    expect(actions()).toEqual(['file.undo-delete-failed']);
  });
});

describe('useFileOps create and rename', () => {
  it('creates an empty file and opens it', async () => {
    const { state, ops } = harness();

    await ops().handleCreate('/p', 'intro.tex');

    expect(files.get('/p/intro.tex')).toBe('');
    expect(state.selected).toEqual(['/p/intro.tex']);
    expect(actions()).toEqual(['file.create']);
  });

  it('renames the open file: disk, buffer key and active file all follow', async () => {
    const { state, ops } = harness();
    const buf = { value: 'main edited', dirty: true, disk: 'main', version: 3 };
    state.buffers.set('/p/main.tex', buf);

    await ops().handleRename('/p/main.tex', 'paper.tex');

    expect(files.has('/p/main.tex')).toBe(false);
    expect(files.get('/p/paper.tex')).toBe('main');
    expect(state.buffers.get('/p/paper.tex')).toBe(buf);
    expect(state.buffers.has('/p/main.tex')).toBe(false);
    expect(state.fileName).toBe('/p/paper.tex');
    expect(actions()).toEqual(['file.rename']);
  });

  it('rejects an empty rename without touching disk', async () => {
    const { ops } = harness();
    await ops().handleRename('/p/fig.tex', '   ');
    expect(files.get('/p/fig.tex')).toBe('fig body');
    expect(actions()).toEqual(['file.rename-failed']);
  });
});

describe('useFileOps clean', () => {
  it('asks the backend to clean the main file by root id, never naming the outdir', async () => {
    vi.mocked(invoke).mockResolvedValueOnce({ op: 'cleanOutputs', result: 3 });
    const { ops } = harness();

    await ops().handleClean();

    expect(invoke).toHaveBeenLastCalledWith('core_request', {
      req: { op: 'cleanOutputs', params: { rootId: 'p1', mainRel: 'main.tex' } },
    });
    expect(actions()).toEqual(['compile.clean']);
    expect(data(0)).toEqual({ action: 'compile.clean', target: 'main.tex', removed: 3 });
  });

  it('cleans an untitled document through the scratch root', async () => {
    vi.mocked(invoke).mockResolvedValueOnce({ op: 'cleanOutputs', result: 0 });
    const { ops } = harness({
      root: null,
      projectId: null,
      mainFile: '/data/untitled/untitled.tex',
      scratch: { rootId: 's1', path: '/data/untitled' },
    });

    await ops().handleClean();

    expect(invoke).toHaveBeenLastCalledWith('core_request', {
      req: { op: 'cleanOutputs', params: { rootId: 's1', mainRel: 'untitled.tex' } },
    });
    expect(transport().snapshot()[0].message).toBe('Clean: already clean');
  });

  it('refuses a target outside every root without calling the backend', async () => {
    vi.mocked(invoke).mockClear();
    const { ops } = harness({ mainFile: '/elsewhere/main.tex' });
    await ops().handleClean();
    expect(invoke).not.toHaveBeenCalled();
    expect(data(0).reason).toBe('no-project-file');
  });

  it('reports a backend refusal', async () => {
    vi.mocked(invoke).mockRejectedValueOnce('forbidden path (outside project): main.tex');
    const { ops } = harness();
    await ops().handleClean();
    expect(actions()).toEqual(['compile.clean-failed']);
  });
});
