// useRevisionHistory.ts — app-local revision snapshots for project files.
//
// Snapshots are keyed by (projectId, relPath): a file outside the open
// project has no history and reads zero. Restore writes the revision back to
// disk and the store keeps the replaced state as a revision of its own, so
// the list is always the way back.

import { useCallback, useEffect, useMemo, useState } from 'react';
import type { RefObject } from 'react';
import { emit } from '../lib/events';
import { revisionRecordData, revisionRestoreData } from '../lib/eventlog';
import { createHistoryStore } from '../lib/history';
import {
  buildRevisionRows,
  historyAvailability,
  retentionSummary,
  truncationNotice,
  type HistoryAvailability,
  type RevisionRow,
} from '../lib/history.view';
import { hashRoot } from '../lib/paths';
import { markSaved, updateBuffer, type BufferState } from '../lib/buffers';

export interface UseRevisionHistoryDeps {
  root: string | null;
  rootRef: RefObject<string | null>;
  projectId: string | null;
  relInProject: (abs: string) => string | null;
  fileName: string;
  setBuffers: React.Dispatch<React.SetStateAction<Map<string, BufferState>>>;
  setTex: (v: string) => void;
  setReloadPath: (v: string | null) => void;
  setLog: (v: string) => void;
  markOwnWrite: (p: string) => void;
}

export function useRevisionHistory(deps: UseRevisionHistoryDeps) {
  const {
    root,
    rootRef,
    projectId,
    relInProject,
    fileName,
    setBuffers,
    setTex,
    setReloadPath,
    setLog,
    markOwnWrite,
  } = deps;

  const [revisionCount, setRevisionCount] = useState(0);
  const [historyOpen, setHistoryOpen] = useState(false);
  const [historyRows, setHistoryRows] = useState<RevisionRow[]>([]);
  const [historyAvail, setHistoryAvail] = useState<HistoryAvailability>('unavailable');
  const [historySummary, setHistorySummary] = useState<string | null>(null);
  const [historyNotice, setHistoryNotice] = useState<string | null>(null);
  const [restoringRev, setRestoringRev] = useState<string | null>(null);
  const history = useMemo(
    () =>
      createHistoryStore((id) => {
        const r = rootRef.current;
        return r && hashRoot(r) === id ? r : null;
      }),
    [rootRef],
  );
  const refreshRevisionCount = useCallback(
    async (path: string): Promise<number> => {
      const rel = relInProject(path);
      if (!projectId || !rel) {
        setRevisionCount(0);
        return 0;
      }
      const n = (await history.listRevisions(projectId, rel)).length;
      setRevisionCount(n);
      return n;
    },
    [history, projectId, relInProject],
  );
  /** Snapshot a saved file. Ineligible files and an unreachable store are quiet. */
  const recordRevision = useCallback(
    async (path: string, text: string) => {
      const rel = relInProject(path);
      if (!projectId || !rel) return;
      const outcome = await history.recordRevision(projectId, rel, new TextEncoder().encode(text));
      const revisions = await refreshRevisionCount(path);
      emit({
        scope: 'fs',
        kind: 'info',
        message: outcome.stored
          ? `revision ${outcome.rev} of ${rel} (${revisions} kept)`
          : `no revision for ${rel} (${outcome.reason})`,
        data: revisionRecordData(rel, outcome, revisions),
      });
    },
    [history, projectId, relInProject, refreshRevisionCount],
  );
  // The counter follows the active file; a file outside the project reads 0.
  useEffect(() => {
    void refreshRevisionCount(fileName);
  }, [fileName, refreshRevisionCount]);

  const openHistory = useCallback(async () => {
    const rel = relInProject(fileName);
    const avail = historyAvailability({
      hasProject: root != null,
      relPath: rel,
      storeReady: projectId != null,
    });
    setHistoryAvail(avail);
    setHistoryOpen(true);
    if (avail !== 'ready' || !projectId || !rel) {
      setHistoryRows([]);
      setHistorySummary(null);
      setHistoryNotice(null);
      return;
    }
    const [revs, info] = await Promise.all([
      history.listRevisions(projectId, rel),
      history.retentionInfo(projectId),
    ]);
    const rows = buildRevisionRows(revs, Date.now());
    setHistoryRows(rows);
    setHistorySummary(retentionSummary(info));
    setHistoryNotice(truncationNotice(rows.length, info));
    setRevisionCount(revs.length);
  }, [fileName, history, projectId, relInProject, root]);

  // Restore writes the revision back to disk; the store keeps the replaced
  // state as a revision of its own, so the list is the way back.
  const restoreRevision = useCallback(
    async (rev: string) => {
      const rel = relInProject(fileName);
      if (!projectId || !rel) return;
      setRestoringRev(rev);
      try {
        const bytes = await history.restoreRevision(projectId, rel, rev);
        if (!bytes) {
          setLog('restore unavailable');
          emit({
            scope: 'fs',
            kind: 'warn',
            message: 'restore unavailable for ' + rel,
            data: { action: 'revision.restore-unavailable', rel, rev },
          });
          return;
        }
        const text = new TextDecoder().decode(bytes);
        markOwnWrite(fileName);
        setBuffers((b) => markSaved(updateBuffer(b, fileName, text), fileName));
        setTex(text);
        setReloadPath(null);
        setLog('restored ' + rel);
        emit({
          scope: 'fs',
          kind: 'success',
          message: 'restored ' + rel,
          data: revisionRestoreData(rel, rev, text.length),
        });
      } finally {
        setRestoringRev(null);
      }
      await openHistory();
    },
    [
      fileName,
      setBuffers,
      setLog,
      setReloadPath,
      setTex,
      history,
      markOwnWrite,
      openHistory,
      projectId,
      relInProject,
    ],
  );

  return {
    revisionCount,
    historyOpen,
    setHistoryOpen,
    historyRows,
    historyAvail,
    historySummary,
    historyNotice,
    setHistoryNotice,
    restoringRev,
    refreshRevisionCount,
    recordRevision,
    openHistory,
    restoreRevision,
  };
}
