import { request } from './core-request.tauri';
import { hasDir, joinPath, relUnder } from './paths';

/**
 * SyncTeX via the bundled sidecar, addressed by session root and
 * root-relative paths. The backend derives the pdf from the main file and
 * parses the tool's output. `{ok:false}` means a genuine query failure
 * (no compiled output, missing `.synctex.gz`, refused path).
 */
export type ForwardResult = { ok: boolean; page: number | null; error: string | null };
export type InverseResult = {
  ok: boolean;
  relPath: string | null;
  line: number | null;
  error: string | null;
};

/** Forward SyncTeX (editor → PDF): the page showing `line` of `texRel`. */
export async function forward_sync(
  rootId: string,
  mainRel: string,
  texRel: string,
  line: number,
): Promise<ForwardResult> {
  try {
    const hit = await request('forwardSync', {
      rootId,
      mainRel,
      texRel,
      line,
    });
    return { ok: true, page: hit.page, error: null };
  } catch (e) {
    return { ok: false, page: null, error: String(e) };
  }
}

/** Inverse SyncTeX (PDF → editor): the root-relative file and line at a point. */
export async function inverse_sync(
  rootId: string,
  mainRel: string,
  page: number,
  x = 0,
  y = 0,
): Promise<InverseResult> {
  try {
    const hit = await request('inverseSync', {
      rootId,
      mainRel,
      page,
      x,
      y,
    });
    return { ok: true, relPath: hit.relPath, line: hit.line, error: null };
  } catch (e) {
    return { ok: false, relPath: null, line: null, error: String(e) };
  }
}

/** Root-relative path of `abs`, or null when it lies outside `rootPath`. */
export function relTo(rootPath: string, abs: string): string | null {
  return relUnder(rootPath, abs);
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
  return hasDir(file) ? file : joinPath(workdirHint, file);
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
