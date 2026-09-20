// history.view.ts — view model for the revision surface: availability and
// row formatting. Pure functions, no fs and no React, so the surface's
// wording and its honesty about caps are pinned headlessly.

import type { Revision, RetentionInfo } from './history';

/** Shown verbatim whenever revisions cannot be listed. Never an error. */
export const HISTORY_UNAVAILABLE = 'History unavailable';

/** Shown when a file is eligible but has not been saved yet. */
export const HISTORY_EMPTY = 'No revisions yet — revisions are kept as you save.';

export type HistoryAvailability = 'ready' | 'unavailable';

/**
 * Revisions are listable only for a file inside an open project with a
 * reachable store. Everything else degrades to one quiet message.
 */
export function historyAvailability(input: {
  hasProject: boolean;
  relPath: string | null;
  storeReady: boolean;
}): HistoryAvailability {
  if (!input.hasProject) return 'unavailable';
  if (!input.relPath) return 'unavailable';
  if (!input.storeReady) return 'unavailable';
  return 'ready';
}

/** One revision as the list renders it. */
export interface RevisionRow {
  rev: string;
  when: string;
  size: string;
  /** Newest revision — matches what is on disk after the last save. */
  latest: boolean;
}

/** Whole units up to days, then a stable calendar date. */
export function formatWhen(at: number, now: number): string {
  const secs = Math.max(0, Math.floor((now - at) / 1000));
  if (secs < 45) return 'just now';
  const mins = Math.floor(secs / 60);
  if (mins < 60) return `${mins} minute${mins === 1 ? '' : 's'} ago`;
  const hours = Math.floor(mins / 60);
  if (hours < 24) return `${hours} hour${hours === 1 ? '' : 's'} ago`;
  const days = Math.floor(hours / 24);
  if (days < 30) return `${days} day${days === 1 ? '' : 's'} ago`;
  return new Date(at).toISOString().slice(0, 10);
}

export function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

/** Newest first, as `listRevisions` returns them. */
export function buildRevisionRows(revisions: Revision[], now: number): RevisionRow[] {
  return revisions.map((r, i) => ({
    rev: r.rev,
    when: formatWhen(r.at, now),
    size: formatSize(r.bytes),
    latest: i === 0,
  }));
}

/** Current usage against the caps, so the list is never silently partial. */
export function retentionSummary(info: RetentionInfo): string {
  return `${info.revisions} revision${info.revisions === 1 ? '' : 's'} kept · ${formatSize(
    info.bytes,
  )} of ${formatSize(info.maxHistoryBytesPerProject)}`;
}

/** Names the cap when this file's list is standing at it, else null. */
export function truncationNotice(rowCount: number, info: RetentionInfo): string | null {
  if (rowCount < info.maxRevisionsPerFile) return null;
  return `Showing the newest ${info.maxRevisionsPerFile} revisions of this file; older ones are removed automatically.`;
}
