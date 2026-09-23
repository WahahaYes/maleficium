// history.tauri.ts — desktop implementation of the history seam: the Rust
// store over IPC. Revision bytes come back raw.

import { invoke } from '@tauri-apps/api/core';
import type { HistoryStore } from './history';

async function bytesOrNull(p: Promise<ArrayBuffer>): Promise<Uint8Array | null> {
  try {
    return new Uint8Array(await p);
  } catch {
    return null;
  }
}

export const desktopHistory: HistoryStore = {
  recordRevision: (rootId, rel, text) => invoke('history_record', { rootId, rel, text }),
  listRevisions: (rootId, rel) => invoke('history_list', { rootId, rel }),
  getRevision: (rootId, rel, rev) => bytesOrNull(invoke('history_get', { rootId, rel, rev })),
  restoreRevision: (rootId, rel, rev) =>
    bytesOrNull(invoke('history_restore', { rootId, rel, rev })),
  retentionInfo: (rootId) => invoke('history_retention', { rootId }),
  batchFiles: (rootId, batch) => invoke('history_batch_files', { rootId, batch }),
};
