import { invoke } from '@tauri-apps/api/core';
export type ForwardResult = { ok: boolean; text: string };
export type InverseResult = { ok: boolean; text: string };
/**
 * SyncTeX via the bundled sidecar (`src-tauri/binaries/synctex-<triple>`,
 * built from `jlaurens/synctex` MIT — credit in About). The tool ships with
 * the app: no PATH lookup, no "not installed" branch. `{ok:false}` now means
 * only a genuine query failure (missing `.synctex.gz`, corrupt output).
 */
/**
 * Forward SyncTeX (editor → PDF).
 * `pdfPath` is the absolute outdir pdf path (Rust splits it into
 * outdir + bare name and runs inside the outdir); `texPath` is the
 * ABSOLUTE path of the VISIBLE source file (`synctex view
 * -i <line>:1:<tex>`) — the gz stores absolute Input paths per file, so
 * callers pass the file the line belongs to, not the project main file.
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
 * Parse `synctex view` output for the target page (`Page: N` line).
 * Returns the 1-based page number, or null when no `Page:` line matches.
 */
export function parseForwardSync(text: string): number | null {
  const m = text.match(/^Page:\s*(\d+)\s*$/m);
  return m ? Math.max(1, parseInt(m[1], 10)) : null;
}

/**
 * True when `synctex view` output is a genuine no-match response.
 */
export function isForwardNoMatch(text: string): boolean {
  return text.includes('no_match') || text.includes('No tag for') || text === '{}';
}

/**
 * Parse `synctex edit` output (`Input:<abs path>` + `Line:<n>` lines).
 * Missing fields stay null — callers treat `line == null` as no-match.
 */
export function parseInverseSync(text: string): { line: number | null; hitFile: string | null } {
  const lm = text.match(/^Line:\s*(\d+)\s*$/m);
  const im = text.match(/^Input:\s*(.+?)\s*$/m);
  return {
    line: lm ? parseInt(lm[1], 10) : null,
    hitFile: im ? im[1].trim() : null,
  };
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
