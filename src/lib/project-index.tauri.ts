// project-index.tauri.ts — desktop implementation of the project index seam:
// the Rust core over the operation contract.

import { request } from './core-request.tauri';
import type { ProjectIndexProvider } from './project-index';

export const desktopProjectIndex: ProjectIndexProvider = {
  open: (rootId) => request('indexOpen', { rootId }),
  watched: (rootId, watched) => request('indexWatched', { rootId, watched }).then(() => undefined),
  touch: (rootId, paths) => request('indexTouch', { rootId, paths }).then(() => undefined),
  overlay: (rootId, rel, text) =>
    request('indexOverlay', { rootId, rel, text }).then(() => undefined),
  search: (rootId, query, mainRel) => request('indexSearch', { rootId, query, mainRel, max: null }),
  findFiles: (rootId, query) => request('indexFindFiles', { rootId, query, max: null }),
  rank: (query, items) => request('fuzzyRank', { query, items, max: null }),
  definitionAt: (rootId, line, col, mainRel) =>
    request('indexDefinitionAt', { rootId, line, col, mainRel }),
  macros: (rootId) => request('indexMacros', { rootId }),
  replacePreview: (rootId, query, replacement, mainRel) =>
    request('indexReplacePreview', { rootId, query, replacement, mainRel }),
  replaceApply: (rootId, token, keepOpen) =>
    request('indexReplaceApply', { rootId, token, keepOpen }),
};
