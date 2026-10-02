import { describe, expect, it, vi } from 'vitest';
import type { ApprovalRequired, WidgetApprovalStatus, WidgetsStatus } from './generated/api';
import type { BusEvent } from './generated/events';
import {
  createPromptQueue,
  createWidgetsModel,
  promptFromEvent,
  widgetEventFor,
  type WidgetsIo,
} from './widgetApproval';

const NO = { connectDomains: [], resourceDomains: [], frameDomains: [] };
const REQ = (over: Partial<ApprovalRequired> = {}) =>
  ({
    status: 'approval_required',
    kind: 'html_widget',
    widget: 'sim',
    path: 'figs/sim',
    digest: 'd1',
    cause: 'never_approved',
    declaredOrigins: NO,
    autoApprove: false,
    panel: 'View > Widgets',
    whatHappens: '',
    userAction: '',
    agentMustNot: [],
    message: '',
    ...over,
  }) as WidgetApprovalStatus;

function status(w: WidgetApprovalStatus[], autoApprove = false): WidgetsStatus {
  return { autoApprove, pending: w.length, widgets: w, unavailable: [], exempt: [] };
}

function fakeIo(over: Partial<WidgetsIo> = {}) {
  let current = status([REQ()]);
  const io = {
    status: vi.fn(async () => current),
    review: vi.fn(async () => ({
      status: current.widgets[0],
      files: [
        { path: 'index.html', change: 'modified' as const, before: 'a', after: 'b', binary: false },
      ],
    })),
    approve: vi.fn(async () => current.widgets[0]),
    revoke: vi.fn(async () => 'figs/sim'),
    setAutoApprove: vi.fn(async (p: { on: boolean }) => p.on),
    ...over,
  } as unknown as WidgetsIo;
  return { io, set: (s: WidgetsStatus) => (current = s) };
}

describe('widgets model', () => {
  it('lists widgets on refresh and reports an unreadable project', async () => {
    const { io } = fakeIo();
    const m = createWidgetsModel(io, 'r', 'main.tex');
    await m.refresh();
    expect(m.get().status?.widgets).toHaveLength(1);
    const bad = fakeIo({ status: vi.fn().mockRejectedValue(new Error('never compiled')) });
    const m2 = createWidgetsModel(bad.io, 'r', 'main.tex');
    await m2.refresh();
    expect(m2.get().notice).toContain('never compiled');
  });

  it('approves the digest that was reviewed, then refreshes', async () => {
    const f = fakeIo();
    const m = createWidgetsModel(f.io, 'r', 'main.tex');
    await m.refresh();
    await m.loadReview('sim');
    expect(m.get().reviews.sim.files).toHaveLength(1);
    // The folder moves after the review: approval must still carry the reviewed digest.
    f.set(status([REQ({ digest: 'd2' })]));
    await m.approve('sim');
    expect(f.io.approve).toHaveBeenCalledWith({
      rootId: 'r',
      mainRel: 'main.tex',
      widget: 'sim',
      digest: 'd1',
    });
    expect(f.io.status).toHaveBeenCalledTimes(2);
  });

  it('shows the refusal and drops the stale review when the folder changed', async () => {
    const f = fakeIo({ approve: vi.fn().mockRejectedValue(new Error('changed since review')) });
    const m = createWidgetsModel(f.io, 'r', 'main.tex');
    await m.refresh();
    await m.loadReview('sim');
    await m.approve('sim');
    expect(m.get().notice).toContain('Not approved: changed since review');
    expect(m.get().reviews.sim).toBeUndefined();
  });

  it('hides a loaded review on request', async () => {
    const f = fakeIo();
    const m = createWidgetsModel(f.io, 'r', 'main.tex');
    await m.refresh();
    await m.loadReview('sim');
    m.hideReview('sim');
    expect(m.get().reviews.sim).toBeUndefined();
  });

  it('revokes by the widget folder path', async () => {
    const f = fakeIo();
    const m = createWidgetsModel(f.io, 'r', 'main.tex');
    await m.refresh();
    await m.revoke('sim');
    expect(f.io.revoke).toHaveBeenCalledWith({ rootId: 'r', path: 'figs/sim' });
  });

  it('toggles auto-approval through the command and re-reads the setting', async () => {
    const f = fakeIo();
    const m = createWidgetsModel(f.io, 'r', 'main.tex');
    await m.refresh();
    f.set(status([REQ()], true));
    await m.setAutoApprove(true);
    expect(f.io.setAutoApprove).toHaveBeenCalledWith({ rootId: 'r', on: true });
    expect(m.get().status?.autoApprove).toBe(true);
  });

  it('drops reviews whose digest the folder no longer has', async () => {
    const f = fakeIo();
    const m = createWidgetsModel(f.io, 'r', 'main.tex');
    await m.refresh();
    await m.loadReview('sim');
    f.set(status([REQ({ digest: 'd9' })]));
    await m.refresh();
    expect(m.get().reviews.sim).toBeUndefined();
  });
});

describe('approval prompt', () => {
  const required = REQ({ digest: 'p1' }) as ApprovalRequired;

  it('queues a synthetic approval-required once per version and approves the shown digest', async () => {
    const approve = vi.fn(async () => REQ());
    const q = createPromptQueue({ approve }, 'r', 'main.tex');
    q.onApprovalRequired(required);
    q.onApprovalRequired(required);
    expect(q.current()?.widget).toBe('sim');
    await q.approve();
    expect(approve).toHaveBeenCalledOnce();
    expect(approve).toHaveBeenCalledWith({
      rootId: 'r',
      mainRel: 'main.tex',
      widget: 'sim',
      digest: 'p1',
    });
    expect(q.current()).toBeNull();
  });

  it('skip drops the prompt without approving', () => {
    const approve = vi.fn();
    const q = createPromptQueue({ approve }, 'r', 'main.tex');
    q.onApprovalRequired(required);
    q.skip();
    expect(q.current()).toBeNull();
    expect(approve).not.toHaveBeenCalled();
  });

  it('reports a refused approval and points at the panel', async () => {
    const q = createPromptQueue(
      { approve: vi.fn().mockRejectedValue(new Error('digest mismatch')) },
      'r',
      'm',
    );
    q.onApprovalRequired(required);
    await q.approve();
    expect(q.failure()).toContain('digest mismatch');
    expect(q.failure()).toContain('View > Widgets');
  });
});

describe('bus events', () => {
  const ev = (event: BusEvent['event']): BusEvent => ({
    scope: 'app',
    kind: 'info',
    actor: 'agent',
    message: '',
    at: 1,
    event,
  });
  const changed = (autoApproved: boolean, rootId = 'r') =>
    ev({
      action: 'widget.digest-changed',
      rootId,
      path: 'figs/sim',
      widget: 'sim',
      approvedDigest: 'a',
      digest: 'b',
      cause: 'changed_since_approval',
      autoApproved,
    });

  it('prompts for a change that needs the user, not for an auto-approved one or another project', () => {
    expect(promptFromEvent(changed(false), 'r')?.digest).toBe('b');
    expect(promptFromEvent(changed(true), 'r')).toBeNull();
    expect(promptFromEvent(changed(false, 'other'), 'r')).toBeNull();
  });

  it('flags approval, revoke and auto-approval events for this project as panel refreshes', () => {
    expect(widgetEventFor(ev({ action: 'widget.revoked', rootId: 'r', path: 'p' }), 'r')).toBe(
      true,
    );
    expect(widgetEventFor(ev({ action: 'widgets.auto-approve', rootId: 'x', on: true }), 'r')).toBe(
      false,
    );
  });
});
