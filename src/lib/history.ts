// history.ts — app-local revision history: content-addressed, snapshot-on-save.
//
// Revisions are captured when a file is saved. Bytes live once per distinct
// content under a blob tree; the index maps (projectId, relPath) to the
// revisions that reference them. Everything lives app-local — the project
// dir holds nothing of ours and the user's own repo is never touched.

import { mkdir, readFile, writeFile, remove, exists } from '@tauri-apps/plugin-fs';
import { appDataDir } from '@tauri-apps/api/path';
import { appHistoryDir, historyBlobPath, joinPath } from './paths';
import { previewKindFor } from './files';

/** Hard cap per file. Oldest revisions are evicted first. */
export const MAX_REVISIONS_PER_FILE = 50;
/** Soft cap on summed distinct blob bytes for one project. */
export const MAX_HISTORY_BYTES_PER_PROJECT = 256 * 1024 * 1024;
/** Byte-cap eviction never takes a file below this many revisions. */
export const MIN_REVISIONS_KEPT_PER_FILE = 5;
/** Files larger than this are not snapshotted. */
export const SNAPSHOT_MAX_FILE_BYTES = 2 * 1024 * 1024;

/** One revision as it crosses the seam: opaque id, no hash, no path. */
export interface Revision {
  rev: string;
  at: number;
  bytes: number;
}

export interface RetentionInfo {
  maxRevisionsPerFile: number;
  maxHistoryBytesPerProject: number;
  minRevisionsKeptPerFile: number;
  snapshotMaxFileBytes: number;
  /** Revisions currently held across the project. */
  revisions: number;
  /** Summed distinct blob bytes currently held. */
  bytes: number;
}

/** Why a save produced no revision. `stored` means one was written. */
export type RecordOutcome =
  | { stored: true; rev: string; deduped: boolean }
  | { stored: false; reason: 'not-text' | 'too-large' | 'unchanged' | 'unavailable' };

export interface HistoryStore {
  recordRevision(projectId: string, relPath: string, bytes: Uint8Array): Promise<RecordOutcome>;
  listRevisions(projectId: string, relPath: string): Promise<Revision[]>;
  getRevision(projectId: string, relPath: string, rev: string): Promise<Uint8Array | null>;
  restoreRevision(projectId: string, relPath: string, rev: string): Promise<Uint8Array | null>;
  retentionInfo(projectId: string): Promise<RetentionInfo>;
}

interface Entry {
  rev: string;
  hash: string;
  at: number;
  bytes: number;
}

export interface Index {
  v: 1;
  seq: number;
  files: Record<string, Entry[]>;
}

export function emptyIndex(): Index {
  return { v: 1, seq: 0, files: {} };
}

/** SHA-256 hex over the exact bytes. Web Crypto: no added dependency. */
export async function hashBytes(bytes: Uint8Array): Promise<string> {
  const buf = await crypto.subtle.digest('SHA-256', bytes as unknown as ArrayBuffer);
  return Array.from(new Uint8Array(buf))
    .map((b) => b.toString(16).padStart(2, '0'))
    .join('');
}

function indexPath(historyDir: string): string {
  return joinPath(historyDir, 'index.json');
}

/** Parse a stored index, rejecting anything that is not the current shape. */
function parseIndex(raw: string): Index {
  const j = JSON.parse(raw) as unknown;
  if (typeof j !== 'object' || j === null) return emptyIndex();
  const o = j as Partial<Index>;
  if (o.v !== 1 || typeof o.seq !== 'number' || typeof o.files !== 'object' || !o.files) {
    return emptyIndex();
  }
  const files: Record<string, Entry[]> = {};
  for (const [rel, list] of Object.entries(o.files)) {
    if (!Array.isArray(list)) continue;
    files[rel] = list.filter(
      (e): e is Entry =>
        typeof e === 'object' &&
        e !== null &&
        typeof (e as Entry).rev === 'string' &&
        typeof (e as Entry).hash === 'string' &&
        typeof (e as Entry).at === 'number' &&
        typeof (e as Entry).bytes === 'number',
    );
  }
  return { v: 1, seq: o.seq, files };
}

/** Distinct blob bytes referenced by the whole index. */
function distinctBytes(index: Index): number {
  const seen = new Map<string, number>();
  for (const list of Object.values(index.files)) {
    for (const e of list) seen.set(e.hash, e.bytes);
  }
  let total = 0;
  for (const b of seen.values()) total += b;
  return total;
}

function countRevisions(index: Index): number {
  let n = 0;
  for (const list of Object.values(index.files)) n += list.length;
  return n;
}

/**
 * Evict to the caps: per-file first, then project bytes oldest-first across
 * files, never taking a file below the floor. Mutates `index`.
 */
export function evict(index: Index): void {
  for (const [rel, list] of Object.entries(index.files)) {
    if (list.length > MAX_REVISIONS_PER_FILE) {
      index.files[rel] = list.slice(list.length - MAX_REVISIONS_PER_FILE);
    }
  }
  while (distinctBytes(index) > MAX_HISTORY_BYTES_PER_PROJECT) {
    let oldestRel: string | null = null;
    let oldestAt = Infinity;
    for (const [rel, list] of Object.entries(index.files)) {
      if (list.length <= MIN_REVISIONS_KEPT_PER_FILE) continue;
      const first = list[0];
      if (first && first.at < oldestAt) {
        oldestAt = first.at;
        oldestRel = rel;
      }
    }
    if (oldestRel === null) return;
    index.files[oldestRel] = index.files[oldestRel].slice(1);
  }
}

/** Hashes still referenced after eviction. */
export function liveHashes(index: Index): Set<string> {
  const live = new Set<string>();
  for (const list of Object.values(index.files)) {
    for (const e of list) live.add(e.hash);
  }
  return live;
}

/** True when this path is eligible for snapshotting at all. */
export function isSnapshotEligible(relPath: string, byteLength: number): RecordOutcome | null {
  if (previewKindFor(relPath) !== 'text') return { stored: false, reason: 'not-text' };
  if (byteLength > SNAPSHOT_MAX_FILE_BYTES) return { stored: false, reason: 'too-large' };
  return null;
}

/**
 * Desktop history store: app-data blob tree plus a per-project JSON index.
 * `rootFor` resolves a project id to its absolute root — absolute paths live
 * only inside this impl and never cross the seam.
 */
export function createHistoryStore(rootFor: (projectId: string) => string | null): HistoryStore {
  async function dirFor(projectId: string): Promise<string> {
    const base = await appDataDir();
    return appHistoryDir(base, projectId);
  }

  async function readIndex(historyDir: string): Promise<Index> {
    try {
      if (!(await exists(indexPath(historyDir)))) return emptyIndex();
      const raw = await readFile(indexPath(historyDir));
      return parseIndex(new TextDecoder().decode(raw));
    } catch {
      return emptyIndex();
    }
  }

  async function writeIndex(historyDir: string, index: Index): Promise<void> {
    await mkdir(historyDir, { recursive: true });
    await writeFile(indexPath(historyDir), new TextEncoder().encode(JSON.stringify(index)));
  }

  /** Delete every blob the index no longer references. */
  async function sweep(historyDir: string, index: Index, touched: Set<string>): Promise<void> {
    const live = liveHashes(index);
    for (const hash of touched) {
      if (live.has(hash)) continue;
      try {
        await remove(historyBlobPath(historyDir, hash));
      } catch {
        /* already gone — the index is the only truth that matters */
      }
    }
  }

  return {
    async recordRevision(projectId, relPath, bytes) {
      const ineligible = isSnapshotEligible(relPath, bytes.byteLength);
      if (ineligible) return ineligible;

      let historyDir: string;
      try {
        historyDir = await dirFor(projectId);
      } catch {
        return { stored: false, reason: 'unavailable' };
      }

      try {
        const hash = await hashBytes(bytes);
        const index = await readIndex(historyDir);
        const list = index.files[relPath] ?? [];
        const last = list[list.length - 1];
        if (last && last.hash === hash) return { stored: false, reason: 'unchanged' };

        const blob = historyBlobPath(historyDir, hash);
        const deduped = await exists(blob);
        if (!deduped) {
          await mkdir(joinPath(historyDir, 'blobs', hash.slice(0, 2)), { recursive: true });
          await writeFile(blob, bytes);
        }

        index.seq += 1;
        const rev = String(index.seq);
        index.files[relPath] = [...list, { rev, hash, at: Date.now(), bytes: bytes.byteLength }];

        const before = liveHashes(index);
        evict(index);
        await writeIndex(historyDir, index);
        await sweep(historyDir, index, before);

        return { stored: true, rev, deduped };
      } catch {
        return { stored: false, reason: 'unavailable' };
      }
    },

    async listRevisions(projectId, relPath) {
      try {
        const index = await readIndex(await dirFor(projectId));
        const list = index.files[relPath] ?? [];
        return list.map((e) => ({ rev: e.rev, at: e.at, bytes: e.bytes })).reverse();
      } catch {
        return [];
      }
    },

    async getRevision(projectId, relPath, rev) {
      try {
        const historyDir = await dirFor(projectId);
        const index = await readIndex(historyDir);
        const entry = (index.files[relPath] ?? []).find((e) => e.rev === rev);
        if (!entry) return null;
        return await readFile(historyBlobPath(historyDir, entry.hash));
      } catch {
        return null;
      }
    },

    async restoreRevision(projectId, relPath, rev) {
      const bytes = await this.getRevision(projectId, relPath, rev);
      if (!bytes) return null;
      const root = rootFor(projectId);
      if (!root) return null;
      // Snapshot what is on disk first, so restore is itself undoable.
      try {
        const current = await readFile(joinPath(root, relPath));
        await this.recordRevision(projectId, relPath, current);
      } catch {
        /* nothing on disk to preserve */
      }
      await writeFile(joinPath(root, relPath), bytes);
      return bytes;
    },

    async retentionInfo(projectId) {
      const base = {
        maxRevisionsPerFile: MAX_REVISIONS_PER_FILE,
        maxHistoryBytesPerProject: MAX_HISTORY_BYTES_PER_PROJECT,
        minRevisionsKeptPerFile: MIN_REVISIONS_KEPT_PER_FILE,
        snapshotMaxFileBytes: SNAPSHOT_MAX_FILE_BYTES,
      };
      try {
        const index = await readIndex(await dirFor(projectId));
        return { ...base, revisions: countRevisions(index), bytes: distinctBytes(index) };
      } catch {
        return { ...base, revisions: 0, bytes: 0 };
      }
    },
  };
}
