import { describe, expect, it } from 'vitest';
import type {
  ApprovalRequired,
  ReviewFile,
  RuntimeInfo,
  WidgetApprovalStatus,
  WidgetsStatus,
} from './generated/api';
import {
  approvalBannerText,
  AUTO_APPROVE_WARNING,
  buildRow,
  diffLines,
  originLines,
  pendingRuntimes,
  pendingWidgets,
  promptFrom,
  reviewFiles,
  WIDGETS_PANEL_PATH,
} from './widgets.view';

const NO_ORIGINS = { connectDomains: [], resourceDomains: [], frameDomains: [] };

function required(over: Partial<ApprovalRequired> = {}): WidgetApprovalStatus {
  return {
    status: 'approval_required',
    kind: 'html_widget',
    widget: 'sim',
    path: 'figs/sim',
    digest: 'd'.repeat(64),
    cause: 'never_approved',
    declaredOrigins: NO_ORIGINS,
    autoApprove: false,
    panel: WIDGETS_PANEL_PATH,
    whatHappens: '',
    userAction: '',
    agentMustNot: [],
    message: '',
    ...over,
  };
}

describe('row state', () => {
  it('maps core causes onto the four states the panel shows', () => {
    expect(buildRow(required()).stateLabel).toBe('pending');
    expect(
      buildRow(required({ cause: 'changed_since_approval', approvedDigest: 'a' })).stateLabel,
    ).toBe('changed since approval');
    expect(buildRow(required({ cause: 'declared_origins_changed' })).state).toBe('changed');
    expect(buildRow(required({ cause: 'revoked' })).stateLabel).toBe('revoked');
    const ok = buildRow({
      status: 'approved',
      kind: 'html_widget',
      widget: 'sim',
      path: 'figs/sim',
      digest: 'x',
      via: 'user',
      declaredOrigins: NO_ORIGINS,
      autoApprove: false,
    });
    expect(ok.stateLabel).toBe('approved');
    expect(ok.detail).toBeNull();
  });

  it('enables Approve and Revoke only where they mean something', () => {
    const pending = buildRow(required());
    expect([pending.canApprove, pending.canRevoke]).toEqual([true, false]);
    const changed = buildRow(required({ cause: 'changed_since_approval', approvedDigest: 'a' }));
    expect([changed.canApprove, changed.canRevoke, changed.hasApproved]).toEqual([
      true,
      true,
      true,
    ]);
    const revoked = buildRow(required({ cause: 'revoked' }));
    expect([revoked.canApprove, revoked.canRevoke]).toEqual([true, false]);
  });

  it('marks auto approvals and shows widened origins next to the approved ones', () => {
    const auto = buildRow({
      status: 'approved',
      kind: 'html_widget',
      widget: 'w',
      path: 'p',
      digest: 'x',
      via: 'auto',
      declaredOrigins: NO_ORIGINS,
      autoApprove: true,
      approvedDigest: 'y',
    });
    expect(auto.viaAuto).toBe(true);
    const widened = buildRow(
      required({
        cause: 'declared_origins_changed',
        declaredOrigins: { ...NO_ORIGINS, connectDomains: ['https://a.example'] },
        approvedOrigins: NO_ORIGINS,
      }),
    );
    expect(widened.origins).toEqual(['connect: https://a.example']);
    expect(widened.approvedOrigins).toEqual([]);
    expect(originLines(NO_ORIGINS)).toEqual([]);
  });
});

describe('wording', () => {
  it('names the panel the MCP userAction points at and warns about auto mode', () => {
    expect(WIDGETS_PANEL_PATH).toBe('View > Widgets');
    expect(AUTO_APPROVE_WARNING).toContain('Agents can then change widget code');
    expect(AUTO_APPROVE_WARNING).toContain('origin changes still need approval');
  });
});

describe('diffLines', () => {
  it('shows the edited line against the approved text, with context folded', () => {
    const before = Array.from({ length: 20 }, (_, i) => `line ${i + 1}`).join('\n') + '\n';
    const after = before.replace('line 10', 'line 10 fetch(evil)');
    const d = diffLines(before, after);
    expect(d.filter((l) => l.kind === 'del').map((l) => l.text)).toEqual(['line 10']);
    expect(d.filter((l) => l.kind === 'add').map((l) => l.text)).toEqual(['line 10 fetch(evil)']);
    expect(d.filter((l) => l.kind === 'gap')).toHaveLength(2);
    expect(d.filter((l) => l.kind === 'same')).toHaveLength(6);
  });

  it('handles added and emptied text and identical input', () => {
    expect(diffLines('', 'a\nb\n').map((l) => l.kind)).toEqual(['add', 'add']);
    expect(diffLines('a\n', '').map((l) => l.kind)).toEqual(['del']);
    expect(diffLines('a\nb\n', 'a\nb\n')).toEqual([{ kind: 'gap', text: '2 unchanged lines' }]);
  });

  it('keeps line numbers for both sides', () => {
    const d = diffLines('a\nb\nc\n', 'a\nx\nb\nc\n');
    expect(d.find((l) => l.kind === 'add')).toMatchObject({ text: 'x', newNo: 2 });
    expect(d.find((l) => l.text === 'c')).toMatchObject({ oldNo: 3, newNo: 4 });
  });
});

describe('reviewFiles', () => {
  it('drops unchanged files and explains files shown without text', () => {
    const files: ReviewFile[] = [
      { path: 'a.js', change: 'unchanged', before: 'x', after: 'x', binary: false },
      { path: 'b.js', change: 'modified', before: 'x\n', after: 'y\n', binary: false },
      { path: 'c.png', change: 'added', binary: true },
    ];
    const v = reviewFiles(files);
    expect(v.map((f) => f.path)).toEqual(['b.js', 'c.png']);
    expect(v[0].lines.map((l) => l.kind)).toEqual(['del', 'add']);
    expect(v[1].note).toContain('no text shown');
  });
});

describe('promptFrom', () => {
  it('keys one prompt per widget version', () => {
    const r = required() as ApprovalRequired & { status: string };
    expect(promptFrom(r).key).toBe(`figs/sim@${'d'.repeat(64)}`);
    expect(promptFrom(r).cause).toBe('never_approved');
  });
});

function runtimeInfo(over: Partial<RuntimeInfo> = {}): RuntimeInfo {
  return {
    reference: 'coffee-mug@1',
    version: '1.0.0',
    title: 'Coffee mug',
    description: 'Renders the mug',
    authors: ['Example'],
    license: 'MIT',
    vendored: [],
    webgl: false,
    widgets: ['fig-mug'],
    warnings: [],
    ...over,
  };
}

function heldWidget(over: Partial<ApprovalRequired> = {}): WidgetApprovalStatus {
  return required({ widget: 'fig-mug', path: 'scratch/coffee', ...over });
}

function heldRuntime(over: Partial<ApprovalRequired> = {}): WidgetApprovalStatus {
  return {
    status: 'approval_required',
    kind: 'custom_runtime',
    widget: 'fig-mug',
    path: 'runtimes/coffee-mug@1',
    digest: 'e'.repeat(64),
    cause: 'never_approved',
    declaredOrigins: NO_ORIGINS,
    autoApprove: false,
    panel: WIDGETS_PANEL_PATH,
    whatHappens: '',
    userAction: '',
    agentMustNot: [],
    message: '',
    runtime: runtimeInfo(),
    ...over,
  } as WidgetApprovalStatus;
}

function listing(
  widgets: WidgetApprovalStatus[] = [],
  runtimes: WidgetApprovalStatus[] = [],
): WidgetsStatus {
  return {
    autoApprove: false,
    pending: widgets.length + runtimes.length,
    widgets,
    runtimes,
    unavailable: [],
    exempt: [],
  };
}

describe('article approval banner', () => {
  it('lists held widgets and runtimes, skipping approved ones', () => {
    const ok: WidgetApprovalStatus = {
      status: 'approved',
      kind: 'html_widget',
      widget: 'fig-ok',
      path: 'figs/ok',
      digest: 'x',
      via: 'user',
      declaredOrigins: NO_ORIGINS,
      autoApprove: false,
    };
    const s = listing([heldWidget(), ok], [heldRuntime()]);
    expect(pendingWidgets(s).map((w) => w.id)).toEqual(['fig-mug']);
    expect(pendingRuntimes(s).map((r) => r.ref)).toEqual(['coffee-mug@1']);
  });

  it('is empty without a listing or when nothing is held', () => {
    expect(pendingWidgets(null)).toEqual([]);
    expect(pendingRuntimes(null)).toEqual([]);
    expect(pendingWidgets(listing())).toEqual([]);
    expect(pendingRuntimes(listing())).toEqual([]);
    expect(approvalBannerText([], [])).toBeNull();
  });

  it('names the held widget and runtime in one sentence', () => {
    const s = listing([heldWidget()], [heldRuntime()]);
    expect(approvalBannerText(pendingWidgets(s), pendingRuntimes(s))).toBe(
      'fig-mug and coffee-mug@1 are waiting for approval. The article shows their posters instead.',
    );
  });

  it('uses the singular form for one held item', () => {
    expect(approvalBannerText([{ id: 'fig-mug' }], [])).toBe(
      'fig-mug is waiting for approval. The article shows its poster instead.',
    );
    expect(approvalBannerText([], [{ ref: 'coffee-mug@1' }])).toBe(
      'coffee-mug@1 is waiting for approval. The article shows its poster instead.',
    );
  });

  it('joins three held items with commas and and', () => {
    expect(approvalBannerText([{ id: 'a' }, { id: 'b' }], [{ ref: 'c@1' }])).toBe(
      'a, b and c@1 are waiting for approval. The article shows their posters instead.',
    );
  });
});
