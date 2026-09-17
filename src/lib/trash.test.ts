import { describe, it, expect, vi, beforeEach } from 'vitest';
import { FileHistory } from './file-history';

vi.mock('@tauri-apps/plugin-fs', () => ({
  mkdir: vi.fn(),
  rename: vi.fn(),
  readFile: vi.fn(),
  writeFile: vi.fn(),
  remove: vi.fn(),
}));

vi.mock('@tauri-apps/api/path', () => ({
  appDataDir: vi.fn(),
}));

import { mkdir, rename, readFile, writeFile, remove } from '@tauri-apps/plugin-fs';
import { appDataDir } from '@tauri-apps/api/path';
import { moveToTrash, undoTrash } from './trash';

beforeEach(() => {
  vi.resetAllMocks();
  vi.mocked(appDataDir).mockResolvedValue('/app/data');
  vi.mocked(mkdir).mockResolvedValue(undefined);
  vi.mocked(remove).mockResolvedValue(undefined);
  vi.mocked(writeFile).mockResolvedValue(undefined);
});

describe('moveToTrash', () => {
  it('preserves arbitrary bytes through cross-device fallback', async () => {
    // Bytes chosen to be lossy under UTF-8 text decode (PNG magic + 0xFF/0xFE/NUL).
    const original = new Uint8Array([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0xff, 0xfe, 0x00, 0x80, 0xc3, 0x28]);
    vi.mocked(rename).mockRejectedValueOnce(new Error('EXDEV: cross-device link not permitted'));
    vi.mocked(readFile).mockResolvedValueOnce(original);

    const history = new FileHistory();
    const result = await moveToTrash(history, '/root', '/root/fig.png');

    expect(result).toMatchObject({ ok: true });
    expect(vi.mocked(readFile)).toHaveBeenCalledWith('/root/fig.png');
    const written = vi.mocked(writeFile).mock.calls[0]?.[1];
    expect(written).toBeInstanceOf(Uint8Array);
    expect(Array.from(written as Uint8Array)).toEqual(Array.from(original));
    expect(vi.mocked(remove)).toHaveBeenCalledWith('/root/fig.png');
    expect(history.size).toBe(1);
    expect(history.list()[0]?.originalPath).toBe('/root/fig.png');
  });

  it('prefers atomic rename without copying bytes when possible', async () => {
    vi.mocked(rename).mockResolvedValueOnce(undefined);

    const history = new FileHistory();
    const result = await moveToTrash(history, '/root', '/root/main.tex');

    expect(result).toMatchObject({ ok: true });
    expect(vi.mocked(readFile)).not.toHaveBeenCalled();
    expect(vi.mocked(writeFile)).not.toHaveBeenCalled();
    expect(vi.mocked(remove)).not.toHaveBeenCalled();
    expect(history.size).toBe(1);
  });

  it('reports failure without throwing when the trash home is unreachable', async () => {
    vi.mocked(mkdir).mockRejectedValueOnce(new Error('no trash home'));
    // rename must not be attempted when mkdir fails
    vi.mocked(rename).mockResolvedValue(undefined);

    const history = new FileHistory();
    const result = await moveToTrash(history, '/root', '/root/main.tex');

    expect(result.ok).toBe(false);
    expect(result.error).toBeTruthy();
    expect(vi.mocked(rename)).not.toHaveBeenCalled();
    expect(history.size).toBe(0);
  });

  it('reports failure without throwing when the fallback copy fails', async () => {
    vi.mocked(rename).mockRejectedValueOnce(new Error('EXDEV'));
    vi.mocked(readFile).mockRejectedValueOnce(new Error('unreadable'));

    const history = new FileHistory();
    const result = await moveToTrash(history, '/root', '/root/fig.png');

    expect(result.ok).toBe(false);
    expect(result.error).toBeTruthy();
    expect(history.size).toBe(0);
  });
});

describe('undoTrash', () => {
  it('reports failure without throwing when there is nothing to undo', async () => {
    const result = await undoTrash(new FileHistory());
    expect(result).toMatchObject({ ok: false, error: 'nothing to undo' });
  });
});
