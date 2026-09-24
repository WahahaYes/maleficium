import { describe, expect, it } from 'vitest';
import type { AppEvent } from './generated/events';
import { foldProgress, IDLE_PROGRESS, progressLabel } from './compileProgress';

const fold = (events: AppEvent[]) => events.reduce(foldProgress, IDLE_PROGRESS);

describe('compile progress', () => {
  it('reads a cold compile as first-compile, phase and live download count', () => {
    const p = fold([
      { action: 'compile.start', target: 'main.tex' },
      { action: 'compile.phase', phase: 'first-compile' },
      { action: 'compile.fetch', file: 'tectonic-format-latex.tex', outcome: 'fetched' },
      { action: 'compile.phase', phase: 'format' },
      { action: 'compile.fetch', file: 'latex.ltx', outcome: 'fetched' },
    ]);
    expect(progressLabel(p)).toBe('first compile · format · 2 downloaded (latex.ltx)');
  });

  it('counts TeX passes with their rerun reasons', () => {
    const p = fold([
      { action: 'compile.phase', phase: 'tex' },
      { action: 'compile.phase', phase: 'bibliography', detail: 'bibtex' },
      { action: 'compile.phase', phase: 'tex', detail: 'bibtex was run' },
    ]);
    expect(progressLabel(p)).toBe('TeX pass 2 (bibtex was run)');
    expect(
      progressLabel(
        foldProgress(p, { action: 'compile.phase', phase: 'writing', detail: 'main.pdf' }),
      ),
    ).toBe('writing main.pdf');
  });

  it('ignores failed fetches in the count and resets on a new start', () => {
    const p = fold([
      { action: 'compile.fetch', file: 'x.sty', outcome: 'failed' },
      { action: 'compile.engine-line', stream: 'stdout' },
    ]);
    expect(progressLabel(p)).toBeNull();
    const q = fold([
      { action: 'compile.fetch', file: 'a.sty', outcome: 'fetched' },
      { action: 'compile.start', target: 'main.tex' },
    ]);
    expect(q).toEqual(IDLE_PROGRESS);
  });

  it('shows the online attempt while the engine is tried', () => {
    const p = fold([
      { action: 'compile.start', target: 'main.tex' },
      { action: 'compile.phase', phase: 'connect' },
    ]);
    expect(progressLabel(p)).toBe('connecting');
  });

  it('replaces the attempt with the run’s own phases', () => {
    const p = fold([
      { action: 'compile.phase', phase: 'connect' },
      { action: 'compile.phase', phase: 'tex' },
    ]);
    expect(progressLabel(p)).toBe('TeX pass 1');
  });
});
