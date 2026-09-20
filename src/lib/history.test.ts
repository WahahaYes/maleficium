import { describe, it, expect, vi, beforeEach } from 'vitest';

vi.mock('@tauri-apps/plugin-fs', () => ({
  mkdir: vi.fn(),
  readFile: vi.fn(),
  writeFile: vi.fn(),
  remove: vi.fn(),
  stat: vi.fn(),
}));

vi.mock('@tauri-apps/api/path', () => ({
  appDataDir: vi.fn(),
}));

import { mkdir, readFile, writeFile, remove, stat } from '@tauri-apps/plugin-fs';
import { appDataDir } from '@tauri-apps/api/path';

import { setProviders } from './fs-provider';
import { desktopFs, desktopDialog } from './fs-provider.tauri';

setProviders({ fs: desktopFs, dialog: desktopDialog });
import {
  createHistoryStore,
  evict,
  liveHashes,
  emptyIndex,
  isSnapshotEligible,
  hashBytes,
  MAX_REVISIONS_PER_FILE,
  MAX_HISTORY_BYTES_PER_PROJECT,
  MIN_REVISIONS_KEPT_PER_FILE,
  SNAPSHOT_MAX_FILE_BYTES,
  type Index,
} from './history';

const PROJECT = 'deadbeef';
const ROOT = '/home/u/paper';

/** In-memory disk: path -> bytes. Mirrors what the plugin would persist. */
function fakeDisk() {
  const files = new Map<string, Uint8Array>();
  vi.mocked(appDataDir).mockResolvedValue('/app/data');
  vi.mocked(mkdir).mockResolvedValue(undefined);
  vi.mocked(stat).mockImplementation(async (p: string | URL) => {
    if (!files.has(String(p))) throw new Error('no such file');
    return { size: files.get(String(p))?.length ?? 0, isDirectory: false, isFile: true } as never;
  });
  vi.mocked(readFile).mockImplementation(async (p: string | URL) => {
    const b = files.get(String(p));
    if (!b) throw new Error(`ENOENT: ${String(p)}`);
    return b as Uint8Array<ArrayBuffer>;
  });
  vi.mocked(writeFile).mockImplementation(async (p: string | URL, b: unknown) => {
    files.set(String(p), b as Uint8Array);
  });
  vi.mocked(remove).mockImplementation(async (p: string | URL) => {
    files.delete(String(p));
  });
  return files;
}

function store() {
  return createHistoryStore((id) => (id === PROJECT ? ROOT : null));
}

function bytes(s: string): Uint8Array {
  return new TextEncoder().encode(s);
}

beforeEach(() => {
  vi.resetAllMocks();
});

describe('recording revisions', () => {
  it('stores a revision on save and lists it back', async () => {
    fakeDisk();
    const h = store();

    const out = await h.recordRevision(PROJECT, 'main.tex', bytes('\\documentclass{article}'));

    expect(out).toMatchObject({ stored: true });
    const list = await h.listRevisions(PROJECT, 'main.tex');
    expect(list).toHaveLength(1);
    expect(list[0].bytes).toBe(23);
  });

  it('lists newest first', async () => {
    fakeDisk();
    const h = store();
    await h.recordRevision(PROJECT, 'main.tex', bytes('one'));
    await h.recordRevision(PROJECT, 'main.tex', bytes('two'));
    await h.recordRevision(PROJECT, 'main.tex', bytes('three'));

    const list = await h.listRevisions(PROJECT, 'main.tex');
    expect(list.map((r) => r.bytes)).toEqual([5, 3, 3]);
  });

  it('skips an unchanged save rather than piling up identical revisions', async () => {
    fakeDisk();
    const h = store();
    await h.recordRevision(PROJECT, 'main.tex', bytes('same'));

    const again = await h.recordRevision(PROJECT, 'main.tex', bytes('same'));

    expect(again).toEqual({ stored: false, reason: 'unchanged' });
    expect(await h.listRevisions(PROJECT, 'main.tex')).toHaveLength(1);
  });

  it('reuses one blob when content returns to an earlier state', async () => {
    const files = fakeDisk();
    const h = store();
    await h.recordRevision(PROJECT, 'main.tex', bytes('A'));
    await h.recordRevision(PROJECT, 'main.tex', bytes('B'));

    const back = await h.recordRevision(PROJECT, 'main.tex', bytes('A'));

    expect(back).toMatchObject({ stored: true, deduped: true });
    const blobs = [...files.keys()].filter((p) => p.includes('/blobs/'));
    expect(blobs).toHaveLength(2);
    expect(await h.listRevisions(PROJECT, 'main.tex')).toHaveLength(3);
  });

  it('refuses non-text paths and oversized files', async () => {
    expect(isSnapshotEligible('fig.png', 10)).toEqual({ stored: false, reason: 'not-text' });
    expect(isSnapshotEligible('main.tex', SNAPSHOT_MAX_FILE_BYTES + 1)).toEqual({
      stored: false,
      reason: 'too-large',
    });
    expect(isSnapshotEligible('main.tex', SNAPSHOT_MAX_FILE_BYTES)).toBeNull();
  });

  it('writes nothing for an ineligible file', async () => {
    fakeDisk();
    const h = store();

    const out = await h.recordRevision(PROJECT, 'fig.png', bytes('not really a png'));

    expect(out).toEqual({ stored: false, reason: 'not-text' });
    expect(vi.mocked(writeFile)).not.toHaveBeenCalled();
  });
});

describe('restore', () => {
  it('returns exact bytes, including bytes that do not survive a text decode', async () => {
    const files = fakeDisk();
    const h = store();
    const original = new Uint8Array([0x89, 0x50, 0xff, 0xfe, 0x00, 0x80, 0xc3, 0x28]);
    const rec = await h.recordRevision(PROJECT, 'main.tex', original);
    if (!rec.stored) throw new Error('expected a stored revision');
    await h.recordRevision(PROJECT, 'main.tex', bytes('replaced'));

    const restored = await h.restoreRevision(PROJECT, 'main.tex', rec.rev);

    expect(restored).not.toBeNull();
    expect(Array.from(restored as Uint8Array)).toEqual(Array.from(original));
    expect(Array.from(files.get('/home/u/paper/main.tex') as Uint8Array)).toEqual(
      Array.from(original),
    );
  });

  it('snapshots the on-disk state before overwriting it, so restore is undoable', async () => {
    const files = fakeDisk();
    const h = store();
    const first = await h.recordRevision(PROJECT, 'main.tex', bytes('first'));
    if (!first.stored) throw new Error('expected a stored revision');
    files.set('/home/u/paper/main.tex', bytes('edited-but-never-saved'));

    await h.restoreRevision(PROJECT, 'main.tex', first.rev);

    const list = await h.listRevisions(PROJECT, 'main.tex');
    expect(list.map((r) => r.bytes)).toContain(22);
  });

  it('returns null for an unknown revision and never writes', async () => {
    fakeDisk();
    const h = store();
    await h.recordRevision(PROJECT, 'main.tex', bytes('x'));
    vi.mocked(writeFile).mockClear();

    expect(await h.restoreRevision(PROJECT, 'main.tex', 'nope')).toBeNull();
    expect(vi.mocked(writeFile)).not.toHaveBeenCalled();
  });

  it('returns null for an unknown project rather than guessing a root', async () => {
    const files = fakeDisk();
    const h = store();
    const rec = await h.recordRevision(PROJECT, 'main.tex', bytes('x'));
    if (!rec.stored) throw new Error('expected a stored revision');
    const before = files.size;

    expect(await h.restoreRevision('other', 'main.tex', rec.rev)).toBeNull();
    expect(files.size).toBe(before);
  });
});

describe('retention', () => {
  function indexWith(rel: string, count: number, size: number, startAt = 0): Index {
    const index = emptyIndex();
    index.files[rel] = Array.from({ length: count }, (_, i) => ({
      rev: `${rel}-${i}`,
      hash: `${rel}-hash-${i}`,
      at: startAt + i,
      bytes: size,
    }));
    index.seq = count;
    return index;
  }

  it('caps revisions per file at the stated bound, dropping oldest first', () => {
    const index = indexWith('main.tex', MAX_REVISIONS_PER_FILE + 10, 10);

    evict(index);

    expect(index.files['main.tex']).toHaveLength(MAX_REVISIONS_PER_FILE);
    expect(index.files['main.tex'][0].rev).toBe('main.tex-10');
  });

  it('evicts across files oldest-first once the project byte cap is exceeded', () => {
    // 26 blobs at a twentieth of the cap each: exactly six must go.
    const big = Math.floor(MAX_HISTORY_BYTES_PER_PROJECT / 20);
    const index = emptyIndex();
    index.files['a.tex'] = Array.from({ length: 20 }, (_, i) => ({
      rev: `a-${i}`,
      hash: `a-hash-${i}`,
      at: i,
      bytes: big,
    }));
    index.files['b.tex'] = Array.from({ length: 6 }, (_, i) => ({
      rev: `b-${i}`,
      hash: `b-hash-${i}`,
      at: 100 + i,
      bytes: big,
    }));

    evict(index);

    // The six oldest went, all from the older file; the newer file is untouched.
    expect(index.files['a.tex']).toHaveLength(14);
    expect(index.files['b.tex']).toHaveLength(6);
    expect(index.files['a.tex'][0].rev).toBe('a-6');
  });

  it('never takes a file below the floor, even when the byte cap still fails', () => {
    const huge = MAX_HISTORY_BYTES_PER_PROJECT;
    const index = emptyIndex();
    index.files['a.tex'] = Array.from({ length: MIN_REVISIONS_KEPT_PER_FILE }, (_, i) => ({
      rev: `a-${i}`,
      hash: `a-hash-${i}`,
      at: i,
      bytes: huge,
    }));

    evict(index);

    expect(index.files['a.tex']).toHaveLength(MIN_REVISIONS_KEPT_PER_FILE);
  });

  it('reports the caps and current usage', async () => {
    fakeDisk();
    const h = store();
    await h.recordRevision(PROJECT, 'main.tex', bytes('abc'));

    const info = await h.retentionInfo(PROJECT);

    expect(info).toMatchObject({
      maxRevisionsPerFile: MAX_REVISIONS_PER_FILE,
      maxHistoryBytesPerProject: MAX_HISTORY_BYTES_PER_PROJECT,
      minRevisionsKeptPerFile: MIN_REVISIONS_KEPT_PER_FILE,
      snapshotMaxFileBytes: SNAPSHOT_MAX_FILE_BYTES,
      revisions: 1,
      bytes: 3,
    });
  });

  it('counts one distinct blob once, however many revisions point at it', async () => {
    fakeDisk();
    const h = store();
    await h.recordRevision(PROJECT, 'main.tex', bytes('A'));
    await h.recordRevision(PROJECT, 'main.tex', bytes('B'));
    await h.recordRevision(PROJECT, 'main.tex', bytes('A'));

    const info = await h.retentionInfo(PROJECT);

    expect(info.revisions).toBe(3);
    expect(info.bytes).toBe(2);
  });
});

describe('garbage collection', () => {
  it('derives live hashes from the index alone', () => {
    const index = emptyIndex();
    index.files['a.tex'] = [{ rev: '1', hash: 'h1', at: 1, bytes: 1 }];
    index.files['b.tex'] = [
      { rev: '2', hash: 'h1', at: 2, bytes: 1 },
      { rev: '3', hash: 'h2', at: 3, bytes: 1 },
    ];

    expect([...liveHashes(index)].sort()).toEqual(['h1', 'h2']);
  });

  it('deletes blobs no surviving revision references', async () => {
    const files = fakeDisk();
    const h = store();
    for (let i = 0; i < MAX_REVISIONS_PER_FILE + 5; i++) {
      await h.recordRevision(PROJECT, 'main.tex', bytes(`revision ${i}`));
    }

    const blobs = [...files.keys()].filter((p) => p.includes('/blobs/'));
    expect(blobs).toHaveLength(MAX_REVISIONS_PER_FILE);
    expect(await h.listRevisions(PROJECT, 'main.tex')).toHaveLength(MAX_REVISIONS_PER_FILE);
  });
});

describe('project footprint', () => {
  it('writes only under the app-data history home, never into the project', async () => {
    const files = fakeDisk();
    const h = store();
    await h.recordRevision(PROJECT, 'main.tex', bytes('body'));

    for (const p of files.keys()) {
      expect(p.startsWith('/app/data/maleficium-history/')).toBe(true);
      expect(p.startsWith(ROOT)).toBe(false);
    }
  });

  it('hashes content, not paths, so identical files share a blob', async () => {
    const files = fakeDisk();
    const h = store();
    await h.recordRevision(PROJECT, 'a.tex', bytes('shared'));
    await h.recordRevision(PROJECT, 'b.tex', bytes('shared'));

    const blobs = [...files.keys()].filter((p) => p.includes('/blobs/'));
    expect(blobs).toHaveLength(1);
    expect(await hashBytes(bytes('shared'))).toHaveLength(64);
  });
});
