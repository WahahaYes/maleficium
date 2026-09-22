// structure.ts — the document-structure seam.
//
// One parser for every surface: the Rust `maleficium-structure` crate. The
// desktop impl reaches it over IPC; a web client compiles the same crate to
// wasm and runs it in the page (no server round trip). Callers hand over
// text they already hold (the live buffer, the engine log) and get records
// back; nothing here touches the fs.

/** What an outline row IS: a sectioning command, or a marker riding the tree. */
export type OutlineKind = 'section' | 'label' | 'figure' | 'table' | 'input';

export interface OutlineEntry {
  level: number;
  title: string;
  line: number;
  kind: OutlineKind;
  /** Marker detail: label key, float graphics file, or input path. Never shown raw. */
  detail?: string;
}

export interface Outline {
  entries: OutlineEntry[];
  /** Rows dropped by the parse cap (1000). */
  truncated: number;
}

/** One engine-log entry. `path` is root-relative; absent when `external`. */
export interface Diagnostic {
  path?: string;
  line: number;
  message: string;
  severity: 'error' | 'warning';
  external: boolean;
}

export interface StructureProvider {
  outline(text: string): Promise<Outline>;
  /** `root` and `base` (the engine's cwd) only resolve paths in `log`. */
  diagnostics(log: string, root: string, base: string): Promise<Diagnostic[]>;
}

let impl: StructureProvider | null = null;

/** Register the platform implementation. Called once at boot. */
export function setStructure(next: StructureProvider) {
  impl = next;
}

export function structure(): StructureProvider {
  if (!impl) throw new Error('structure provider not configured');
  return impl;
}
