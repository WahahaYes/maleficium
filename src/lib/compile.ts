import { invoke } from '@tauri-apps/api/core';

export type CompileResult = { ok: boolean, pdfPath: string | null, log: string };

export async function compileTex(inputPath: string, workdir: string): Promise<CompileResult> {
  try {
    const log = await invoke<string>('compile_tex', { input: inputPath, workdir });
    return { ok: true, pdfPath: log, log };
  } catch (e) {
    return { ok: false, pdfPath: null, log: String(e) };
  }
}