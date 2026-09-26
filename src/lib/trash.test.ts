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
import { setProviders } from './fs-provider';
import { desktopFs, desktopDialog } from './fs-provider.tauri';

beforeEach(() => {
  vi.resetAllMocks();
  setProviders({ fs: desktopFs, dialog: desktopDialog });
  vi.mocked(appDataDir).mockResolvedValue('/app/data');
  vi.mocked(mkdir).mockResolvedValue(undefined);
  vi.mocked(remove).mockResolvedValue(undefined);
  vi.mocked(writeFile).mockResolvedValue(undefined);
});

describe('moveToTrash', () => {
  it('preserves arbitrary bytes through cross-device fallback', async () => {
    // Bytes chosen to be lossy under UTF-8 text decode (PNG magic + 0xFF/0xFE/NUL).
    const original = new Uint8Array([
      0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0xff, 0xfe, 0x00, 0x80, 0xc3, 0x28,
    ]);
    vi.mocked(rename).mockRejectedValueOnce(new Error('EXDEV: cross-device link not permitted'));
    vi.mocked(readFile).mockResolvedValueOnce(original);

    const history = new FileHistory();
    const result = await moveToTrash(history, '1a2b3c4d', '/root', '/root/fig.png');

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
    const result = await moveToTrash(history, '1a2b3c4d', '/root', '/root/main.tex');

    expect(result).toMatchObject({ ok: true });
    // The trash dir is the grant's root id, not a frontend hash of the path.
    expect(history.list()[0]?.trashPath.startsWith('/app/data/maleficium-trash/1a2b3c4d/')).toBe(
      true,
    );
    expect(vi.mocked(readFile)).not.toHaveBeenCalled();
    expect(vi.mocked(writeFile)).not.toHaveBeenCalled();
    expect(vi.mocked(remove)).not.toHaveBeenCalled();
    expect(history.size).toBe(1);
  });

  it('names the entry by its project-relative path, as core does', async () => {
    vi.mocked(rename).mockResolvedValueOnce(undefined);

    const history = new FileHistory();
    await moveToTrash(history, '1a2b3c4d', '/root', '/root/sub/a.tex');

    const dest = history.list()[0]?.trashPath ?? '';
    expect(dest).toMatch(/^\/app\/data\/maleficium-trash\/1a2b3c4d\/a\.tex__sub__a\.tex__\d+$/);
    expect(vi.mocked(rename)).toHaveBeenCalledWith('/root/sub/a.tex', dest);
  });

  it('refuses a path outside the project root', async () => {
    const history = new FileHistory();
    const result = await moveToTrash(history, '1a2b3c4d', '/root', '/other/a.tex');

    expect(result.ok).toBe(false);
    expect(vi.mocked(rename)).not.toHaveBeenCalled();
    expect(history.size).toBe(0);
  });

  it('reports failure without throwing when the trash home is unreachable', async () => {
    vi.mocked(mkdir).mockRejectedValueOnce(new Error('no trash home'));
    // rename must not be attempted when mkdir fails
    vi.mocked(rename).mockResolvedValue(undefined);

    const history = new FileHistory();
    const result = await moveToTrash(history, '1a2b3c4d', '/root', '/root/main.tex');

    expect(result.ok).toBe(false);
    expect(result.error).toBeTruthy();
    expect(vi.mocked(rename)).not.toHaveBeenCalled();
    expect(history.size).toBe(0);
  });

  it('reports failure without throwing when the fallback copy fails', async () => {
    vi.mocked(rename).mockRejectedValueOnce(new Error('EXDEV'));
    vi.mocked(readFile).mockRejectedValueOnce(new Error('unreadable'));

    const history = new FileHistory();
    const result = await moveToTrash(history, '1a2b3c4d', '/root', '/root/fig.png');

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

  it('falls back to copy-then-delete when rename crosses devices', async () => {
    const bytes = new Uint8Array([0x89, 0x50, 0x4e, 0x47, 0x00, 0xff]);
    vi.mocked(rename).mockRejectedValueOnce(new Error('EXDEV: cross-device link not permitted'));
    vi.mocked(readFile).mockResolvedValueOnce(bytes);

    const history = new FileHistory();
    history.record({ originalPath: '/root/main.tex', trashPath: '/trash/main.tex.123' });
    const result = await undoTrash(history);

    expect(result).toMatchObject({ ok: true });
    expect(vi.mocked(readFile)).toHaveBeenCalledWith('/trash/main.tex.123');
    expect(Array.from(vi.mocked(writeFile).mock.calls[0]?.[1] as Uint8Array)).toEqual(
      Array.from(bytes),
    );
    expect(vi.mocked(remove)).toHaveBeenCalledWith('/trash/main.tex.123');
    expect(history.size).toBe(0);
  });
});
