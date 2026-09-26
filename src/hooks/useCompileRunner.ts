// useCompileRunner.ts — the compile flow and its status.
//
// One run at a time: `phaseRef` gates reentrancy and leads `compilePhase` by
// a tick, so a second trigger is rejected before React re-renders. Dirty
// buffers are persisted before the engine reads the tree, and a run that
// does not own its target aborts rather than clobbering another buffer.

import { useEffect, useRef, useState } from 'react';
import type { RefObject } from 'react';
import { markSaved, type BufferState } from '../lib/buffers';
import {
  cancelCompile,
  compileLogText,
  type CompiledOutput,
  compileTex,
  describeFinding,
  engineLog,
  missingLine,
  precompileChecks,
  offlineBadge,
  offlineReadiness,
  readinessLine,
  type OfflineBadge,
  onCompileLine,
  outputsFresh,
  type CompileResult,
} from '../lib/compile';
import { emit } from '../lib/events';
import { foldProgress, IDLE_PROGRESS, progressLabel } from '../lib/compileProgress';
import { INITIAL_AUTO, parseAutoCompile, stepAuto, type AutoInput } from '../lib/autoCompile';
import { DEVICE_PREF_KEYS, store } from '../lib/app-store';
import { transport } from '../lib/event-transport';
import type { Actor, PrecheckPanelVia } from '../lib/generated/events';
import type { Finding } from '../lib/generated/structure';
import { findingsKey, loadPrecheckPopup, savePrecheckPopup, shouldPop } from '../lib/precheckPanel';
import { saveTex } from '../lib/files';
import { structure } from '../lib/structure';
import { emitPdf, sourceFor, type SessionRoot } from '../lib/preview-bus';
import type { OwnWrites } from '../lib/own-writes';

/** The compile lifecycle. `phaseRef` leads this state by a tick. */
export type CompilePhase = 'idle' | 'compiling' | 'success' | 'failure';

/** The latest run's pre-compile findings; `path`s are relative to `rootPath`. */
export interface PrecheckFindings {
  target: string;
  rootPath: string;
  findings: Finding[];
}

export interface UseCompileRunnerDeps {
  tex: string;
  fileName: string;
  mainFile: string | null;
  setMainFileState: (v: string | null) => void;
  mainDir: string;
  workdirHint: string;
  root: string | null;
  projectId: string | null;
  /** The untitled scratch root; null until its grant resolves. */
  scratch: SessionRoot | null;
  buffers: Map<string, BufferState>;
  setBuffers: React.Dispatch<React.SetStateAction<Map<string, BufferState>>>;
  largeFile: string | null;
  previewFile: string | null;
  ownWrites: OwnWrites;
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
    scratch,
    buffers,
    setBuffers,
    largeFile,
    previewFile,
    ownWrites,
    setLog,
    setLogCollapsed,
    compileRef,
  } = deps;

  const [compilePhase, setCompilePhase] = useState<CompilePhase>('idle');
  const [compileTimer, setCompileTimer] = useState(0);
  const [compileStart, setCompileStart] = useState<number | null>(null);
  const [offline, setOffline] = useState<OfflineBadge | null>(null);
  /** The latest successful run's line and pdf, for the status tooltip. */
  const [compiled, setCompiled] = useState<CompiledOutput | null>(null);
  const [progress, setProgress] = useState(IDLE_PROGRESS);
  const [precheck, setPrecheck] = useState<PrecheckFindings | null>(null);
  const [precheckOpen, setPrecheckOpen] = useState(false);
  const [precheckPopup, setPrecheckPopupState] = useState(loadPrecheckPopup);
  /** Finding-set key last popped per target. */
  const precheckShownRef = useRef(new Map<string, string>());
  /** Emit a compile event that also moves the live progress line. */
  const emitProgress = (e: Parameters<typeof emit>[0]) => {
    emit(e);
    setProgress((p) => foldProgress(p, e.event));
  };

  /** Read the project's offline readiness onto the bus and the badge. */
  const refreshReadiness = async (p: SessionRoot) => {
    try {
      const r = await offlineReadiness(p.rootId);
      const badge = offlineBadge(r);
      setOffline(badge);
      const line = readinessLine(r);
      emit({
        scope: 'compile',
        kind: line.kind,
        actor: 'system',
        message: line.message,
        event: { action: 'offline.readiness', root: p.path, state: r.state, needs: r.needs },
      });
    } catch (e) {
      setOffline(null);
      emit({
        scope: 'compile',
        kind: 'warn',
        actor: 'system',
        message: 'offline readiness unavailable: ' + String(e).slice(0, 200),
        event: { action: 'offline.readiness-failed', root: p.path, error: String(e).slice(0, 200) },
      });
    }
  };
  useEffect(() => {
    if (root && projectId) void refreshReadiness({ rootId: projectId, path: root });
    else setOffline(null);
  }, [root, projectId]);

  useEffect(() => {
    const h = () => {
      cancelCompile().catch(() => {
        /* exiting — nothing to report */
      });
    };
    window.addEventListener('beforeunload', h);
    return () => window.removeEventListener('beforeunload', h);
  }, []);
  /** Engine-log diagnostics onto the bus, root-relative to the run's root. */
  const publishProblems = async (
    text: string,
    base: string,
    src: { rootId: string; rootPath: string } | null,
    actor: Actor,
  ) => {
    try {
      const wsRoot = src?.rootPath ?? (root || workdirHint);
      const entries = (await structure().diagnostics(text, wsRoot, base)).slice(0, 100);
      for (const d of entries) {
        emit({
          scope: 'compile',
          kind: d.external ? 'warn' : 'error',
          actor,
          message: `${d.path ?? '(outside project)'}:${d.line} ${d.message}`,
          event: { action: 'compile.problem', rootId: src?.rootId ?? null, ...d },
        });
      }
    } catch {
      /* parse never blocks the stream */
    }
  };

  /** Granted roots a target may sit in: the project (or `project`) and scratch. */
  function sessionRoots(project?: SessionRoot): SessionRoot[] {
    const p = project ?? (root && projectId ? { rootId: projectId, path: root } : null);
    return [...(p ? [p] : []), ...(scratch ? [scratch] : [])];
  }

  async function compile(popup = true) {
    await runCompile(mainFile ?? (fileName.includes('/') ? fileName : null), { popup });
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
    opts?: { skipPersist?: boolean; root?: SessionRoot; networked?: boolean; popup?: boolean },
  ): Promise<boolean> {
    if (phaseRef.current === 'compiling') return false;
    phaseRef.current = 'compiling';
    // Warm-open runs on its own; every other run is someone's request.
    const actor: Actor = opts?.skipPersist ? 'system' : 'user';
    const finish = (phase: CompilePhase) => {
      phaseRef.current = phase;
      setCompilePhase(phase);
    };
    if (largeFile || previewFile) {
      emit({
        scope: 'compile',
        kind: 'error',
        actor,
        message: 'compile blocked: open a .tex file first',
        event: {
          action: 'compile.blocked',
          reason: largeFile ? 'large-placeholder' : 'non-text-selection',
        },
      });
      finish('failure');
      setLog('compile blocked: open a .tex file first');
      return false;
    }
    emitProgress({
      scope: 'compile',
      kind: 'progress',
      actor,
      message: 'compiling ' + (target ?? fileName),
      event: { action: 'compile.start', target: target ?? fileName },
    });
    finish('compiling');
    setCompileStart(Date.now());
    setCompileTimer(0);
    setLog('compiling...');
    // Write-then-compile: the engine reads from disk, so persist first.
    let workdir: string;
    let unlisten: () => void = () => {};
    // Fetch lines arrive typed: each becomes `compile.fetch`, so a long first
    // build reads as network-wait, not an engine hang. A failed fetch is
    // reported once per file (the engine retries and repeats itself).
    const failedFetches = new Set<string>();
    try {
      unlisten = await onCompileLine((line) => {
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
      /* listener attach best-effort — compile proceeds without live lines */
    }
    const t0 = Date.now();
    const hb = setInterval(
      () =>
        emit({
          scope: 'compile',
          kind: 'progress',
          actor,
          message: `still compiling ${target ?? fileName} (${Math.floor((Date.now() - t0) / 1000)}s)`,
          event: {
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
    // Persist before compile: every dirty buffer goes to its own file, so
    // \input parts compile from disk. A target with no buffer compiles
    // from disk as-is — the visible editor may show a different file
    // whose bytes must never land here. Only a relative visible file
    // (untitled flow) is written to the target; it lives nowhere else.
    // Warm skips persist entirely (disk is fresh, warm never writes).
    const skipPersist = opts?.skipPersist === true;
    try {
      if (target && target.includes('/')) {
        if (!skipPersist) {
          // Persist ALL dirty buffers so \input parts compile from disk.
          for (const [p, buf] of buffers) {
            if (buf.dirty) {
              try {
                await saveTex(p, buf.value);
                ownWrites.wrote(p, buf.value);
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
          // A visible file with no buffer is the untitled flow: its text
          // lives nowhere else, so it goes to the target. Any other
          // visible file was persisted above to its own path.
          if (!buffers.has(target) && !fileName.includes('/')) {
            await saveTex(target, tex);
            ownWrites.wrote(target, tex);
          }
        }
      } else {
        if (!scratch) throw new Error('scratch root unavailable');
        workdir = scratch.path;
        const t2 = workdir + '/' + fileName;
        await saveTex(t2, tex);
        ownWrites.wrote(t2, tex);
        setMainFileState(t2);
      }
    } catch (e) {
      finish('failure');
      setCompileStart(null);
      probing = false;
      emit({
        scope: 'compile',
        kind: 'error',
        actor,
        message: 'save failed: ' + String(e).slice(0, 200),
        event: { action: 'compile.persist-failed', target, error: String(e).slice(0, 200) },
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
    // The backend compiles by session root: a target outside every granted
    // root fails here, never reaching the engine.
    const src = sourceFor(activeTarget, sessionRoots(opts?.root));
    // Pre-compile checks list every dependency problem at once; the compile
    // itself stops at the first. They inform, never block.
    if (src) {
      try {
        const findings = await precompileChecks(src.rootId, src.mainRel);
        for (const f of findings) {
          emit({
            scope: 'compile',
            kind: 'warn',
            actor,
            message: describeFinding(f),
            event: { action: 'compile.precheck', target: activeTarget, ...f },
          });
        }
        showFindings(
          { target: activeTarget, rootPath: src.rootPath, findings },
          opts?.popup ?? !opts?.skipPersist,
          actor,
        );
      } catch (e) {
        setPrecheck(null);
        emit({
          scope: 'compile',
          kind: 'warn',
          actor,
          message: 'pre-compile checks unavailable: ' + String(e).slice(0, 200),
          event: {
            action: 'compile.precheck-failed',
            target: activeTarget,
            error: String(e).slice(0, 200),
          },
        });
      }
    }
    const r: CompileResult = src
      ? await compileTex(src.rootId, src.mainRel, opts?.networked === true)
      : {
          ok: false,
          pdfUrl: null,
          log: 'compile target is outside the project: ' + activeTarget,
          failure: 'engine-error',
          missing: null,
        };
    const logText = compileLogText(r, src?.mainRel);
    setLog(logText);
    setCompiled(r.ok && r.pdfUrl ? { text: logText, pdfUrl: String(r.pdfUrl) } : null);
    if (r.missing) {
      const m = r.missing;
      const line = missingLine(m, r.ok)!;
      emit({
        scope: 'compile',
        kind: line.kind,
        actor,
        message: line.message,
        event: {
          action: 'compile.missing',
          target: activeTarget,
          file: m.file ?? null,
          reason: m.reason,
        },
      });
    }
    const readEngineLog = () => (src ? engineLog(src.rootId, src.mainRel) : Promise.resolve(null));
    if (r.ok && r.pdfUrl) {
      finish('success');
      setCompileStart(null);
      setLogCollapsed(false);
      emit({
        scope: 'compile',
        kind: 'success',
        actor,
        message: 'compiled ' + String(r.pdfUrl),
        event: {
          action: 'compile.finish',
          ok: true,
          target: activeTarget,
          pdfUrl: String(r.pdfUrl),
          ms: Date.now() - t0,
        },
      });
      emitPdf({ url: r.pdfUrl, source: src, revision: null });
      emit({
        scope: 'preview',
        kind: 'success',
        actor,
        message: 'preview ' + String(r.pdfUrl),
        event: { action: 'preview.update', pdfUrl: String(r.pdfUrl) },
      });
    } else if (!r.ok && r.failure === 'spawn-failed') {
      finish('failure');
      setCompileStart(null);
      setLogCollapsed(false);
      setLog(r.log + ' (engine sidecar failed to start)');
      emit({
        scope: 'compile',
        kind: 'error',
        actor,
        message: String(r.log).slice(0, 300),
        event: {
          action: 'compile.finish',
          ok: false,
          target: activeTarget,
          reason: 'spawn-failed',
          ms: Date.now() - t0,
        },
      });
      const c = await readEngineLog();
      await publishProblems(c ?? r.log, mainDir, src, actor);
    } else if (!r.ok) {
      finish('failure');
      setCompileStart(null);
      setLogCollapsed(false);
      emit({
        scope: 'compile',
        kind: 'error',
        actor,
        message: String(r.log).slice(0, 300),
        event: {
          action: 'compile.finish',
          ok: false,
          target: activeTarget,
          reason: r.failure ?? 'engine-error',
          ms: Date.now() - t0,
        },
      });
      const c = await readEngineLog();
      await publishProblems(c ?? r.log, mainDir, src, actor);
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
      actor: 'system',
      message: `main-thread max frame ${Math.round(maxGap)}ms during compile`,
      event: { action: 'compile.frame-probe', maxFrameMs: Math.round(maxGap) },
    });
    if (src && src.rootId === (opts?.root?.rootId ?? projectId)) {
      await refreshReadiness({ rootId: src.rootId, path: src.rootPath });
    }
    return true;
  }

  // Auto-compile on save: the scheduler folds saves and every run's start
  // and finish off the bus; a timer wakes it when an armed run falls due.
  const [autoCompile, setAutoCompileState] = useState(() => {
    try {
      return parseAutoCompile(store().get(DEVICE_PREF_KEYS.autoCompile));
    } catch {
      // No app store configured: the default (on).
      return true;
    }
  });
  const autoRef = useRef({ ...INITIAL_AUTO, enabled: autoCompile });
  const autoFireRef = useRef<() => void>(() => {});
  const autoTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const feedAuto = useRef((i: AutoInput) => {
    const r = stepAuto(autoRef.current, i);
    autoRef.current = r.state;
    if (r.fire) autoFireRef.current();
    if (autoTimerRef.current) clearTimeout(autoTimerRef.current);
    autoTimerRef.current = null;
    const due = autoRef.current.dueAt;
    if (due != null) {
      autoTimerRef.current = setTimeout(
        () => feedAuto.current({ kind: 'tick', at: Date.now() }),
        Math.max(0, due - Date.now()),
      );
    }
  });
  // Path of the save that armed the pending run: a file closed since
  // (e.g. close-all persists, then evicts) must not fire it.
  const savePathRef = useRef<string | null>(null);
  autoFireRef.current = () => {
    // A main file from before a root switch must never auto-fire: the
    // backend would refuse it as outside the project.
    if (mainFile && mainFile.includes('/') && root && !mainFile.startsWith(root + '/')) return;
    const saved = savePathRef.current;
    if (saved && !buffers.has(saved)) return;
    const target = mainFile ?? fileName;
    emit({
      scope: 'compile',
      kind: 'info',
      actor: 'system',
      message: 'auto-compile after save: ' + target,
      event: { action: 'compile.auto', target },
    });
    void compile(false);
  };
  useEffect(
    () =>
      transport().subscribe((e) => {
        const a = e.event.action;
        if (a === 'file.save') {
          savePathRef.current = e.event.path;
          feedAuto.current({ kind: 'save', at: Date.now(), path: e.event.path });
        } else if (a === 'compile.start') feedAuto.current({ kind: 'start' });
        else if (a === 'compile.finish') feedAuto.current({ kind: 'finish', at: Date.now() });
      }),
    [],
  );
  useEffect(() => {
    feedAuto.current({ kind: 'reset' });
    setPrecheck(null);
    setPrecheckOpen(false);
  }, [projectId]);

  /** Record a run's findings and pop the panel when the pop rule says so. */
  function showFindings(p: PrecheckFindings, popup: boolean, actor: Actor) {
    if (p.findings.length === 0) {
      setPrecheck(null);
      setPrecheckOpen(false);
      return;
    }
    setPrecheck(p);
    const key = findingsKey(p.findings);
    const shown = precheckShownRef.current;
    if (shouldPop({ enabled: precheckPopup, popup, key, lastShown: shown.get(p.target) })) {
      shown.set(p.target, key);
      announcePanel(p, 'auto', actor);
    }
  }
  function announcePanel(p: PrecheckFindings, via: PrecheckPanelVia, actor: Actor) {
    setPrecheckOpen(true);
    emit({
      scope: 'compile',
      kind: 'info',
      actor,
      message: `pre-compile warnings: ${p.findings.length} for ${p.target}`,
      event: {
        action: 'precheck.panel-shown',
        target: p.target,
        count: p.findings.length,
        via,
      },
    });
  }
  /** Open the panel on request (status-bar chip or Tools menu). */
  const openPrecheck = () => {
    if (precheck) announcePanel(precheck, 'request', 'user');
  };
  const setPrecheckPopup = (on: boolean) => {
    setPrecheckPopupState(on);
    savePrecheckPopup(on);
    emit({
      scope: 'compile',
      kind: 'info',
      actor: 'user',
      message: on ? 'pre-compile warnings popup on' : 'pre-compile warnings popup off',
      event: { action: 'precheck.popup-setting', on },
    });
  };
  const closePrecheck = (dontShowAgain: boolean) => {
    setPrecheckOpen(false);
    emit({
      scope: 'compile',
      kind: 'info',
      actor: 'user',
      message: dontShowAgain
        ? 'pre-compile warnings closed (popup off)'
        : 'pre-compile warnings closed',
      event: { action: 'precheck.panel-dismissed', dontShowAgain },
    });
    if (dontShowAgain) setPrecheckPopup(false);
  };
  const setAutoCompile = (on: boolean) => {
    setAutoCompileState(on);
    feedAuto.current({ kind: 'enable', on });
    try {
      store().set(DEVICE_PREF_KEYS.autoCompile, String(on));
    } catch {
      // No app store configured: the toggle lasts for this session only.
    }
  };

  /** One networked compile of the main file, proven offline right after. */
  async function makeOffline() {
    await runCompile(mainFile ?? (fileName.includes('/') ? fileName : null), { networked: true });
  }

  // Cache-warm on open: a background compile of a freshly opened project's
  // main file, after the editor is populated. The write phase is skipped
  // (skipPersist: disk is fresh, warm never writes). Two rules: (1) this
  // runs only when the engine cache is usable — no cache entry means a full
  // cold build, which is the user's explicit Ctrl+R to pay for, not open's;
  // (2) a warm failure is quiet (debug line only) — open must never look
  // broken because a background guess failed; the user's explicit Ctrl+R
  // reports loudly through the normal path.
  // `project` is passed explicitly: warm runs right after an open, before
  // this closure has seen the new root. `cold` builds even without cached
  // output: a project the user just created or asked to open by name.
  async function warmCompile(mainAbsPath: string, project: SessionRoot, cold = false) {
    const src = sourceFor(mainAbsPath, [project]);
    const usable = src != null && (cold || (await outputsFresh(src.rootId, src.mainRel)));
    if (!usable) {
      emit({
        scope: 'compile',
        kind: 'info',
        actor: 'system',
        message: 'preview will build on first Compile (no cached output)',
        event: { action: 'compile.warm-skipped', reason: 'no-cached-output', target: mainAbsPath },
      });
      return;
    }
    emit({
      scope: 'compile',
      kind: 'info',
      actor: 'system',
      message: 'warming preview for ' + mainAbsPath,
      event: { action: 'compile.warm', target: mainAbsPath },
    });
    await runCompile(mainAbsPath, { skipPersist: true, root: project });
    // Not owned (user raced us) → their stream wins; nothing to report.
  }

  async function handleCompileFile(path: string) {
    if (!path.endsWith('.tex')) {
      emit({
        scope: 'compile',
        kind: 'error',
        actor: 'user',
        message: 'compile blocked: open a .tex file first',
        event: { action: 'compile.blocked', reason: 'not-a-tex-file', target: path },
      });
      return;
    }
    emit({
      scope: 'compile',
      kind: 'info',
      actor: 'user',
      message: `compiling ${path} directly (one-off, not the main file)`,
      event: { action: 'compile.one-off', target: path },
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
    offline,
    compiled,
    progress: progressLabel(progress),
    autoCompile,
    setAutoCompile,
    makeOffline,
    warmCompile,
    handleCompileFile,
    precheck,
    precheckOpen,
    openPrecheck,
    closePrecheck,
    precheckPopup,
    setPrecheckPopup,
  };
}
