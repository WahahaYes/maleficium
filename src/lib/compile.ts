import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

export type CompileResult = { ok: boolean, pdfPath: string | null, log: string };

export function onCompileLine(cb:(line:string)=>void):Promise<()=>void> { return listen<string>('compile-line', (e)=>cb(e.payload)); }

export async function compileTex(inputPath: string, workdir: string): Promise<CompileResult> {
  try {
    const log = await invoke<string>('compile_tex', { input: inputPath, workdir });
    return { ok: true, pdfPath: log, log };
  } catch (e) {
    return { ok: false, pdfPath: null, log: String(e) };
  }
}

export async function cancelCompile(): Promise<string> { return await invoke<string>('cancel_compile'); }