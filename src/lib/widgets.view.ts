// widgets.view.ts — view model for the Widgets panel and the approval
// prompt: row states, wording, origins, and the read-only source diff.
// Pure functions, no IO and no React, so what the panel tells the user about
// a widget's code is pinned headlessly.

import type {
  ApprovalRequired,
  ReviewFile,
  VendoredId,
  WidgetApprovalStatus,
  WidgetCsp,
  WidgetsStatus,
} from './generated/api';
import type { WidgetApprovalCause } from './generated/events';

/** Menu label and panel title: the MCP's userAction names `View > Widgets`. */
export const WIDGETS_PANEL_TITLE = 'Widgets';
/** Where the user is told to go; core's `PANEL` constant says the same. */
export const WIDGETS_PANEL_PATH = `View > ${WIDGETS_PANEL_TITLE}`;

/** Shown beside the auto-approval toggle, always. */
export const AUTO_APPROVE_WARNING =
  'Agents can then change widget code that runs when the app renders it; origin changes still need approval.';

export const WIDGETS_EMPTY = 'This project has no HTML widgets that need approval.';
export const WIDGETS_NEEDS_MAIN = 'Choose a main file and compile it to list its widgets.';

export type RowState = 'approved' | 'changed' | 'pending' | 'revoked';

export const STATE_LABEL: Record<RowState, string> = {
  approved: 'approved',
  changed: 'changed since approval',
  pending: 'pending',
  revoked: 'revoked',
};

const CAUSE_STATE: Record<WidgetApprovalCause, RowState> = {
  never_approved: 'pending',
  changed_since_approval: 'changed',
  declared_origins_changed: 'changed',
  revoked: 'revoked',
  license_or_vendored_changed: 'changed',
};

const CAUSE_DETAIL: Record<WidgetApprovalCause, string> = {
  never_approved: 'You have not approved this widget yet.',
  changed_since_approval: 'Its files changed after you approved it.',
  declared_origins_changed: 'It now declares origins your approval did not cover.',
  revoked: 'You revoked it; auto-approval does not bring it back.',
  license_or_vendored_changed: 'Its licence or vendored libraries changed after you allowed it.',
};

export function causeDetail(c: WidgetApprovalCause): string {
  return CAUSE_DETAIL[c];
}

export function stateOf(s: WidgetApprovalStatus): RowState {
  return s.status === 'approved' ? 'approved' : CAUSE_STATE[s.cause];
}

/** Declared origins as `directive: a, b` strings; empty when none. */
export function originLines(csp: WidgetCsp): string[] {
  const out: string[] = [];
  if (csp.connectDomains.length) out.push(`connect: ${csp.connectDomains.join(', ')}`);
  if (csp.resourceDomains.length) out.push(`resources: ${csp.resourceDomains.join(', ')}`);
  if (csp.frameDomains.length) out.push(`frames: ${csp.frameDomains.join(', ')}`);
  return out;
}

export interface WidgetRow {
  /** The widget id, as the document names it. */
  id: string;
  /** Widget type; only html widgets are gated. */
  type: 'html';
  path: string;
  state: RowState;
  stateLabel: string;
  /** Why it is in this state, in a sentence; null when approved. */
  detail: string | null;
  /** Approved by the app's auto mode rather than by the user. */
  viaAuto: boolean;
  origins: string[];
  /** Origins the last approval covered, when they differ from the declared. */
  approvedOrigins: string[] | null;
  /** The digest the user would approve: what core last hashed. */
  digest: string;
  canApprove: boolean;
  canRevoke: boolean;
  /** There is an earlier approved version to diff against. */
  hasApproved: boolean;
}

export function buildRow(s: WidgetApprovalStatus): WidgetRow {
  const state = stateOf(s);
  const origins = originLines(s.declaredOrigins);
  const approvedOrigins =
    s.status === 'approval_required' && s.approvedOrigins ? originLines(s.approvedOrigins) : null;
  return {
    id: s.widget,
    type: 'html',
    path: s.path,
    state,
    stateLabel: STATE_LABEL[state],
    detail: s.status === 'approved' ? null : causeDetail(s.cause),
    viaAuto: s.status === 'approved' && s.via === 'auto',
    origins,
    approvedOrigins:
      approvedOrigins && approvedOrigins.join('|') !== origins.join('|') ? approvedOrigins : null,
    digest: s.digest,
    canApprove: state !== 'approved',
    canRevoke: state === 'approved' || state === 'changed',
    hasApproved: s.status === 'approval_required' && s.approvedDigest != null,
  };
}

export function buildRows(status: WidgetsStatus): WidgetRow[] {
  return status.widgets.map(buildRow);
}

/** Held html widgets: every listing the store has not approved. */
export function pendingWidgets(status: WidgetsStatus | null): WidgetRow[] {
  if (!status) return [];
  return status.widgets.filter((s) => s.status === 'approval_required').map(buildRow);
}

/** Short digest for display; the full one is the title. */
export function shortDigest(d: string): string {
  return d.length > 12 ? d.slice(0, 12) : d;
}

// ---- Source diff -----------------------------------------------------

export interface DiffLine {
  kind: 'same' | 'add' | 'del' | 'gap';
  text: string;
  /** 1-based line numbers in the approved and current text. */
  oldNo?: number;
  newNo?: number;
}

/** Above this many cell comparisons a file diffs as one block replaced. */
const LCS_LIMIT = 4_000_000;
const CONTEXT = 3;

function splitLines(t: string): string[] {
  if (t === '') return [];
  const lines = t.split('\n');
  if (lines[lines.length - 1] === '') lines.pop();
  return lines;
}

/** Line diff of two texts, unchanged runs folded to `gap` rows. */
export function diffLines(before: string, after: string): DiffLine[] {
  const a = splitLines(before);
  const b = splitLines(after);
  const ops: DiffLine[] = [];
  let lo = 0;
  while (lo < a.length && lo < b.length && a[lo] === b[lo]) lo++;
  let ha = a.length;
  let hb = b.length;
  while (ha > lo && hb > lo && a[ha - 1] === b[hb - 1]) {
    ha--;
    hb--;
  }
  for (let i = 0; i < lo; i++) ops.push({ kind: 'same', text: a[i], oldNo: i + 1, newNo: i + 1 });
  const n = ha - lo;
  const m = hb - lo;
  if (n * m > LCS_LIMIT) {
    for (let i = lo; i < ha; i++) ops.push({ kind: 'del', text: a[i], oldNo: i + 1 });
    for (let j = lo; j < hb; j++) ops.push({ kind: 'add', text: b[j], newNo: j + 1 });
  } else {
    const w = m + 1;
    const t = new Uint32Array((n + 1) * w);
    for (let i = n - 1; i >= 0; i--)
      for (let j = m - 1; j >= 0; j--)
        t[i * w + j] =
          a[lo + i] === b[lo + j]
            ? t[(i + 1) * w + j + 1] + 1
            : Math.max(t[(i + 1) * w + j], t[i * w + j + 1]);
    let i = 0;
    let j = 0;
    while (i < n && j < m) {
      if (a[lo + i] === b[lo + j]) {
        ops.push({ kind: 'same', text: a[lo + i], oldNo: lo + i + 1, newNo: lo + j + 1 });
        i++;
        j++;
      } else if (t[(i + 1) * w + j] >= t[i * w + j + 1]) {
        ops.push({ kind: 'del', text: a[lo + i], oldNo: lo + i + 1 });
        i++;
      } else {
        ops.push({ kind: 'add', text: b[lo + j], newNo: lo + j + 1 });
        j++;
      }
    }
    for (; i < n; i++) ops.push({ kind: 'del', text: a[lo + i], oldNo: lo + i + 1 });
    for (; j < m; j++) ops.push({ kind: 'add', text: b[lo + j], newNo: lo + j + 1 });
  }
  for (let i = ha; i < a.length; i++)
    ops.push({ kind: 'same', text: a[i], oldNo: i + 1, newNo: hb + (i - ha) + 1 });
  return fold(ops);
}

function fold(ops: DiffLine[]): DiffLine[] {
  const keep = new Array<boolean>(ops.length).fill(false);
  ops.forEach((o, i) => {
    if (o.kind === 'same') return;
    for (let k = Math.max(0, i - CONTEXT); k <= Math.min(ops.length - 1, i + CONTEXT); k++)
      keep[k] = true;
  });
  const out: DiffLine[] = [];
  let skipped = 0;
  ops.forEach((o, i) => {
    if (keep[i]) {
      if (skipped)
        out.push({ kind: 'gap', text: `${skipped} unchanged line${skipped === 1 ? '' : 's'}` });
      skipped = 0;
      out.push(o);
    } else skipped++;
  });
  if (skipped)
    out.push({ kind: 'gap', text: `${skipped} unchanged line${skipped === 1 ? '' : 's'}` });
  return out;
}

export interface FileReviewView {
  path: string;
  change: ReviewFile['change'];
  /** Why there is no text to show, or null when `lines` is the diff. */
  note: string | null;
  lines: DiffLine[];
}

/** One file of a widget review as the panel shows it; unchanged files are left out. */
export function reviewFiles(files: readonly ReviewFile[]): FileReviewView[] {
  return files
    .filter((f) => f.change !== 'unchanged')
    .map((f) => {
      if (f.binary || (f.before === undefined && f.after === undefined)) {
        return {
          path: f.path,
          change: f.change,
          note: `${f.change} (binary or large file: no text shown)`,
          lines: [],
        };
      }
      return {
        path: f.path,
        change: f.change,
        note: null,
        lines: diffLines(f.before ?? '', f.after ?? ''),
      };
    });
}

// ---- The approval prompt --------------------------------------------

/** What the poster prompt asks the user about. */
export interface ApprovalPrompt {
  /** `path@digest`: one prompt per widget version. */
  key: string;
  widget: string;
  path: string;
  digest: string;
  cause: WidgetApprovalCause;
  origins: string[];
}

export function promptFrom(r: ApprovalRequired): ApprovalPrompt {
  return {
    key: `${r.path}@${r.digest}`,
    widget: r.widget,
    path: r.path,
    digest: r.digest,
    cause: r.cause,
    origins: originLines(r.declaredOrigins),
  };
}

// ---- Custom runtimes --------------------------------------------------
// Panel sections and the export pop-up read the `runtimes` entries of the
// status (one per `<name>@<major>` the document uses) plus the
// `runtimes/<ref>` rows of `unavailable` (missing or invalid packages).

export type RuntimeSection = 'needs' | 'allowed' | 'allowed-auto' | 'changed' | 'denied';

export const RUNTIME_SECTION_LABEL: Record<RuntimeSection, string> = {
  needs: 'Needs approval',
  allowed: 'Allowed',
  'allowed-auto': 'Allowed (auto)',
  changed: 'Changed',
  denied: 'Denied',
};

export interface RuntimeRow {
  /** `<name>@<major>`, the key the user allows or denies. */
  ref: string;
  title: string;
  version: string;
  license: string;
  digest: string;
  /** `runtimes/<ref>` inside the project. */
  path: string;
  widgets: string[];
  warnings: string[];
  webgl: boolean;
  vendored: VendoredId[];
  section: RuntimeSection;
  sectionLabel: string;
  /** Why it waits, in a sentence; null when allowed. */
  detail: string | null;
  /** Allowed by auto-approval after a change the user had allowed before. */
  viaAuto: boolean;
  canAllow: boolean;
  canDeny: boolean;
  /** An earlier allowed version exists to diff against. */
  hasApproved: boolean;
}

function runtimeSectionOf(s: WidgetApprovalStatus): RuntimeSection {
  if (s.status === 'approved') return s.via === 'auto' ? 'allowed-auto' : 'allowed';
  if (s.cause === 'revoked') return 'denied';
  if (s.cause === 'never_approved') return 'needs';
  return 'changed';
}

export function buildRuntimeRow(s: WidgetApprovalStatus): RuntimeRow | null {
  if (s.kind !== 'custom_runtime') return null;
  const info = s.runtime;
  const ref = info?.reference ?? s.path.replace(/^runtimes\//, '');
  const section = runtimeSectionOf(s);
  return {
    ref,
    title: info?.title ?? ref,
    version: info?.version ?? '',
    license: info?.license ?? '',
    digest: s.digest,
    path: s.path,
    widgets: info?.widgets ?? [s.widget],
    warnings: info?.warnings ?? [],
    webgl: info?.webgl ?? false,
    vendored: info?.vendored ?? [],
    section,
    sectionLabel: RUNTIME_SECTION_LABEL[section],
    detail: s.status === 'approved' ? null : causeDetail(s.cause),
    viaAuto: section === 'allowed-auto',
    canAllow: section === 'needs' || section === 'changed' || section === 'denied',
    canDeny: section === 'needs' || section === 'allowed' || section === 'changed',
    hasApproved: s.status === 'approval_required' && s.approvedDigest != null,
  };
}

export function buildRuntimeRows(status: WidgetsStatus): RuntimeRow[] {
  const out: RuntimeRow[] = [];
  for (const s of status.runtimes) {
    const row = buildRuntimeRow(s);
    if (row) out.push(row);
  }
  return out;
}

/** Held custom runtimes: every package the store has not allowed. */
export function pendingRuntimes(status: WidgetsStatus | null): RuntimeRow[] {
  if (!status) return [];
  const out: RuntimeRow[] = [];
  for (const s of status.runtimes) {
    if (s.status !== 'approval_required') continue;
    const row = buildRuntimeRow(s);
    if (row) out.push(row);
  }
  return out;
}

/** One sentence naming the held widgets and runtimes, or null when none. */
export function approvalBannerText(
  widgets: readonly Pick<WidgetRow, 'id'>[],
  runtimes: readonly Pick<RuntimeRow, 'ref'>[],
): string | null {
  const names = [...widgets.map((w) => w.id), ...runtimes.map((r) => r.ref)];
  if (names.length === 0) return null;
  const who =
    names.length === 1
      ? names[0]
      : names.length === 2
        ? `${names[0]} and ${names[1]}`
        : `${names.slice(0, -1).join(', ')} and ${names[names.length - 1]}`;
  const verb = names.length === 1 ? 'is' : 'are';
  const poster = names.length === 1 ? 'its poster' : 'their posters';
  return `${who} ${verb} waiting for approval. The article shows ${poster} instead.`;
}

export interface RuntimeUnavailable {
  ref: string;
  widget: string;
  error: string;
  /** No `runtimes/<ref>/` copy in the project, so nothing to review. */
  notInstalled: boolean;
}

/** Missing or invalid packages: never approvable, never asked about. */
export function runtimeUnavailable(status: WidgetsStatus): RuntimeUnavailable[] {
  const out: RuntimeUnavailable[] = [];
  for (const u of status.unavailable) {
    if (!u.path.startsWith('runtimes/')) continue;
    out.push({
      ref: u.path.slice('runtimes/'.length),
      widget: u.widget,
      error: u.error,
      notInstalled: /not installed/i.test(u.error),
    });
  }
  return out;
}

// ---- The export pop-up -------------------------------------------------
