import { describe, expect, it, vi } from 'vitest';
import type { ApprovalRequired, WidgetApprovalStatus, WidgetsStatus } from './generated/api';
import { createRuntimeModel, runtimeEventFor, type WidgetsIo } from './widgetApproval';
import type { BusEvent } from './generated/events';
import { buildRuntimeRow, buildRuntimeRows, runtimeUnavailable } from './widgets.view';

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
