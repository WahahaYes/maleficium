import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

/**
 * Compile backend contract, addressed by session root and the main file's
 * root-relative path. The backend owns where outputs live: callers never
 * name or derive the outdir.
 *
 * `ok:true` → `pdfUrl` locates the pdf (desktop: an absolute path; hosted:
 * an opaque URL), `log` is empty. `ok:false` → `pdfUrl` is null, `log` is
 * the error message. Engine lines stream via `compile-line` during the run.
 */
export type CompileResult = { ok: boolean; pdfUrl: string | null; log: string };

export function onCompileLine(cb: (line: string) => void): Promise<() => void> {
  return listen<string>('compile-line', (e) => cb(e.payload));
}

export async function compileTex(rootId: string, mainRel: string): Promise<CompileResult> {
  try {
    const pdfUrl = await invoke<string>('compile_tex', { rootId, mainRel });
    return { ok: true, pdfUrl, log: '' };
  } catch (e) {
    return { ok: false, pdfUrl: null, log: String(e) };
  }
}

export async function cancelCompile(): Promise<string> {
  return await invoke<string>('cancel_compile');
}

/** The full engine log of the last compile, or null when there is none. */
export async function engineLog(rootId: string, mainRel: string): Promise<string | null> {
  try {
    return await invoke<string>('engine_log', { rootId, mainRel });
  } catch {
    return null;
  }
}

/** Whether a previous compile left a pdf. Never throws. */
export async function outputsFresh(rootId: string, mainRel: string): Promise<boolean> {
  try {
    return await invoke<boolean>('outputs_fresh', { rootId, mainRel });
  } catch {
    return false;
  }
}

export type CleanResult = { ok: boolean; removed: number; error: string | null };

/** Remove the main file's build artifacts. Sources are never touched. */
export async function cleanOutputs(rootId: string, mainRel: string): Promise<CleanResult> {
  try {
    const removed = await invoke<number>('clean_outputs', { rootId, mainRel });
    return { ok: true, removed, error: null };
  } catch (e) {
    return { ok: false, removed: 0, error: String(e) };
  }
}
