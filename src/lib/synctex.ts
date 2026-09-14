import { invoke } from '@tauri-apps/api/core';
export type ForwardResult = { ok: boolean; text: string };
export type InverseResult = { ok: boolean; text: string };
/**
 * SyncTeX via the bundled sidecar (`src-tauri/binaries/synctex-<triple>`,
 * built from `jlaurens/synctex` MIT — credit in About). The tool ships with
 * the app: no PATH lookup, no "not installed" branch. `{ok:false}` now means
 * only a genuine query failure (missing `.synctex.gz`, corrupt output).
 */
export async function forward_sync(pdfPath: string, texPath: string, line: number): Promise<ForwardResult> {
  try {
    const text = await invoke<string>('forward_sync', { pdf: pdfPath, tex: texPath, line });
    return { ok: true, text };
  } catch (e) {
    return { ok: false, text: String(e) };
  }
}
/**
 * Inverse SyncTeX. The Rust side runs `synctex edit` INSIDE the out dir (the
 * tool resolves `<pdf>.synctex.gz` relative to CWD), so callers pass the pdf's
 * absolute path and we split it into (outDir, pdfName) here.
 * Failure contract: `{ok:false}` — see `forward_sync` above.
 */
export async function inverse_sync(pdfAbsPath: string, page: number, x = 0, y = 0): Promise<InverseResult> {
  const slash = pdfAbsPath.lastIndexOf('/');
  const synctexDir = slash > 0 ? pdfAbsPath.slice(0, slash) : '.';
  const pdfName = slash >= 0 ? pdfAbsPath.slice(slash + 1) : pdfAbsPath;
  try {
    const text = await invoke<string>('inverse_sync', { synctexDir, pdfName, page, x, y });
    return { ok: true, text };
  } catch (e) {
    return { ok: false, text: String(e) };
  }
}
