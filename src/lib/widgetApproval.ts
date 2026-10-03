// widgetApproval.ts — the Widgets panel's model and the approval prompt.
//
// Reads (status, review) go through the shared operation contract. Approve,
// revoke and auto-approval are dedicated desktop commands outside it, so no
// MCP tool can reach them. The model holds the panel state with injected IO;
// the hook binds it to React and the real commands.

import type {
  ApprovalRequired,
  WidgetApprovalStatus,
  WidgetApproveParams,
  WidgetAutoApproveParams,
  WidgetReview,
  WidgetRevokeParams,
  WidgetsStatus,
} from './generated/api';
import type { BusEvent } from './generated/events';
import { eventOf } from './events';
import { type ApprovalPrompt, originLines, promptFrom, WIDGETS_PANEL_PATH } from './widgets.view';

export interface WidgetsIo {
  status(rootId: string, mainRel: string): Promise<WidgetsStatus>;
  review(rootId: string, mainRel: string, widget: string): Promise<WidgetReview>;
  approve(p: WidgetApproveParams): Promise<WidgetApprovalStatus>;
  revoke(p: WidgetRevokeParams): Promise<string>;
  setAutoApprove(p: WidgetAutoApproveParams): Promise<boolean>;
}

export interface WidgetsState {
  status: WidgetsStatus | null;
  loading: boolean;
  /** Last failure to show (a refused approval, an unreadable project). */
  notice: string | null;
  /** Widget ids with an action in flight. */
  busy: readonly string[];
  /** Loaded reviews by widget id. */
  reviews: Readonly<Record<string, WidgetReview>>;
}

const EMPTY: WidgetsState = { status: null, loading: false, notice: null, busy: [], reviews: {} };

function msg(e: unknown): string {
  return String(e instanceof Error ? e.message : e).slice(0, 300);
}

/** Panel state for one project and main file. */
export function createWidgetsModel(io: WidgetsIo, rootId: string, mainRel: string) {
  let state: WidgetsState = EMPTY;
  const subs = new Set<() => void>();
  const set = (patch: Partial<WidgetsState>) => {
    state = { ...state, ...patch };
    subs.forEach((f) => f());
  };
  const withBusy = async (id: string, fn: () => Promise<void>) => {
    set({ busy: [...state.busy, id], notice: null });
    try {
      await fn();
    } finally {
      set({ busy: state.busy.filter((b) => b !== id) });
    }
  };
  const dropReview = (id: string) => {
    return Object.fromEntries(Object.entries(state.reviews).filter(([k]) => k !== id));
  };

  async function refresh(): Promise<void> {
    set({ loading: true });
    try {
      const status = await io.status(rootId, mainRel);
      // A review for a digest the folder no longer has would mislead.
      const reviews = Object.fromEntries(
        Object.entries(state.reviews).filter(([id, r]) =>
          status.widgets.some((w) => w.widget === id && w.digest === r.status.digest),
        ),
      );
      set({ status, loading: false, notice: status.storeError ?? state.notice, reviews });
    } catch (e) {
      set({ loading: false, notice: `Could not read the widgets: ${msg(e)}` });
    }
  }

  async function loadReview(id: string): Promise<void> {
    await withBusy(id, async () => {
      try {
        const r = await io.review(rootId, mainRel, id);
        set({ reviews: { ...state.reviews, [id]: r } });
      } catch (e) {
        set({ notice: `Could not load the source of '${id}': ${msg(e)}` });
      }
    });
  }

  function hideReview(id: string): void {
    set({ reviews: dropReview(id) });
  }

  /** The digest the user last looked at: the loaded review, else the listing. */
  function reviewedDigest(id: string): string | null {
    return (
      state.reviews[id]?.status.digest ??
      state.status?.widgets.find((w) => w.widget === id)?.digest ??
      null
    );
  }

  async function approve(id: string): Promise<void> {
    const digest = reviewedDigest(id);
    if (!digest) return;
    await withBusy(id, async () => {
      try {
        await io.approve({ rootId, mainRel, widget: id, digest });
        set({ reviews: dropReview(id) });
      } catch (e) {
        // The folder moved under the review: show the new state, not the old.
        set({ notice: `Not approved: ${msg(e)}`, reviews: dropReview(id) });
      }
    });
    await refresh();
  }

  async function revoke(id: string): Promise<void> {
    const path = state.status?.widgets.find((w) => w.widget === id)?.path;
    if (!path) return;
    await withBusy(id, async () => {
      try {
        await io.revoke({ rootId, path });
        set({ reviews: dropReview(id) });
      } catch (e) {
        set({ notice: `Could not revoke '${id}': ${msg(e)}` });
      }
    });
    await refresh();
  }

  async function setAutoApprove(on: boolean): Promise<void> {
    set({ notice: null });
    try {
      await io.setAutoApprove({ rootId, on });
    } catch (e) {
      set({ notice: `Could not change auto-approval: ${msg(e)}` });
    }
    await refresh();
  }

  return {
    get: () => state,
    subscribe(cb: () => void) {
      subs.add(cb);
      return () => {
        subs.delete(cb);
      };
    },
    refresh,
    loadReview,
    hideReview,
    approve,
    revoke,
    setAutoApprove,
  };
}

export type WidgetsModel = ReturnType<typeof createWidgetsModel>;

// ---- Approval prompt ---------------------------------------------------

/**
 * The prompt a bus event asks for: a compile found an html widget waiting for
 * approval, or one changed and is not auto-approved.
 */
export function promptFromEvent(e: BusEvent, rootId: string): ApprovalPrompt | null {
  const need = eventOf(e, 'widget.approval-required');
  if (need) {
    if (need.rootId !== rootId) return null;
    return {
      key: `${need.path}@${need.digest}`,
      widget: need.widget,
      path: need.path,
      digest: need.digest,
      cause: need.cause,
      origins: originLines({
        connectDomains: need.connectDomains,
        resourceDomains: need.resourceDomains,
        frameDomains: need.frameDomains,
      }),
    };
  }
  const ev = eventOf(e, 'widget.digest-changed');
  if (!ev || ev.rootId !== rootId || ev.autoApproved) return null;
  return {
    key: `${ev.path}@${ev.digest}`,
    widget: ev.widget,
    path: ev.path,
    digest: ev.digest,
    cause: ev.cause,
    origins: [],
  };
}

/** True when a bus event means the panel's data is stale. */
export function widgetEventFor(e: BusEvent, rootId: string): boolean {
  for (const a of ['widget.approved', 'widget.revoked', 'widgets.auto-approve'] as const) {
    const ev = eventOf(e, a);
    if (ev && ev.rootId === rootId) return true;
  }
  return false;
}

/**
 * The queue of prompts the user is asked about, one per widget version. A
 * skipped version is not asked about again; a changed folder is a new
 * version and is.
 */
export function createPromptQueue(io: Pick<WidgetsIo, 'approve'>, rootId: string, mainRel: string) {
  let queue: readonly ApprovalPrompt[] = [];
  let failure: string | null = null;
  const skipped = new Set<string>();
  const subs = new Set<() => void>();
  const set = (q: readonly ApprovalPrompt[], f: string | null) => {
    queue = q;
    failure = f;
    subs.forEach((cb) => cb());
  };
  const enqueue = (p: ApprovalPrompt) => {
    if (skipped.has(p.key) || queue.some((q) => q.key === p.key)) return;
    set([...queue, p], null);
  };
  return {
    /** Entry point for the poster renderer: an html widget needs approval. */
    onApprovalRequired: (r: ApprovalRequired) => enqueue(promptFrom(r)),
    enqueue,
    current: (): ApprovalPrompt | null => queue[0] ?? null,
    failure: () => failure,
    /** Approve the version the prompt shows; refused if the folder changed since. */
    async approve() {
      const p = queue[0];
      if (!p) return;
      try {
        await io.approve({ rootId, mainRel, widget: p.widget, digest: p.digest });
        set(queue.slice(1), null);
      } catch (e) {
        set(queue.slice(1), `Not approved: ${msg(e)} Review it in ${WIDGETS_PANEL_PATH}.`);
      }
    },
    skip() {
      if (queue[0]) skipped.add(queue[0].key);
      set(queue.slice(1), null);
    },
    /**
     * The user acted on this widget folder elsewhere (approved or revoked it
     * in the panel): what was skipped or queued for it is stale.
     */
    settled(path: string) {
      const stale = (k: string) => k.startsWith(`${path}@`);
      [...skipped].filter(stale).forEach((k) => skipped.delete(k));
      const rest = queue.filter((q) => !stale(q.key));
      if (rest.length !== queue.length) set(rest, failure);
    },
    clearFailure() {
      set(queue, null);
    },
    subscribe(cb: () => void) {
      subs.add(cb);
      return () => {
        subs.delete(cb);
      };
    },
  };
}

export type PromptQueue = ReturnType<typeof createPromptQueue>;
