// widgets.view.ts — view model for the Widgets panel and the approval
// prompt: row states, wording, origins, and the read-only source diff.
// Pure functions, no IO and no React, so what the panel tells the user about
// a widget's code is pinned headlessly.

import type {
  ApprovalRequired,
  ReviewFile,
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
};

const CAUSE_DETAIL: Record<WidgetApprovalCause, string> = {
  never_approved: 'You have not approved this widget yet.',
  changed_since_approval: 'Its files changed after you approved it.',
  declared_origins_changed: 'It now declares origins your approval did not cover.',
  revoked: 'You revoked it; auto-approval does not bring it back.',
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
