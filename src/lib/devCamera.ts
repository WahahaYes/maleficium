// Dev builds only: camera moves for the video harness (e2e/showreel-run.py).
// The harness appends numbered moves, one JSON object per line, to the file
// named by SHOWREEL_CAMERA_FILE; the dev server serves it at /__camera, and
// the app runs each new move through the same handlers as its commands. No
// keystrokes, so a move can never type into a document. Release builds have
// no dev server and never load this.

export type CameraMove =
  | { seq: number; op: 'open'; rel: string }
  | { seq: number; op: 'line'; line: number }
  | { seq: number; op: 'page'; page: number }
  | { seq: number; op: 'command'; id: string };

const COMMANDS = new Set([
  'tools.forward-sync',
  'view.toggle-log',
  'view.zoom-fit-width',
  'view.zoom-fit-page',
]);

/** Moves after `afterSeq`, in order; malformed lines and unknown moves are skipped. */
export function newMoves(text: string, afterSeq: number): CameraMove[] {
  const out: CameraMove[] = [];
  for (const line of text.split('\n')) {
    if (!line.trim()) continue;
    let m: unknown;
    try {
      m = JSON.parse(line);
    } catch {
      continue;
    }
    if (!m || typeof m !== 'object') continue;
    const r = m as Record<string, unknown>;
    if (typeof r.seq !== 'number' || r.seq <= afterSeq) continue;
    if (
      r.op === 'open' &&
      typeof r.rel === 'string' &&
      !r.rel.startsWith('/') &&
      !r.rel.split('/').includes('..')
    ) {
      out.push({ seq: r.seq, op: 'open', rel: r.rel });
    } else if (r.op === 'line' && Number.isInteger(r.line) && (r.line as number) > 0) {
      out.push({ seq: r.seq, op: 'line', line: r.line as number });
    } else if (r.op === 'page' && Number.isInteger(r.page) && (r.page as number) > 0) {
      out.push({ seq: r.seq, op: 'page', page: r.page as number });
    } else if (r.op === 'command' && typeof r.id === 'string' && COMMANDS.has(r.id)) {
      out.push({ seq: r.seq, op: 'command', id: r.id });
    }
  }
  return out.sort((a, b) => a.seq - b.seq);
}
