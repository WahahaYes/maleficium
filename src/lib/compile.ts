import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

/**
 * `compile_tex` backend contract (`src-tauri/src/commands/compile.rs`):
 * success returns the absolute outdir pdf path (`Ok(pdf…)`), failure
 * returns an error string. The engine log is NOT in this return — it
 * streams via `compile-line` events during the run, and the full `.log`
 * file lives in the app-local outdir (read separately).
 *
 * Hence: `ok:true` → `pdfPath` is the pdf path, `log` is empty;
 * `ok:false` → `pdfPath` is null, `log` is the error message.
 */
export type CompileResult = { ok: boolean; pdfPath: string | null; log: string };

export function onCompileLine(cb: (line: string) => void): Promise<() => void> {
  return listen<string>('compile-line', (e) => cb(e.payload));
}

export async function compileTex(inputPath: string, workdir: string): Promise<CompileResult> {
  try {
    const pdfPath = await invoke<string>('compile_tex', { input: inputPath, workdir });
    return { ok: true, pdfPath, log: '' };
  } catch (e) {
    return { ok: false, pdfPath: null, log: String(e) };
  }
}

export async function cancelCompile(): Promise<string> {
  return await invoke<string>('cancel_compile');
}
