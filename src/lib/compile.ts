import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type {
  CompileFailure,
  CompileLine,
  CompileReport,
  OfflineReadiness,
} from './generated/events';
import type { MissingDependency } from './generated/structure';

/**
 * Compile backend contract, addressed by session root and the main file's
 * root-relative path. The backend owns where outputs live: callers never
 * name or derive the outdir.
 *
 * `ok:true` → `pdfUrl` locates the pdf (desktop: an absolute path; hosted:
 * an opaque URL), `log` is empty. `ok:false` → `pdfUrl` is null, `log` is
 * the error message, `failure` says why. `missing` names the dependency the
 * run lacked (beside a pdf only when the pinned bundle changed). Engine
 * lines stream via `compile-line` during the run.
 */
export type CompileResult = {
  ok: boolean;
  pdfUrl: string | null;
  log: string;
  failure: CompileFailure | null;
  missing: MissingDependency | null;
};

export function onCompileLine(cb: (line: CompileLine) => void): Promise<() => void> {
  return listen<CompileLine>('compile-line', (e) => cb(e.payload));
}

/**
 * `networked` fetches everything online first, then proves the document
 * compiles from the cache alone (Make Available Offline).
 */
export async function compileTex(
  rootId: string,
  mainRel: string,
  networked = false,
): Promise<CompileResult> {
  try {
    const r = await invoke<CompileReport>('compile_tex', { rootId, mainRel, networked });
    return {
      ok: r.pdfUrl != null,
      pdfUrl: r.pdfUrl,
      log: r.message,
      failure: r.failure,
      missing: r.missing,
    };
  } catch (e) {
    // Refused before the engine ran (target outside the root) or cancelled.
    return { ok: false, pdfUrl: null, log: String(e), failure: 'engine-error', missing: null };
  }
}

/** One sentence naming what a compile lacked and what would fix it. */
export function describeMissing(m: MissingDependency): string {
  const f = m.file ?? 'a file';
  switch (m.reason) {
    case 'not-cached':
      return `${f} is not cached yet and there is no network to fetch it`;
    case 'fetch-failed':
      return `could not download ${f}: check the network connection`;
    case 'not-in-bundle':
      return `${f} is not in the TeX bundle: fetching cannot help`;
    case 'bundle-unreachable':
      return 'the TeX bundle is not cached and cannot be reached';
    case 'bundle-invalid':
      return 'the TeX bundle location is not a bundle';
    case 'cache-empty':
      return 'the first compile needs network: no TeX support files are cached yet';
    case 'bundle-changed':
      return 'the pinned TeX bundle changed upstream: offline readiness is no longer trusted';
    case 'system-font':
      return `font "${f}" is not installed on this machine`;
    case 'external-tool':
      return f === 'biber'
        ? 'biber is not installed: use \\usepackage[backend=bibtex]{biblatex} to compile without it'
        : `${f} is not installed on this machine`;
    case 'shell-escape-required':
      return `${f} needs shell escape, which compiles never enable`;
  }
}

export async function cancelCompile(): Promise<string> {
  return await invoke<string>('cancel_compile');
}

/** The full engine log of the last compile, or null when there is none. */
export async function engineLog(rootId: string, mainRel: string): Promise<string | null> {
  try {
    return await invoke<string>('engine_log', { rootId, mainRel });
  } catch {
    // No log kept for this main file yet; callers fall back to the run's own output.
    return null;
  }
}

/** Whether a previous compile left a pdf. Never throws. */
export async function outputsFresh(rootId: string, mainRel: string): Promise<boolean> {
  try {
    return await invoke<boolean>('outputs_fresh', { rootId, mainRel });
  } catch {
    // Main file no longer resolvable: nothing to warm, the next Compile reports why.
    return false;
  }
}

export type CleanResult = { ok: boolean; removed: number; error: string | null };

/** Remove the main file's build artifacts. Sources are never touched. */
export async function cleanOutputs(rootId: string, mainRel: string): Promise<CleanResult> {
  try {
    const removed = await invoke<number>('clean_outputs', { rootId, mainRel });
    return { ok: true, removed, error: null };
  } catch (e) {
    return { ok: false, removed: 0, error: String(e) };
  }
}

/** The project's offline readiness; rejects when the root is not granted. */
export async function offlineReadiness(rootId: string): Promise<OfflineReadiness> {
  return await invoke<OfflineReadiness>('offline_readiness', { rootId });
}

export type OfflineBadge = {
  label: string;
  title: string;
  tone: 'success' | 'warning' | 'error' | 'neutral';
};

/** The status-bar badge for a readiness state. */
export function offlineBadge(r: OfflineReadiness): OfflineBadge {
  const needs = r.needs.join(', ');
  const why = r.missing ? describeMissing(r.missing) : '';
  switch (r.state) {
    case 'ready':
      return {
        label: 'Ready offline',
        title: 'The last compile used only cached TeX files: this project compiles without network',
        tone: 'success',
      };
    case 'needs-network':
      return {
        label: `Needs network for: ${needs}`,
        title: why || 'Tools → Make Available Offline fetches what this project needs',
        tone: 'warning',
      };
    case 'needs-tool':
      return {
        label: `Needs ${needs}`,
        title: why || `${needs} is not installed`,
        tone: 'warning',
      };
    case 'needs-font':
      return {
        label: `Needs font: ${needs}`,
        title: why || `${needs} is not installed`,
        tone: 'warning',
      };
    case 'blocked':
      return { label: `Can't compile: ${needs}`, title: why, tone: 'error' };
    case 'unverified':
      return {
        label: needs ? `Offline unverified: ${needs}` : 'Offline unverified',
        title: 'Compile, or use Tools → Make Available Offline, to check this project offline',
        tone: 'neutral',
      };
  }
}
