// useProjectTree.ts — the open project: its file tree, watcher, and recents.
//
// Opening is grant-first: the runtime scope grant precedes any read, so an
// unreachable root fails before the tree is touched. The watcher coalesces
// bursts and suppresses echoes of our own writes.

import { useCallback, useEffect, useRef, useState } from 'react';
import type { RefObject } from 'react';
import { emit } from '../lib/events';
import { listDir1Level, listTreeDeep, openProject, type TreeEntry } from '../lib/files';
import { grantProjectAccess } from '../lib/projectAccess';
import { getRecentProjects, pruneRecentProjects, touchRecentProject } from '../lib/recentProjects';
import { coalesceEvents, debounce } from '../lib/watcher';
import { watchBackend, type WatchChangeEvent } from '../lib/watch-backend';
import { projectIndex } from '../lib/project-index';
import type { OwnWrites } from '../lib/own-writes';
import { fs } from '../lib/fs-provider';
import type { FileHistory } from '../lib/file-history';
import type { SessionRoot } from '../lib/preview-bus';

export interface UseProjectTreeDeps {
  root: string | null;
  projectId: string | null;
  setRoot: (v: string | null) => void;
  setProjectId: (v: string | null) => void;
  setTree: (v: TreeEntry[]) => void;
  fileNameRef: RefObject<string>;
  ownWrites: OwnWrites;
  setReloadPath: (v: string | null) => void;
  setLog: (v: string) => void;
  trash: FileHistory;
  resolveMain: (r: string, opened: string | null) => Promise<string | null>;
  handleSelect: (path: string) => Promise<void>;
  warmCompile: (mainAbsPath: string, project: SessionRoot) => Promise<void>;
}

export function useProjectTree(deps: UseProjectTreeDeps) {
  const {
    root,
    projectId,
    setRoot,
    setProjectId,
    setTree,
    fileNameRef,
    ownWrites,
    setReloadPath,
    setLog,
    trash,
    resolveMain,
    handleSelect,
    warmCompile,
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

  // Watcher: notify + debounce/coalesce. Tree refreshes on create/rename;
  // open-file edits offer reload; on-disk deletes mark the buffer.
  useEffect(() => {
    if (!root || !projectId) return;
    const rootId = projectId;
    let unwatch: (() => void) | null = null;
    let cancelled = false;
    const pending: WatchChangeEvent[] = [];
    const indexFailed = (e: unknown) =>
      emit({
        scope: 'fs',
        kind: 'warn',
        actor: 'system',
        message: 'project index not updated: ' + String(e).slice(0, 120),
        event: { action: 'index.failed', root, error: String(e).slice(0, 200) },
      });
    const flush = debounce(async () => {
      if (cancelled || pending.length === 0) return;
      const batch = coalesceEvents(pending.splice(0));
      void reloadTree(root);
      // Every change, own writes included, is what the disk now holds.
      projectIndex()
        .touch(
          rootId,
          batch.map((ev) => ev.path),
        )
        .catch(indexFailed);
      for (const ev of batch) {
        // Echoes of our own writes: the disk still holds what we wrote.
        if (await ownWrites.isEcho(ev.path)) continue;
        if (cancelled) return;
        if (ev.path === fileNameRef.current && ev.kind === 'modify') setReloadPath(ev.path);
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
    }, 250);
    watchBackend()
      .watch(root, (changes) => {
        pending.push(...changes);
        flush();
      })
      .then(
        (u) => {
          if (!cancelled) {
            unwatch = u;
            projectIndex().watched(rootId, true).catch(indexFailed);
          } else u();
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
      if (unwatch) {
        unwatch();
        projectIndex().watched(rootId, false).catch(indexFailed);
      }
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [root, projectId]);

  // Recents for File > Open Recent as state. Restored entries re-validate
  // via stat.
  const [recentProjects, setRecentProjects] = useState<string[]>(() => getRecentProjects());
  async function openRoot(r: string, opts?: { warm?: boolean }) {
    // The runtime scope grant comes first: every fs call below resolves
    // through it. The backend fails closed on invalid roots
    // (empty/NUL/relative/missing/non-dir); a failed grant leaves the
    // current project untouched.
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
    const canon = grant.path;
    setRoot(canon);
    setProjectId(grant.rootId);
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
    const m = await resolveMain(canon, null);
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
    // Cache-warm on open: a background compile starts after the editor is
    // populated — but only when the engine cache is usable (previous output
    // present). No cache → no surprise build; the preview waits for the
    // user's explicit Ctrl+R. Open never fails because warm failed.
    if (opts?.warm && m) void warmCompile(m, { rootId: grant.rootId, path: canon });
  }

  async function open() {
    const r = await openProject();
    if (r) {
      await openRoot(r, { warm: true });
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

  // Restore-on-launch: the `?project=` preset wins, else the most recent
  // project that still resolves, else the Hello sample (no project forced).
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
      const stale: string[] = [];
      for (const r of recents) {
        try {
          // Grant first: the validation stat below resolves only through the
          // runtime grant. Unreachable entries land in `stale` here.
          const grant = await grantProjectAccess(r);
          if (!grant.ok || !grant.path) throw new Error(grant.error ?? 'grant failed');
          if ((await fs().stat(grant.path)) === null) throw new Error('root unreachable');
          await openRoot(grant.path, { warm: true });
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

  return { reloadTree, openRoot, open, recentProjects, setRecentProjects };
}
