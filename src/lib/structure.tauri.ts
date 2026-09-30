// structure.tauri.ts — desktop implementation of the structure seam: the
// Rust crate over the operation contract.

import { request } from './core-request.tauri';
import type { StructureProvider } from './structure';

export const desktopStructure: StructureProvider = {
  outline: (text) => request('structureOutline', { text }),
  diagnostics: (log, root, base) => request('structureDiagnostics', { log, root, base }),
};
