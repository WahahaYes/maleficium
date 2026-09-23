// project-index.tauri.ts — desktop implementation of the project index seam:
// the Rust core over IPC.

import { invoke } from '@tauri-apps/api/core';
import type { ProjectIndexProvider } from './project-index';

export const desktopProjectIndex: ProjectIndexProvider = {
  open: (rootId) => invoke('index_open', { rootId }),
  watched: (rootId, watched) => invoke('index_watched', { rootId, watched }),
  touch: (rootId, paths) => invoke('index_touch', { rootId, paths }),
  overlay: (rootId, rel, text) => invoke('index_overlay', { rootId, rel, text }),
  search: (rootId, query, mainRel) => invoke('index_search', { rootId, query, mainRel }),
  findFiles: (rootId, query) => invoke('index_find_files', { rootId, query }),
  rank: (query, items) => invoke('fuzzy_rank', { query, items }),
  replacePreview: (rootId, query, replacement, mainRel) =>
    invoke('index_replace_preview', { rootId, query, replacement, mainRel }),
  replaceApply: (rootId, token, keepOpen) =>
    invoke('index_replace_apply', { rootId, token, keepOpen }),
};
