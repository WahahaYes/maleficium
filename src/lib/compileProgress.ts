// compileProgress.ts — where a running compile is, folded from its typed
// bus events (`compile.start`, `compile.phase`, `compile.fetch`) only.

import type { AppEvent } from './generated/events';
import type { CompilePhase as EnginePhase } from './generated/structure';

export type CompileProgress = {
  phase: EnginePhase | null;
  detail: string | null;
  /** TeX passes so far. */
  pass: number;
  /** Files fetched so far, and the latest one. */
  downloads: number;
  current: string | null;
  /** Nothing was cached: this compile downloads the TeX support files. */
  firstCompile: boolean;
};

export const IDLE_PROGRESS: CompileProgress = {
  phase: null,
  detail: null,
  pass: 0,
  downloads: 0,
  current: null,
  firstCompile: false,
};

export function foldProgress(p: CompileProgress, e: AppEvent): CompileProgress {
  switch (e.action) {
    case 'compile.start':
      return IDLE_PROGRESS;
    case 'compile.phase':
      if (e.phase === 'first-compile') return { ...p, firstCompile: true };
      return {
        ...p,
        phase: e.phase,
        detail: e.detail ?? null,
        pass: e.phase === 'tex' ? p.pass + 1 : p.pass,
      };
    case 'compile.fetch':
      return e.outcome === 'fetched' ? { ...p, downloads: p.downloads + 1, current: e.file } : p;
    default:
      return p;
  }
}

function phaseText(p: CompileProgress): string | null {
  switch (p.phase) {
    case null:
    case 'first-compile':
      return null;
    case 'format':
      return 'building the LaTeX format';
    case 'tex':
      return `TeX pass ${p.pass}` + (p.detail ? ` (${p.detail})` : '');
    case 'bibliography':
      return `bibliography (${p.detail ?? 'bibtex'})`;
    case 'xdvipdfmx':
      return 'converting to PDF';
    case 'writing':
      return `writing ${p.detail ?? 'output'}`;
  }
}

/** One status-bar line for a running compile, or null before any signal. */
export function progressLabel(p: CompileProgress): string | null {
  const parts: string[] = [];
  if (p.firstCompile) parts.push('first compile downloads TeX support files');
  const phase = phaseText(p);
  if (phase) parts.push(phase);
  if (p.downloads > 0) {
    parts.push(
      `downloaded ${p.downloads} file${p.downloads === 1 ? '' : 's'}` +
        (p.current ? ` (${p.current})` : ''),
    );
  }
  return parts.length ? parts.join(' · ') : null;
}
