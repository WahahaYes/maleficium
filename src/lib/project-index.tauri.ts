// project-index.tauri.ts — desktop implementation of the project index seam:
// the Rust core over IPC.

import { invoke } from '@tauri-apps/api/core';
import type { ProjectIndexProvider } from './project-index';

export const desktopProjectIndex: ProjectIndexProvider = {
  open: (rootId) => invoke('index_open', { rootId }),
  watched: (rootId, watched) => invoke('index_watched', { rootId, watched }),
  touch: (rootId, paths) => invoke('index_touch', { rootId, paths }),
  overlay: (rootId, rel, text) => invoke('index_overlay', { rootId, rel, text }),
};
