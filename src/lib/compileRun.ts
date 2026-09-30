// compileRun.ts — the side channels of one compile run: engine lines, the
// still-compiling heartbeat, and the main-thread frame probe. None of it
// touches React state.

import { onCompileLine } from './compile';
import { emit } from './events';
import type { Actor } from './generated/events';

type Emit = (e: Parameters<typeof emit>[0]) => void;

/**
 * Forward the engine's lines to the bus. Fetch lines arrive typed: each
 * becomes `compile.fetch`, so a long first build reads as network-wait, not an
 * engine hang. A failed fetch is reported once per file (the engine retries
 * and repeats itself). Returns the detach function; attaching is best-effort.
 */
export async function forwardCompileLines(actor: Actor, emitProgress: Emit): Promise<() => void> {
  const failedFetches = new Set<string>();
  try {
    return await onCompileLine((line) => {
      const sig = line.signal;
      if (sig?.kind === 'phase') {
        emitProgress({
          scope: 'compile',
          kind: 'progress',
          actor,
          message: line.text.slice(0, 300),
          event: { action: 'compile.phase', phase: sig.phase, detail: sig.detail },
        });
      } else if (sig?.kind === 'fetch') {
        if (sig.outcome === 'failed') {
          if (failedFetches.has(sig.file)) return;
          failedFetches.add(sig.file);
        }
        emitProgress({
          scope: 'compile',
          kind: sig.outcome === 'failed' ? 'warn' : 'info',
          actor,
          message: (sig.outcome === 'failed' ? 'could not download ' : 'downloading ') + sig.file,
          event: { action: 'compile.fetch', file: sig.file, outcome: sig.outcome },
        });
      } else
        emit({
          scope: 'compile',
          kind: 'progress',
          actor,
          message: line.text.slice(0, 300),
          event: { action: 'compile.engine-line', stream: line.stream },
        });
    });
  } catch {
    return () => {}; // compile proceeds without live lines
  }
}

/** Every 5s, say the compile is still running. Returns the stop function. */
export function startHeartbeat(actor: Actor, target: string, t0: number): () => void {
  const timer = setInterval(
    () =>
      emit({
        scope: 'compile',
        kind: 'progress',
        actor,
        message: `still compiling ${target} (${Math.floor((Date.now() - t0) / 1000)}s)`,
        event: { action: 'compile.progress', target, elapsedMs: Date.now() - t0 },
      }),
    5000,
  );
  return () => clearInterval(timer);
}

/** Track the longest gap between animation frames. `stop` returns it in ms. */
export function startFrameProbe(): () => number {
  let maxGap = 0;
  let lastT = performance.now();
  let probing = true;
  const tick = () => {
    if (!probing) return;
    const now = performance.now();
    maxGap = Math.max(maxGap, now - lastT);
    lastT = now;
    requestAnimationFrame(tick);
  };
  requestAnimationFrame(tick);
  return () => {
    probing = false;
    return Math.round(maxGap);
  };
}

/** Detach a listener, tolerating one that is already gone. */
export function detach(unlisten: () => void) {
  try {
    unlisten();
  } catch {
    /* already detached */
  }
}
