import { invoke } from '@tauri-apps/api/core';

export async function gitStatus(root: string): Promise<{ ok: boolean; text: string }> {
  try {
    const text = await invoke<string>('git_status', { root });
    return { ok: true, text };
  } catch {
    return { ok: false, text: '' };
  }
}