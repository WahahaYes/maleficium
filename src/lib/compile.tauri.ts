// compile.tauri.ts — desktop transport for streaming compiles.
//
// Request/response ops travel over the core contract; the live run streams
// outside it through the dedicated compile_tex command plus compile-line
// window events.

import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { CompileLine, CompileReport } from './generated/events';

/** Start one streaming compile; lines arrive on compile-line events. */
export function runCompileTex(
  rootId: string,
  mainRel: string,
  networked: boolean,
): Promise<CompileReport> {
  return invoke<CompileReport>('compile_tex', { rootId, mainRel, networked });
}

/** Cancel the running compile, if any. */
export function cancelCompileRun(): Promise<string> {
  return invoke<string>('cancel_compile');
}

/** Subscribe to compile lines; the resolver unsubscribes. */
export function subscribeCompileLines(cb: (line: CompileLine) => void): Promise<() => void> {
  return listen<CompileLine>('compile-line', (e) => cb(e.payload));
}
