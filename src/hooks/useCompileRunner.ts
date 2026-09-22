// useCompileRunner.ts — the compile flow and its status.
//
// One run at a time: `phaseRef` gates reentrancy and leads `compilePhase` by
// a tick, so a second trigger is rejected before React re-renders. Dirty
// buffers are persisted before the engine reads the tree, and a run that
// does not own its target aborts rather than clobbering another buffer.

import { useEffect, useRef, useState } from 'react';
import type { RefObject } from 'react';
import { appCacheDir } from '@tauri-apps/api/path';
import { markSaved, type BufferState } from '../lib/buffers';
import { cancelCompile, compileTex, onCompileLine } from '../lib/compile';
import { emit, type ProblemEvent } from '../lib/events';
import { saveTex } from '../lib/files';
import { fs } from '../lib/fs-provider';
import { parseLog } from '../lib/parseLog';
import { grantUntitledAccess } from '../lib/projectAccess';
import { appOutDir } from '../lib/paths';
import { emitPdf } from '../lib/preview-bus';

/** The compile lifecycle. `phaseRef` leads this state by a tick. */
export type CompilePhase = 'idle' | 'compiling' | 'success' | 'failure';

export interface UseCompileRunnerDeps {
  tex: string;
  fileName: string;
  mainFile: string | null;
  setMainFileState: (v: string | null) => void;
  mainDir: string;
  workdirHint: string;
  root: string | null;
  projectId: string | null;
  relInProject: (abs: string) => string | null;
  buffers: Map<string, BufferState>;
  setBuffers: React.Dispatch<React.SetStateAction<Map<string, BufferState>>>;
  largeFile: string | null;
  previewFile: string | null;
  markOwnWrite: (p: string) => void;
  setLog: (v: string) => void;
  setLogCollapsed: (v: boolean) => void;
  /** Latest-closure handle for the keymap listener and the menu dispatcher. */
  compileRef: RefObject<() => Promise<void>>;
}

export function useCompileRunner(deps: UseCompileRunnerDeps) {
  const {
    tex,
    fileName,
    mainFile,
    setMainFileState,
    mainDir,
    workdirHint,
    root,
    projectId,
    relInProject,
    buffers,
    setBuffers,
    largeFile,
    previewFile,
    markOwnWrite,
    setLog,
    setLogCollapsed,
    compileRef,
  } = deps;

  const [compilePhase, setCompilePhase] = useState<CompilePhase>('idle');
  const [compileTimer, setCompileTimer] = useState(0);
  const [compileStart, setCompileStart] = useState<number | null>(null);

  useEffect(() => {
    const h = () => {
      cancelCompile().catch(() => {
        /* exiting — nothing to report */
      });
    };
    window.addEventListener('beforeunload', h);
    return () => window.removeEventListener('beforeunload', h);
  }, []);
  const publishProblems = (text: string, base: string, wsRoot: string) => {
    try {
      const entries = parseLog(text, wsRoot, base).slice(0, 100);
      for (const l of entries) {
        emit({
          scope: 'compile',
          kind: l.clickable ? 'error' : 'warn',
          message: `${l.file}:${l.line} ${l.msg}`,
          data: {
            action: 'compile.problem',
            file: l.file,
            line: l.line,
            msg: l.msg,
            clickable: l.clickable,
          } satisfies ProblemEvent & { action: string },
        });
      }
    } catch {
      /* parse never blocks the stream */
    }
  };

  async function compile() {
    await runCompile(mainFile ?? (fileName.includes('/') ? fileName : null));
  }

  // Shared compile runner: `target` is the main-file target, or an explicit
  // one-off file. `skipPersist` is the warm-open path only: the target was
  // just loaded from disk, so there is nothing to persist — warm never
  // writes. All other callers persist through the ownership check.
  // Reentrancy: a second call while `compiling` is a no-op returning false
  // (warm-on-open and double-Ctrl+R collapse into one run — never two
  // engine children). Returns true when this call owned the run. The gate
  // reads a ref (not state) so a warm run racing a user Ctrl+R in the same
  // tick still collapses.
  const phaseRef = useRef<CompilePhase>('idle');
  async function runCompile(
    target: string | null,
    opts?: { skipPersist?: boolean },
  ): Promise<boolean> {
    if (phaseRef.current === 'compiling') return false;
    phaseRef.current = 'compiling';
    const finish = (phase: CompilePhase) => {
      phaseRef.current = phase;
      setCompilePhase(phase);
    };
    if (largeFile || previewFile) {
      emit({
        scope: 'compile',
        kind: 'error',
        message: 'compile blocked: open a .tex file first',
        data: {
          action: 'compile.blocked',
          reason: largeFile ? 'large-placeholder' : 'non-text-selection',
        },
      });
      finish('failure');
      setLog('compile blocked: open a .tex file first');
      return false;
    }
    emit({
      scope: 'compile',
      kind: 'progress',
      message: 'compiling ' + (target ?? fileName),
      data: { action: 'compile.start', target: target ?? fileName },
    });
    finish('compiling');
    setCompileStart(Date.now());
    setCompileTimer(0);
    setLog('compiling...');
    // Write-then-compile: the engine reads from disk, so persist first.
    let workdir: string;
    let unlisten: () => void = () => {};
    // `note: downloading <pkg>` lines get their own download-wait signal
    // so a long first build reads as network-wait, not an engine hang.
    const isDownloadLine = (l: string) => /(^|\s)downloading\s/i.test(l);
    try {
      unlisten = await onCompileLine((line) => {
        const s = String(line);
        if (isDownloadLine(s)) {
          const pkg = s.replace(/^.*downloading\s+/i, '').slice(0, 120);
          emit({
            scope: 'compile',
            kind: 'info',
            message: 'downloading ' + pkg,
            data: { action: 'compile.download', package: pkg },
          });
        } else
          emit({
            scope: 'compile',
            kind: 'progress',
            message: s.slice(0, 300),
            data: { action: 'compile.engine-line' },
          });
      });
    } catch {
      /* listener attach best-effort — compile proceeds without live lines */
    }
    const t0 = Date.now();
    const hb = setInterval(
      () =>
        emit({
          scope: 'compile',
          kind: 'progress',
          message: `still compiling ${target ?? fileName} (${Math.floor((Date.now() - t0) / 1000)}s)`,
          data: {
            action: 'compile.progress',
            target: target ?? fileName,
            elapsedMs: Date.now() - t0,
          },
        }),
      5000,
    );
    let maxGap = 0;
    let lastT = performance.now();
    let probing = true;
    const tickProbe = () => {
      if (!probing) return;
      const now = performance.now();
      maxGap = Math.max(maxGap, now - lastT);
      lastT = now;
      requestAnimationFrame(tickProbe);
    };
    requestAnimationFrame(tickProbe);
    // Anti-clobber check: the persist step writes editor content to `target`.
    // Two invariants make that safe: (1) the visible editor must actually
    // own `target` (buffered, or the untitled flow); (2) `target` must be a
    // contained project file or an explicit one-off .tex — never a bare
    // untitled name resolved against a project dir. Violations abort before
    // any write. Warm skips both check and persist (disk is fresh, warm
    // never writes).
    const skipPersist = opts?.skipPersist === true;
    const ownsTarget =
      target == null ||
      buffers.has(target) ||
      target === fileName ||
      (!target.includes('/') && !fileName.includes('/'));
    if (!skipPersist && target != null && target.includes('/') && !ownsTarget) {
      finish('failure');
      setCompileStart(null);
      probing = false;
      emit({
        scope: 'compile',
        kind: 'error',
        message: `compile refused: editor does not own ${target} (open it first)`,
        data: { action: 'compile.refused', reason: 'editor-does-not-own-target', target },
      });
      setLog(`compile refused: editor does not own ${target}`);
      clearInterval(hb);
      try {
        unlisten();
      } catch {
        /* already detached */
      }
      return false;
    }
    try {
      if (target && target.includes('/')) {
        if (!skipPersist) {
          // Persist ALL dirty buffers so \input parts compile from disk.
          for (const [p, buf] of buffers) {
            if (buf.dirty) {
              try {
                await saveTex(p, buf.value);
                markOwnWrite(p);
              } catch {
                /* keep dirty, reported at finish */
              }
            }
          }
          setBuffers((b) => {
            let n = b;
            for (const [p, buf] of b) if (buf.dirty) n = markSaved(n, p);
            return n;
          });
          // Also persist the visible editor if it was never buffered (untitled flow).
          if (!buffers.has(target)) {
            await saveTex(target, tex);
            markOwnWrite(target);
          }
        }
        workdir = target.slice(0, target.lastIndexOf('/')) || '/';
      } else {
        const scratch = await grantUntitledAccess();
        if (!scratch.ok || !scratch.path) throw new Error(scratch.error ?? 'scratch unavailable');
        workdir = scratch.path;
        const t2 = workdir + '/' + fileName;
        await saveTex(t2, tex);
        markOwnWrite(t2);
        setMainFileState(t2);
      }
    } catch (e) {
      finish('failure');
      setCompileStart(null);
      probing = false;
      emit({
        scope: 'compile',
        kind: 'error',
        message: 'save failed: ' + String(e).slice(0, 200),
        data: { action: 'compile.persist-failed', target, error: String(e).slice(0, 200) },
      });
      clearInterval(hb);
      try {
        unlisten();
      } catch {
        /* already detached */
      }
      return false;
    }
    const activeTarget = target ?? (fileName.includes('/') ? fileName : workdir! + '/' + fileName);
    const main = activeTarget.slice(activeTarget.lastIndexOf('/') + 1);
    const r = await compileTex(activeTarget, workdir!);
    // `compile_tex` returns the pdf path on success, empty error string;
    // the status line shows the pdf path or the failure message.
    setLog(r.ok ? (r.pdfPath ?? '') : r.log);
    const readEngineLog = async (): Promise<string | null> => {
      try {
        // App-local outdir over the app-cache dir: the engine log lives in
        // cache, never in the project.
        const out = appOutDir(await appCacheDir(), workdir!);
        return await fs().readText(`${out}/${main.replace(/\.tex$/, '.log')}`);
      } catch {
        return null;
      }
    };
    if (r.ok && r.pdfPath) {
      finish('success');
      setCompileStart(null);
      setLogCollapsed(false);
      emit({
        scope: 'compile',
        kind: 'success',
        message: 'compiled ' + String(r.pdfPath),
        data: {
          action: 'compile.finish',
          ok: true,
          target: activeTarget,
          pdfPath: String(r.pdfPath),
          ms: Date.now() - t0,
        },
      });
      const docRel = target ? relInProject(target) : null;
      emitPdf({
        url: r.pdfPath,
        docKey: projectId && docRel ? `${projectId}:${docRel}` : null,
        revision: null,
      });
      emit({
        scope: 'preview',
        kind: 'success',
        message: 'preview ' + String(r.pdfPath),
        data: { action: 'preview.update', pdfPath: String(r.pdfPath) },
      });
    } else if (!r.ok && r.log.includes('spawn')) {
      finish('failure');
      setCompileStart(null);
      setLogCollapsed(false);
      setLog(r.log + ' (engine sidecar failed to start)');
      emit({
        scope: 'compile',
        kind: 'error',
        message: String(r.log).slice(0, 300),
        data: {
          action: 'compile.finish',
          ok: false,
          target: activeTarget,
          reason: 'spawn-failed',
          ms: Date.now() - t0,
        },
      });
      const c = await readEngineLog();
      publishProblems(c ?? r.log, mainDir, root || workdirHint);
    } else if (!r.ok) {
      finish('failure');
      setCompileStart(null);
      setLogCollapsed(false);
      emit({
        scope: 'compile',
        kind: 'error',
        message: String(r.log).slice(0, 300),
        data: {
          action: 'compile.finish',
          ok: false,
          target: activeTarget,
          reason: 'engine-error',
          ms: Date.now() - t0,
        },
      });
      const c = await readEngineLog();
      publishProblems(c ?? r.log, mainDir, root || workdirHint);
    }
    clearInterval(hb);
    try {
      unlisten();
    } catch {
      /* already detached */
    }
    probing = false;
    emit({
      scope: 'compile',
      kind: 'info',
      message: `main-thread max frame ${Math.round(maxGap)}ms during compile`,
      data: { action: 'compile.frame-probe', maxFrameMs: Math.round(maxGap) },
    });
    return true;
  }

  // Cache-warm on open: a background compile of a freshly opened project's
  // main file, after the editor is populated. The write phase is skipped
  // (skipPersist: disk is fresh, warm never writes). Two rules: (1) this
  // runs only when the engine cache is usable — no cache entry means a full
  // cold build, which is the user's explicit Ctrl+R to pay for, not open's;
  // (2) a warm failure is quiet (debug line only) — open must never look
  // broken because a background guess failed; the user's explicit Ctrl+R
  // reports loudly through the normal path.
  async function warmCompile(mainAbsPath: string) {
    const usable = await engineCacheUsable(mainAbsPath).catch(() => false);
    if (!usable) {
      emit({
        scope: 'compile',
        kind: 'info',
        message: 'preview will build on first Compile (no cached output)',
        data: { action: 'compile.warm-skipped', reason: 'no-cached-output', target: mainAbsPath },
      });
      return;
    }
    emit({
      scope: 'compile',
      kind: 'info',
      message: 'warming preview for ' + mainAbsPath,
      data: { action: 'compile.warm', target: mainAbsPath },
    });
    await runCompile(mainAbsPath, { skipPersist: true });
    // Not owned (user raced us) → their stream wins; nothing to report.
  }

  // True when the app-local outdir already holds this target's engine output
  // (pdf from a previous successful run): the warm compile then only
  // verifies freshness instead of paying a full cold build on every open.
  // Best-effort stat only — never throws. The log is not required here:
  // the engine does not reliably leave one beside every pdf.
  async function engineCacheUsable(targetAbsPath: string): Promise<boolean> {
    try {
      const dir = targetAbsPath.slice(0, targetAbsPath.lastIndexOf('/')) || '/tmp';
      const stem = targetAbsPath.slice(targetAbsPath.lastIndexOf('/') + 1).replace(/\.tex$/, '');
      const out = appOutDir(await appCacheDir(), dir);
      return (await fs().stat(`${out}/${stem}.pdf`)) !== null;
    } catch {
      return false;
    }
  }
  async function handleCompileFile(path: string) {
    if (!path.endsWith('.tex')) {
      emit({
        scope: 'compile',
        kind: 'error',
        message: 'compile blocked: open a .tex file first',
        data: { action: 'compile.blocked', reason: 'not-a-tex-file', target: path },
      });
      return;
    }
    emit({
      scope: 'compile',
      kind: 'info',
      message: `compiling ${path} directly (one-off, not the main file)`,
      data: { action: 'compile.one-off', target: path },
    });
    await runCompile(path);
  }
  // Mirror compile bus → status bar phase/timer.
  useEffect(() => {
    const t = setInterval(() => {
      if (compileStart != null) setCompileTimer(Math.floor((Date.now() - compileStart) / 1000));
    }, 500);
    return () => clearInterval(t);
  }, [compileStart]);

  compileRef.current = compile;

  return {
    compilePhase,
    compileTimer,
    warmCompile,
    handleCompileFile,
  };
}
