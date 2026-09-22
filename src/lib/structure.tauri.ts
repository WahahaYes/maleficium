// structure.tauri.ts — desktop implementation of the structure seam: the
// Rust crate over IPC.

import { invoke } from '@tauri-apps/api/core';
import type { Diagnostic, Outline } from './generated/structure';
import type { StructureProvider } from './structure';

export const desktopStructure: StructureProvider = {
  outline: (text) => invoke<Outline>('structure_outline', { text }),
  diagnostics: (log, root, base) =>
    invoke<Diagnostic[]>('structure_diagnostics', { log, root, base }),
};
