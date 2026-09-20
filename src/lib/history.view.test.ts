import { describe, it, expect } from 'vitest';
import {
  HISTORY_EMPTY,
  HISTORY_UNAVAILABLE,
  buildRevisionRows,
  formatSize,
  formatWhen,
  historyAvailability,
  retentionSummary,
  truncationNotice,
} from './history.view';
import {
  MAX_HISTORY_BYTES_PER_PROJECT,
  MAX_REVISIONS_PER_FILE,
  MIN_REVISIONS_KEPT_PER_FILE,
  SNAPSHOT_MAX_FILE_BYTES,
  type RetentionInfo,
} from './history';

const info = (over: Partial<RetentionInfo> = {}): RetentionInfo => ({
  maxRevisionsPerFile: MAX_REVISIONS_PER_FILE,
  maxHistoryBytesPerProject: MAX_HISTORY_BYTES_PER_PROJECT,
  minRevisionsKeptPerFile: MIN_REVISIONS_KEPT_PER_FILE,
  snapshotMaxFileBytes: SNAPSHOT_MAX_FILE_BYTES,
  revisions: 3,
  bytes: 4096,
  ...over,
});

describe('revision availability', () => {
  it('lists only for a project file with a reachable store', () => {
    expect(
      historyAvailability({ hasProject: true, relPath: 'chapters/a.tex', storeReady: true }),
    ).toBe('ready');
  });
  it('degrades quietly with no project, no project file, or no store', () => {
    expect(historyAvailability({ hasProject: false, relPath: 'a.tex', storeReady: true })).toBe(
      'unavailable',
    );
    expect(historyAvailability({ hasProject: true, relPath: null, storeReady: true })).toBe(
      'unavailable',
    );
    expect(historyAvailability({ hasProject: true, relPath: 'a.tex', storeReady: false })).toBe(
      'unavailable',
    );
  });
  it('speaks user words, never an error and never version-control jargon', () => {
    for (const m of [HISTORY_UNAVAILABLE, HISTORY_EMPTY]) {
      expect(m).not.toMatch(/git|HEAD|commit|branch|repo|error|failed/i);
    }
    expect(HISTORY_UNAVAILABLE).toBe('History unavailable');
  });
});

describe('revision row formatting', () => {
  it('ages in whole units, then a stable calendar date', () => {
    const now = Date.UTC(2026, 8, 20, 12, 0, 0);
    expect(formatWhen(now - 5_000, now)).toBe('just now');
    expect(formatWhen(now - 60_000, now)).toBe('1 minute ago');
    expect(formatWhen(now - 5 * 60_000, now)).toBe('5 minutes ago');
    expect(formatWhen(now - 3 * 3_600_000, now)).toBe('3 hours ago');
    expect(formatWhen(now - 2 * 86_400_000, now)).toBe('2 days ago');
    expect(formatWhen(Date.UTC(2020, 0, 2), now)).toBe('2020-01-02');
  });
  it('sizes read in whole units', () => {
    expect(formatSize(512)).toBe('512 B');
    expect(formatSize(2048)).toBe('2 KB');
    expect(formatSize(3 * 1024 * 1024)).toBe('3.0 MB');
  });
  it('marks the newest row, preserving store order', () => {
    const now = Date.UTC(2026, 8, 20, 12, 0, 0);
    const rows = buildRevisionRows(
      [
        { rev: '9', at: now - 60_000, bytes: 100 },
        { rev: '8', at: now - 7_200_000, bytes: 90 },
      ],
      now,
    );
    expect(rows.map((r) => r.rev)).toEqual(['9', '8']);
    expect(rows.map((r) => r.latest)).toEqual([true, false]);
    expect(rows[1].when).toBe('2 hours ago');
  });
});

describe('retention honesty', () => {
  it('summary states usage against the project cap', () => {
    expect(retentionSummary(info({ revisions: 1, bytes: 1024 }))).toBe(
      '1 revision kept · 1 KB of 256.0 MB',
    );
  });
  it('names the per-file cap only when the list is standing at it', () => {
    expect(truncationNotice(MAX_REVISIONS_PER_FILE - 1, info())).toBeNull();
    const notice = truncationNotice(MAX_REVISIONS_PER_FILE, info());
    expect(notice).toContain(String(MAX_REVISIONS_PER_FILE));
    expect(notice).toMatch(/older ones are removed automatically/);
  });
});
