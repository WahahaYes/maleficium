import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type {
  CompileFailure,
  CompileLine,
  CompileReport,
  EventKind,
  OfflineReadiness,
} from './generated/events';
import type { Finding, MissingDependency } from './generated/structure';
import type { OutputStamp } from './externalRefresh';

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
      return 'the TeX bundle changed upstream: this PDF may not match your sources — compile online, then make available offline again';
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

/** The needs marker the badge shows when the bundle digest drifted. */
const BUNDLE_CHANGED_NEEDS = 'TeX bundle changed';

/** Whether a readiness record shows a bundle digest drift. */
export function bundleDrifted(r: OfflineReadiness): boolean {
  return r.missing?.reason === 'bundle-changed' || r.needs.includes(BUNDLE_CHANGED_NEEDS);
}

/** The log line for a compile's missing dependency, or null when it lacked none. */
export function missingLine(
  m: MissingDependency | null,
  ok: boolean,
): { kind: EventKind; message: string } | null {
  if (m === null) return null;
  return {
    kind: m.reason === 'bundle-changed' || !ok ? 'error' : 'warn',
    message: describeMissing(m),
  };
}

/** The log line for a readiness refresh, reusing the badge label. */
export function readinessLine(r: OfflineReadiness): { kind: EventKind; message: string } {
  const badge = offlineBadge(r);
  return {
    kind: r.state === 'ready' ? 'success' : bundleDrifted(r) ? 'error' : 'info',
    message: 'offline: ' + badge.label,
  };
}

/**
 * The compile status line: the main file a run compiled, or its failure. A
 * drift warning leads the line. The pdf path is app-internal, so it stays out
 * of the line (see `compileLogTitle`); the event log still records it.
 */
export function compileLogText(r: CompileResult, mainRel?: string | null): string {
  const body = r.ok ? (mainRel ? 'compiled ' + mainRel : 'compiled') : r.log;
  return r.missing ? `${describeMissing(r.missing)}\n${body}` : body;
}

/** The latest successful run: its status line and the pdf it wrote. */
export type CompiledOutput = { text: string; pdfUrl: string };

/** The tooltip for a status line: the pdf path while it shows that run's line. */
export function compileLogTitle(log: string, out: CompiledOutput | null): string | undefined {
  return out && log === out.text ? out.pdfUrl : undefined;
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

/** Stamp of the main file's pdf, or null before any compile. */
export async function outputStamp(rootId: string, mainRel: string): Promise<OutputStamp | null> {
  return await invoke<OutputStamp | null>('output_stamp', { rootId, mainRel });
}

/** What an export wrote. */
export type Exported = { path: string; bytes: number; files: string[] };

/** Copy the compiled pdf to `dest` (absolute, outside the project). */
export async function exportPdf(rootId: string, mainRel: string, dest: string): Promise<Exported> {
  return await invoke<Exported>('export_pdf', { rootId, mainRel, dest });
}

/** Zip the project's sources to `dest` (absolute, outside the project). */
export async function exportZip(rootId: string, dest: string): Promise<Exported> {
  return await invoke<Exported>('export_zip', { rootId, dest });
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
      if (bundleDrifted(r)) {
        return {
          label: 'TeX bundle changed: PDF may not match sources',
          title: why || 'The TeX bundle changed upstream: this PDF may not match your sources',
          tone: 'error',
        };
      }
      return {
        label: needs ? `Offline unverified: ${needs}` : 'Offline unverified',
        title: 'Compile, or use Tools → Make Available Offline, to check this project offline',
        tone: 'neutral',
      };
  }
}

/**
 * Dependency findings over the saved document, before compiling: every
 * package the bundle and project lack (all at once), biber, shell escape,
 * missing fonts. Rejects when the checks cannot run.
 */
export async function precompileChecks(rootId: string, mainRel: string): Promise<Finding[]> {
  const r = await invoke<{ findings: Finding[] }>('precompile_checks', { rootId, mainRel });
  return r.findings;
}

/** One sentence for a pre-compile finding, with where the document asks. */
export function describeFinding(f: Finding): string {
  const at = `${f.path}:${f.line}`;
  switch (f.kind) {
    case 'not-in-bundle':
      return `${f.name} is in neither the TeX bundle nor the project (${at})`;
    case 'external-tool':
      return (
        `${at} needs ${f.name}, a program outside the TeX bundle` +
        (f.suggestion ? `: ${f.suggestion}` : '')
      );
    case 'shell-escape':
      return `${f.name} needs shell escape, which compiles never enable (${at})`;
    case 'system-font':
      return `font "${f.name}" is not installed on this machine (${at})`;
  }
}
