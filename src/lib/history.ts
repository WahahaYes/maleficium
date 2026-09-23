// history.ts — the revision-history seam: app-local snapshots of project files.
//
// Revisions are keyed by (projectId, relPath) and addressed by opaque ids.
// The store is the Rust core (shared with the MCP server); the desktop impl
// reaches it over IPC. The project dir never holds history.

import type { BatchFile, RecordOutcome, RetentionInfo, Revision } from './generated/events';

export type { BatchFile, RecordOutcome, RetentionInfo, Revision };

export interface HistoryStore {
  /** Snapshot the text a file now holds (called on save). */
  recordRevision(projectId: string, relPath: string, text: string): Promise<RecordOutcome>;
  /** Newest first; none when history is unreadable. */
  listRevisions(projectId: string, relPath: string): Promise<Revision[]>;
  getRevision(projectId: string, relPath: string, rev: string): Promise<Uint8Array | null>;
  /** Write the revision back to disk (the replaced state is kept first). */
  restoreRevision(projectId: string, relPath: string, rev: string): Promise<Uint8Array | null>;
  retentionInfo(projectId: string): Promise<RetentionInfo>;
  /** The files of one replace batch and the revisions holding their prior content. */
  batchFiles(projectId: string, batch: string): Promise<BatchFile[]>;
}

let impl: HistoryStore | null = null;

/** Register the platform implementation. Called once at boot. */
export function setHistoryStore(next: HistoryStore) {
  impl = next;
}

export function historyStore(): HistoryStore {
  if (!impl) throw new Error('history store not configured');
  return impl;
}
