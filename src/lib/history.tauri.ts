// history.tauri.ts — desktop implementation of the history seam: the Rust
// store over the operation contract, with raw revision bytes.

import { invoke } from '@tauri-apps/api/core';
import { request } from './core-request.tauri';
import type { HistoryStore } from './history';

async function bytesOrNull(p: Promise<ArrayBuffer>): Promise<Uint8Array | null> {
  try {
    return new Uint8Array(await p);
  } catch {
    return null;
  }
}

export const desktopHistory: HistoryStore = {
  listRevisions: (rootId, rel) => request('historyList', { rootId, rel }),
  getRevision: (rootId, rel, rev) => bytesOrNull(invoke('history_get', { rootId, rel, rev })),
  restoreRevision: (rootId, rel, rev) =>
    bytesOrNull(invoke('history_restore', { rootId, rel, rev })),
  retentionInfo: (rootId) => request('historyRetention', { rootId }),
  batchFiles: (rootId, batch) => request('historyBatchFiles', { rootId, batch }),
};
