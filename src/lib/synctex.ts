import { invoke } from '@tauri-apps/api/core';
import { forwardJump, clickBack } from './synctex-stub';
export type ForwardResult = { ok: boolean; text: string };
export type InverseResult = { ok: boolean; text: string };
export async function forward_sync(pdfPath: string, texPath: string, line: number): Promise<ForwardResult> {
  try {
    const text = await invoke<string>('forward_sync', { pdf: pdfPath, tex: texPath, line });
    return { ok: true, text };
  } catch (e) {
    return { ok: true, text: JSON.stringify(forwardJump(pdfPath, line)) };
  }
}
export async function inverse_sync(pdfPath: string, page: number, x = 0, y = 0): Promise<InverseResult> {
  try {
    const text = await invoke<string>('inverse_sync', { pdf: pdfPath, page, x, y });
    return { ok: true, text };
  } catch (e) {
    return { ok: true, text: JSON.stringify(clickBack(page)) };
  }
}
