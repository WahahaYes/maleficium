// useProjectTree.ts — the open project: its file tree, watcher, and recents.
//
// Opening is grant-first: the backend validates and registers the root
// before the tree is touched, so an unreachable root fails first. Reads go
// through the core file service; the core watcher tracks the tree while it
// is open, feeding the index and queueing external changes this effect
// drains.

import { useCallback, useEffect, useRef, useState } from 'react';
import type { RefObject } from 'react';
import { emit } from '../lib/events';
import { listDir1Level, listTreeDeep, openProject, type TreeEntry } from '../lib/files';
import { grantProjectAccess } from '../lib/projectAccess';
import { getRecentProjects, pruneRecentProjects, touchRecentProject } from '../lib/recentProjects';
import { watchBackend, type WatchChangeEvent } from '../lib/watch-backend';
import { projectIndex } from '../lib/project-index';
import { fs } from '../lib/fs-provider';
import type { FileHistory } from '../lib/file-history';
import type { SessionRoot } from '../lib/preview-bus';
import { welcomeProject } from '../lib/templates';
import { setMainFile } from '../lib/mainFile.tauri';

export interface UseProjectTreeDeps {
  root: string | null;
  projectId: string | null;
  setRoot: (v: string | null) => void;
  setProjectId: (v: string | null) => void;
  setTree: (v: TreeEntry[]) => void;
  fileNameRef: RefObject<string>;
  /** Re-check open buffers whose files changed on disk. */
  checkExternal: (paths: readonly string[]) => Promise<void>;
  setLog: (v: string) => void;
  trash: FileHistory;
  resolveMain: (r: string, rootId: string, opened: string | null) => Promise<string | null>;
  clearMainFile: () => void;
  handleSelect: (path: string) => Promise<void>;
  warmCompile: (mainAbsPath: string, project: SessionRoot, cold?: boolean) => Promise<void>;
  /** Close every open file (saving the dirty one); false when one stays open. */
  closeAll: () => Promise<boolean>;
}

export function useProjectTree(deps: UseProjectTreeDeps) {
  const {
    root,
    projectId,
    setRoot,
    setProjectId,
    setTree,
    fileNameRef,
    checkExternal,
    setLog,
    trash,
    resolveMain,
    clearMainFile,
    handleSelect,
    warmCompile,
    closeAll,
  } = deps;

  const reloadTree = useCallback(
    async (r: string, deep = false) => {
      const t0 = performance.now();
      let t: TreeEntry[];
      try {
        t = deep ? await listTreeDeep(r) : await listDir1Level(r);
      } catch (e) {
        setTree([]);
        emit({
          scope: 'fs',
          kind: 'error',
          actor: 'system',
          message: 'could not list ' + r,
          event: { action: 'tree.load-failed', dir: r, error: String(e).slice(0, 200) },
        });
        return;
      }
      setTree(t);
      const dt = Math.round(performance.now() - t0);
      emit({
        scope: 'fs',
        kind: 'info',
        actor: 'system',
        message: `tree ${deep ? 'full' : 'root'} loaded ${t.length} rows in ${dt}ms`,
        event: { action: 'tree.load', rows: t.length, ms: dt, deep },
      });
    },
    [setTree],
  );

  // Watcher: the core thread feeds the index and filters our own writes,
  // so each poll batch is someone else's change. The tree refreshes, open
  // buffers re-check their files, and on-disk deletes mark the buffer.
  // Stopping ends the poll and the core watch alongside it.
  useEffect(() => {
    if (!root || !projectId) return;
    let unwatch: (() => void) | null = null;
    let cancelled = false;
    const flush = (batch: WatchChangeEvent[]) => {
      if (cancelled || batch.length === 0) return;
      void reloadTree(root);
      void checkExternal(batch.filter((ev) => ev.kind !== 'delete').map((ev) => ev.path));
      for (const ev of batch) {
        if (cancelled) return;
        if (ev.path === fileNameRef.current && ev.kind === 'delete') {
          setLog('deleted on disk: ' + ev.path);
          emit({
            scope: 'fs',
            kind: 'warn',
            actor: 'system',
            message: 'deleted on disk: ' + ev.path,
            event: { action: 'fs.external-delete', path: ev.path },
          });
        }
        emit({
          scope: 'fs',
          kind: 'info',
          actor: 'system',
          message: `external ${ev.kind} ${ev.path}`,
          event: { action: 'fs.external', change: ev.kind, path: ev.path },
        });
      }
    };
    watchBackend()
      .watch(root, (changes) => {
        flush(changes);
      })
      .then(
        (u) => {
          if (!cancelled) unwatch = u;
          else u();
        },
        (e: unknown) => {
          // The tree still refreshes on reload; say so rather than silently
          // losing external changes.
          if (cancelled) return;
          emit({
            scope: 'fs',
            kind: 'warn',
            actor: 'system',
            message:
              'live file tracking unavailable: external changes need File > Reload from Disk',
            event: { action: 'fs.watch-unavailable', root, error: String(e).slice(0, 200) },
          });
        },
      );
    return () => {
      cancelled = true;
      if (unwatch) unwatch();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [root, projectId]);

  // Recents for File > Open Recent as state. Restored entries re-validate
  // via stat.
  const [recentProjects, setRecentProjects] = useState<string[]>(() => getRecentProjects());
  /** Open project `r`: the previous project's files close, the main file
   *  opens and compiles (with `compile: false`, only from a usable cache). */
  async function openRoot(r: string, opts?: { compile?: boolean; main?: string }) {
    // The backend grant comes first: validation fails closed on invalid
    // roots (empty/NUL/relative/missing/non-dir), and the grant binds the
    // root for the core file service. A failed grant leaves the current
    // project untouched.
    const grant = await grantProjectAccess(r);
    if (!grant.ok || !grant.path || !grant.rootId) {
      const reason = (grant.error ?? 'grant failed').slice(0, 200);
      const msg = 'open refused: ' + reason;
      setLog(msg);
      emit({
        scope: 'fs',
        kind: 'error',
        actor: 'user',
        message: msg,
        event: { action: 'project.open-refused', root: r, error: reason },
      });
      return;
    }
    // The previous project's files close first (the dirty one is saved);
    // one that cannot be saved stays open and the switch is called off.
    if (!(await closeAll())) {
      const msg = 'open cancelled: an open file could not be saved';
      setLog(msg);
      emit({
        scope: 'fs',
        kind: 'warn',
        actor: 'user',
        message: msg,
        event: { action: 'project.open-cancelled' },
      });
      return;
    }
    const canon = grant.path;
    setRoot(canon);
    setProjectId(grant.rootId);
    // Drop the previous root's main file now: resolveMain below is async,
    // and a compile fired mid-switch must never see the old root's target.
    clearMainFile();
    // A freshly created project names its main file; record it under the
    // backend's root id before resolving.
    if (opts?.main) void setMainFile(grant.rootId, grant.path, opts.main);
    setRecentProjects(touchRecentProject(canon));
    const t0 = performance.now();
    projectIndex()
      .open(grant.rootId)
      .then(
        (files) =>
          emit({
            scope: 'fs',
            kind: 'info',
            actor: 'system',
            message: `project index: ${files} files in ${Math.round(performance.now() - t0)}ms`,
            event: { action: 'index.open', files, ms: Math.round(performance.now() - t0) },
          }),
        (e: unknown) =>
          emit({
            scope: 'fs',
            kind: 'warn',
            actor: 'system',
            message: 'project index unavailable: ' + String(e).slice(0, 120),
            event: { action: 'index.failed', root: canon, error: String(e).slice(0, 200) },
          }),
      );
    await reloadTree(canon, false);
    setLog('opened ' + canon);
    emit({
      scope: 'fs',
      kind: 'info',
      actor: 'user',
      message: 'opened ' + canon,
      event: { action: 'project.open', root: canon },
    });
    trash.clear();
    const m = await resolveMain(canon, grant.rootId, null);
    setLog(m ? `opened ${canon} (main: ${m})` : `opened ${canon} (no main file found)`);
    emit({
      scope: 'fs',
      kind: m ? 'success' : 'warn',
      actor: 'system',
      message: m ? 'main file ' + m : 'no main file found in ' + canon,
      event: { action: 'main.resolved', root: canon, mainFile: m },
    });
    // Open the main file on project select — the editor must never sit on
    // stale untitled content while the tree shows a project. No main →
    // keep the current editor as-is.
    if (m) await handleSelect(m);
    // Compile on open, once the editor is populated, so the preview and
    // the article are current. `compile: false` (the first-launch tour)
    // builds only from a usable cache, never starting a cold build. Open
    // never fails because the compile did.
    if (m) void warmCompile(m, { rootId: grant.rootId, path: canon }, opts?.compile !== false);
  }

  async function open() {
    const r = await openProject();
    if (r) {
      await openRoot(r);
    } else {
      setLog('open cancelled');
      emit({
        scope: 'fs',
        kind: 'warn',
        actor: 'user',
        message: 'cancelled',
        event: { action: 'project.open-cancelled' },
      });
    }
  }

  /** Open the welcome tour (written into app data the first time); `compile` builds it. */
  async function openWelcome(compile = true) {
    try {
      const c = await welcomeProject();
      emit({
        scope: 'app',
        kind: 'info',
        actor: 'system',
        message: 'welcome project: ' + c.root,
        event: { action: 'template.welcome', root: c.root },
      });
      await openRoot(c.root, { compile, main: c.main });
    } catch (e) {
      emit({
        scope: 'app',
        kind: 'error',
        actor: 'system',
        message: 'welcome project unavailable: ' + String(e).slice(0, 200),
        event: {
          action: 'template.create-failed',
          template: 'welcome',
          error: String(e).slice(0, 200),
        },
      });
    }
  }

  // Restore-on-launch: the `?project=` preset wins, else the most recent
  // project that still resolves; a first run (no recents at all) opens the
  // welcome tour.
  // Runs once; only roots that all fail validation are pruned.
  const restoredRef = useRef(false);
  useEffect(() => {
    if (restoredRef.current) return;
    restoredRef.current = true;
    void (async () => {
      try {
        const q = new URLSearchParams(window.location.search);
        const h = window.location.hash.match(/project=([^&]+)/);
        if (q.get('project') || h) return; // openProject() preset path owns it
      } catch {
        /* non-browser — fall through to recents */
      }
      const recents = getRecentProjects();
      if (recents.length === 0) {
        // First launch: show the tour without starting the engine download.
        await openWelcome(false);
        return;
      }
      const stale: string[] = [];
      for (const r of recents) {
        try {
          // Grant first: the validation stat below resolves through the
          // core file service once the grant binds the root. Unreachable
          // entries land in `stale` here.
          const grant = await grantProjectAccess(r);
          if (!grant.ok || !grant.path) throw new Error(grant.error ?? 'grant failed');
          if ((await fs().stat(grant.path)) === null) throw new Error('root unreachable');
          await openRoot(grant.path);
          return;
        } catch {
          // Moved, deleted, or no longer grantable: pruned below.
          stale.push(r);
        }
      }
      if (stale.length > 0 && stale.length === recents.length) {
        setRecentProjects(pruneRecentProjects((kept) => !stale.includes(kept)));
      }
    })();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return { reloadTree, openRoot, open, openWelcome, recentProjects, setRecentProjects };
}
