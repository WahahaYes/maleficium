import { describe, expect, it, vi } from 'vitest';
import type { ApprovalRequired, WidgetApprovalStatus, WidgetsStatus } from './generated/api';
import {
  createRuntimeModel,
  createRuntimePromptQueue,
  offerRuntimeEvent,
  runtimeApprovalRef,
  runtimeEventFor,
  type WidgetsIo,
} from './widgetApproval';
import type { BusEvent } from './generated/events';
import {
  buildRuntimeRow,
  buildRuntimeRows,
  pendingExportRuntimes,
  promptFromRuntime,
  runtimePromptBody,
  runtimePromptTitle,
  runtimeUnavailable,
} from './widgets.view';

const NO = { connectDomains: [], resourceDomains: [], frameDomains: [] };
const INFO = {
  reference: 'stl-viewer@1',
  version: '1.2.0',
  title: 'STL viewer',
  description: 'Orbits meshes',
  authors: ['Example'],
  license: 'MIT',
  vendored: [{ name: 'three', version: '0.186.1', license: 'MIT' }],
  webgl: true,
  widgets: ['fig-part'],
  warnings: [],
};

const RUNTIME_REQ = (over: Partial<ApprovalRequired> = {}) =>
  ({
    status: 'approval_required',
    kind: 'custom_runtime',
    widget: 'fig-part',
    path: 'runtimes/stl-viewer@1',
    digest: 'e'.repeat(64),
    cause: 'never_approved',
    declaredOrigins: NO,
    autoApprove: false,
    panel: 'View > Widgets',
    whatHappens: '',
    userAction: '',
    agentMustNot: [],
    message: '',
    runtime: { ...INFO },
    ...over,
  }) as WidgetApprovalStatus;

function status(runtimes: WidgetApprovalStatus[]): WidgetsStatus {
  return {
    autoApprove: false,
    pending: runtimes.length,
    widgets: [],
    runtimes,
    unavailable: [],
    exempt: [],
  };
}

describe('runtime rows', () => {
  it('maps judge outcomes onto the panel sections', () => {
    expect(buildRuntimeRow(RUNTIME_REQ())?.section).toBe('needs');
    expect(
      buildRuntimeRow(RUNTIME_REQ({ cause: 'changed_since_approval', approvedDigest: 'a' }))
        ?.section,
    ).toBe('changed');
    expect(
      buildRuntimeRow(RUNTIME_REQ({ cause: 'license_or_vendored_changed', approvedDigest: 'a' }))
        ?.section,
    ).toBe('changed');
    expect(buildRuntimeRow(RUNTIME_REQ({ cause: 'revoked' }))?.section).toBe('denied');
    const allowed = buildRuntimeRow({
      status: 'approved',
      kind: 'custom_runtime',
      widget: 'fig-part',
      path: 'runtimes/stl-viewer@1',
      digest: 'e'.repeat(64),
      via: 'user',
      declaredOrigins: NO,
      autoApprove: false,
      runtime: { ...INFO },
    });
    expect(allowed?.section).toBe('allowed');
    const auto = buildRuntimeRow({
      status: 'approved',
      kind: 'custom_runtime',
      widget: 'fig-part',
      path: 'runtimes/stl-viewer@1',
      digest: 'f'.repeat(64),
      via: 'auto',
      declaredOrigins: NO,
      autoApprove: true,
      approvedDigest: 'e'.repeat(64),
      runtime: { ...INFO },
    });
    expect(auto?.section).toBe('allowed-auto');
  });

  it('enables Allow and Deny only where they mean something', () => {
    const needs = buildRuntimeRow(RUNTIME_REQ())!;
    expect([needs.canAllow, needs.canDeny]).toEqual([true, true]);
    const changed = buildRuntimeRow(
      RUNTIME_REQ({ cause: 'changed_since_approval', approvedDigest: 'a' }),
    )!;
    expect([changed.canAllow, changed.canDeny, changed.hasApproved]).toEqual([true, true, true]);
    const denied = buildRuntimeRow(RUNTIME_REQ({ cause: 'revoked' }))!;
    expect([denied.canAllow, denied.canDeny]).toEqual([true, false]);
  });

  it('ignores html widget entries', () => {
    expect(buildRuntimeRows(status([RUNTIME_REQ()]))).toHaveLength(1);
    expect(
      buildRuntimeRow({ ...RUNTIME_REQ(), kind: 'html_widget' } as WidgetApprovalStatus),
    ).toBeNull();
  });

  it('splits missing and invalid packages out of the judged rows', () => {
    const s = status([]);
    s.unavailable = [
      {
        widget: 'fig-part',
        path: 'runtimes/stl-viewer@1',
        error: 'runtime stl-viewer@1: not installed in this project',
      },
      {
        widget: 'fig-heat',
        path: 'runtimes/heatmap@1',
        error: 'runtime heatmap@1: contract must be 1, found 2',
      },
      { widget: 'sim', path: 'figs/sim', error: 'unreadable' },
    ];
    const rows = runtimeUnavailable(s);
    expect(rows).toHaveLength(2);
    expect(rows[0]).toMatchObject({ ref: 'stl-viewer@1', notInstalled: true });
    expect(rows[1]).toMatchObject({ ref: 'heatmap@1', notInstalled: false });
  });
});

describe('export gate', () => {
  it('asks about unknown and changed runtimes, never denied or missing ones', () => {
    const s = status([
      RUNTIME_REQ(),
      RUNTIME_REQ({ cause: 'changed_since_approval', approvedDigest: 'a' }),
      RUNTIME_REQ({ cause: 'revoked' }),
    ]);
    s.unavailable = [{ widget: 'fig-x', path: 'runtimes/gone@1', error: 'not installed' }];
    const pending = pendingExportRuntimes(s);
    expect(pending.map((p) => p.cause)).toEqual(['never_approved', 'changed_since_approval']);
  });

  it('words the pop-up per the contract', () => {
    const p = promptFromRuntime(RUNTIME_REQ() as ApprovalRequired, 'main.tex');
    expect(runtimePromptTitle(p)).toBe('Allow runtime \u201cSTL viewer\u201d (stl-viewer@1)?');
    expect(runtimePromptBody(p)).toContain('1 widget in main.tex uses this runtime');
    expect(runtimePromptBody(p)).toContain('Allow only code you have reviewed or trust');
  });
});

describe('runtime prompt queue', () => {
  const prompt = promptFromRuntime(RUNTIME_REQ() as ApprovalRequired, 'main.tex');

  it('allows the shown digest, then drains', async () => {
    const decide = vi.fn(async () => RUNTIME_REQ());
    const q = createRuntimePromptQueue({ decideRuntime: decide }, 'r', 'main.tex');
    q.enqueue(prompt);
    const drained = q.drain();
    await q.allow();
    await drained;
    expect(decide).toHaveBeenCalledWith({
      rootId: 'r',
      mainRel: 'main.tex',
      runtime: 'stl-viewer@1',
      digest: prompt.digest,
      decision: 'allowed',
    });
    expect(q.current()).toBeNull();
  });

  it('denies, and not-now writes nothing', async () => {
    const decide = vi.fn(async () => RUNTIME_REQ());
    const q = createRuntimePromptQueue({ decideRuntime: decide }, 'r', 'main.tex');
    q.enqueue(prompt);
    await q.deny();
    expect(decide).toHaveBeenCalledWith(expect.objectContaining({ decision: 'denied' }));
    q.enqueue(prompt);
    await q.notNow();
    expect(decide).toHaveBeenCalledTimes(1);
    expect(q.current()).toBeNull();
  });

  it('reports a refused decision and still drains so the export runs', async () => {
    const q = createRuntimePromptQueue(
      { decideRuntime: vi.fn().mockRejectedValue(new Error('changed since review')) },
      'r',
      'm',
    );
    q.enqueue(prompt);
    const drained = q.drain();
    await q.allow();
    await drained;
    expect(q.failure()).toContain('changed since review');
    expect(q.current()).toBeNull();
  });
});

describe('runtime model', () => {
  function fakeIo(over: Partial<WidgetsIo> = {}) {
    let current = status([RUNTIME_REQ()]);
    const io = {
      status: vi.fn(async () => current),
      reviewRuntime: vi.fn(async () => ({
        status: current.runtimes[0],
        files: [
          {
            path: 'index.html',
            change: 'modified' as const,
            before: 'a',
            after: 'b',
            binary: false,
          },
        ],
      })),
      decideRuntime: vi.fn(async () => current.runtimes[0]),
      ...over,
    } as unknown as WidgetsIo;
    return { io, set: (s: WidgetsStatus) => (current = s) };
  }

  it('decides the digest that was reviewed, then refreshes', async () => {
    const f = fakeIo();
    const m = createRuntimeModel(f.io, 'r', 'main.tex');
    await m.refresh();
    await m.loadReview('stl-viewer@1');
    expect(m.get().reviews['stl-viewer@1'].files).toHaveLength(1);
    await m.allow('stl-viewer@1');
    expect(f.io.decideRuntime).toHaveBeenCalledWith({
      rootId: 'r',
      mainRel: 'main.tex',
      runtime: 'stl-viewer@1',
      digest: 'e'.repeat(64),
      decision: 'allowed',
    });
    expect(f.io.status).toHaveBeenCalledTimes(2);
  });

  it('shows the refusal and drops the stale review when the package changed', async () => {
    const f = fakeIo({
      decideRuntime: vi.fn().mockRejectedValue(new Error('changed since review')),
    });
    const m = createRuntimeModel(f.io, 'r', 'main.tex');
    await m.refresh();
    await m.loadReview('stl-viewer@1');
    await m.deny('stl-viewer@1');
    expect(m.get().notice).toContain('Not denied: changed since review');
    expect(m.get().reviews['stl-viewer@1']).toBeUndefined();
  });
});

describe('runtime bus events', () => {
  const ev = (event: BusEvent['event']): BusEvent => ({
    scope: 'app',
    kind: 'info',
    actor: 'agent',
    message: '',
    at: 1,
    event,
  });
  const needed = (rootId = 'r') =>
    ev({
      action: 'runtime.approval-required',
      rootId,
      runtime: 'stl-viewer@1',
      digest: 'e'.repeat(64),
      cause: 'never_approved',
      widgets: ['fig-part'],
    } as unknown as BusEvent['event']);

  it('matches the approval event for this project only', () => {
    expect(runtimeApprovalRef(needed(), 'r')).toEqual({
      runtime: 'stl-viewer@1',
      digest: 'e'.repeat(64),
    });
    expect(runtimeApprovalRef(needed('other'), 'r')).toBeNull();
  });

  it('enriches the event into a full pop-up from the status', async () => {
    const queued: ReturnType<typeof promptFromRuntime>[] = [];
    await offerRuntimeEvent(
      { status: async () => status([RUNTIME_REQ()]) },
      (p) => queued.push(p),
      needed(),
      'r',
      'main.tex',
    );
    expect(queued).toHaveLength(1);
    expect(queued[0].title).toBe('STL viewer');
    expect(queued[0].version).toBe('1.2.0');
  });

  it('flags a decision on this project as a panel refresh', () => {
    expect(
      runtimeEventFor(
        ev({
          action: 'runtime.decided',
          rootId: 'r',
          runtime: 'stl-viewer@1',
          digest: 'e'.repeat(64),
          decision: 'allowed',
        } as unknown as BusEvent['event']),
        'r',
      ),
    ).toBe(true);
    expect(
      runtimeEventFor(
        ev({
          action: 'widget.approved',
          rootId: 'r',
          path: 'p',
          widget: 'w',
          digest: 'd',
        }),
        'r',
      ),
    ).toBe(false);
  });
});
