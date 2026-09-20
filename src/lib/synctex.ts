import { invoke } from '@tauri-apps/api/core';
export type ForwardResult = { ok: boolean; text: string };
export type InverseResult = { ok: boolean; text: string };
/**
 * SyncTeX via the bundled sidecar. The tool ships with the app: no PATH
 * lookup, no "not installed" branch. `{ok:false}` means only a genuine
 * query failure (missing `.synctex.gz`, corrupt output).
 */
/**
 * Forward SyncTeX (editor → PDF).
 * `pdfPath` is the absolute outdir pdf path; `texPath` is the absolute
 * path of the visible source file — the gz stores absolute Input paths
 * per file, so pass the file the line belongs to.
 */
export async function forward_sync(
  pdfPath: string,
  texPath: string,
  line: number,
): Promise<ForwardResult> {
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
 * Missing fields stay null (`line == null` is no-match).
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
 * Inverse SyncTeX. The query runs inside the out dir (the tool resolves
 * `<pdf>.synctex.gz` relative to CWD), so the pdf's absolute path is split
 * into (outDir, pdfName) here. Failure contract: `{ok:false}`.
 */
export async function inverse_sync(
  pdfAbsPath: string,
  page: number,
  x = 0,
  y = 0,
): Promise<InverseResult> {
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

/**
 * SyncTeX is unavailable without a rendered pdf, and while a compile runs:
 * the `.synctex.gz` is being rewritten, so any answer would describe the
 * previous document.
 */
export function syncAvailable(pdfUrl: string | null, compiling: boolean): boolean {
  return pdfUrl != null && pdfUrl !== '' && !compiling;
}

/** Absolute source path for a query: bare names resolve against the workdir. */
export function texPathFor(file: string, workdirHint: string): string {
  return file.includes('/') ? file : workdirHint + '/' + file;
}

/**
 * Whether a forward hit should move the preview. Preamble and untagged
 * lines resolve to a rect on the current page: arriving without moving is
 * noise, not navigation.
 */
export function shouldTurnPage(target: number | null, current: number): target is number {
  return target != null && target !== current;
}

/**
 * Whether an inverse hit lands in a different file than the open one.
 * SyncTeX names the owning file in multi-file projects; a null or matching
 * name means reveal the line in the current buffer.
 */
export function isCrossFileHit(hitFile: string | null, openFile: string): hitFile is string {
  return hitFile != null && hitFile !== '' && hitFile !== openFile;
}
