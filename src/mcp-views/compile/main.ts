// The compile-dashboard MCP App View: a `compile_run` job as a live panel,
// with its status, phase and downloads, log tail, what it lacked, and a
// cancel button. One self-contained page, no framework. It only reads
// through tools a text-only host sees too (compile_poll, compile_cancel,
// diagnostics, offline_readiness), so both get the same facts.
import {
  App,
  applyDocumentTheme,
  applyHostFonts,
  applyHostStyleVariables,
  type McpUiHostContext,
} from '@modelcontextprotocol/ext-apps';
import type { CallToolResult } from '@modelcontextprotocol/client';

type Status = 'running' | 'success' | 'failed' | 'timed-out' | 'cancelled';
interface Missing {
  file?: string;
  reason: string;
}
interface Progress {
  phase: string | null;
  detail?: string;
  fetched: number;
  fetch_failed: number;
}
interface Poll {
  status: Status;
  log: string;
  lines: string[];
  missing: Missing | null;
  progress?: Progress;
}
interface Diagnostic {
  path?: string;
  line: number;
  message: string;
  severity: 'error' | 'warning';
}
interface Readiness {
  state: string;
  needs: string[];
}

const POLL_MS = 1000;
const TAIL_LINES = 40;
const MAX_DIAGNOSTICS = 8;

const STATUS_TEXT: Record<Status, string> = {
  running: 'Running',
  success: 'Compiled',
  failed: 'Failed',
  'timed-out': 'Timed out',
  cancelled: 'Cancelled',
};
const PHASE_TEXT: Record<string, string> = {
  connect: 'Trying the network for files the cache lacks',
  'first-compile': 'First compile: downloading TeX support files',
  format: 'Building the LaTeX format',
  tex: 'Running TeX',
  bibliography: 'Building the bibliography',
  xdvipdfmx: 'Converting to PDF',
  writing: 'Writing',
};
const MISSING_TEXT: Record<string, string> = {
  'not-cached': 'is in the TeX bundle but not cached, and this compile could not download it',
  'fetch-failed': 'could not be downloaded (offline, or the server is down)',
  'not-in-bundle': 'is not in the TeX bundle, so downloading cannot help',
  'cache-empty': 'No TeX support files are cached yet, and there is no network',
  'bundle-unreachable': 'The TeX bundle is not cached and could not be reached',
  'bundle-invalid': 'The bundle location is not a TeX bundle',
  'bundle-changed': 'The pinned TeX bundle changed on the server',
  'system-font': 'is a system font that is not installed',
  'external-tool': 'is a program the engine runs, and it is not installed',
  'shell-escape-required': 'needs shell escape, which compiles never enable',
};
const READINESS_TEXT: Record<string, string> = {
  'needs-network': 'Offline: needs network for',
  'needs-tool': 'Offline: needs an installed tool:',
  'needs-font': 'Offline: needs an installed font:',
  blocked: 'Offline: blocked',
};

const $ = <T extends HTMLElement>(id: string): T => document.getElementById(id) as T;
const [bar, statusTag, where, time, phase, msg, findings, log] = [
  'bar',
  'status',
  'where',
  'time',
  'phase',
  'msg',
  'findings',
  'log',
].map((id) => $(id));
const cancel = $<HTMLButtonElement>('cancel');

const app = new App({ name: 'maleficium-compile', version: '1.0.0' });

let rootId: string | null = null;
let job: { id: string; main: string } | null = null;
let status: Status = 'running';
let startedAt = 0;
let timer: ReturnType<typeof setTimeout> | undefined;
let inFlight = false;

function say(text: string): void {
  msg.textContent = text;
  msg.hidden = false;
}

function textOf(r: CallToolResult): string {
  const t = r.content.find((c) => c.type === 'text');
  return t && t.type === 'text' ? t.text : 'The tool failed.';
}

const done = (): boolean => status !== 'running';

function setStatus(s: Status): void {
  status = s;
  statusTag.dataset.status = s;
  statusTag.textContent = STATUS_TEXT[s] ?? s;
  cancel.hidden = done();
  document.body.dataset.status = s;
}

function tick(): void {
  if (!startedAt) return;
  time.textContent = `${((Date.now() - startedAt) / 1000).toFixed(1)} s`;
}

function phaseText(p: Progress | undefined): string {
  if (!p) return '';
  const parts: string[] = [];
  if (p.phase && status === 'running') {
    const base = PHASE_TEXT[p.phase] ?? p.phase;
    parts.push(p.detail ? `${base} (${p.detail})` : base);
  }
  if (p.fetched) parts.push(`${p.fetched} file${p.fetched === 1 ? '' : 's'} downloaded`);
  if (p.fetch_failed)
    parts.push(`${p.fetch_failed} download${p.fetch_failed === 1 ? '' : 's'} failed`);
  return parts.join(' · ');
}

function finding(text: string, kind: 'error' | 'warning' | 'ok', at?: string): HTMLLIElement {
  const li = document.createElement('li');
  li.className = kind;
  if (at) {
    const span = document.createElement('span');
    span.className = 'at';
    span.textContent = at;
    li.append(span);
  }
  li.append(text);
  return li;
}

function missingText(m: Missing): string {
  const why = MISSING_TEXT[m.reason] ?? m.reason;
  return m.file ? `${m.file} ${why}` : why;
}

function showPoll(p: Poll): void {
  setStatus(p.status);
  tick();
  const ph = phaseText(p.progress);
  phase.textContent = ph;
  phase.hidden = !ph;
  if (p.lines.length) {
    log.textContent = p.lines.join('\n');
    log.hidden = false;
    log.scrollTop = log.scrollHeight;
  }
  msg.hidden = true;
  if (p.status === 'running') return;
  // A finished run: its own account first, then the missing dependency.
  const items: HTMLLIElement[] = [];
  if (p.missing) items.push(finding(missingText(p.missing), 'error'));
  if (p.status === 'cancelled') say('The compile was cancelled; nothing was written.');
  else if (p.status === 'success') say('The PDF is up to date.');
  else if (p.log) say(p.log);
  findings.replaceChildren(...items);
  findings.hidden = !items.length;
}

// Once a run settles: the source lines it failed on, and whether the
// project now compiles offline.
async function settle(p: Poll): Promise<void> {
  if (!rootId || !job) return;
  const extra: HTMLLIElement[] = [];
  if (p.status === 'failed' || p.status === 'timed-out') {
    try {
      const r = await app.callServerTool({
        name: 'diagnostics',
        arguments: { root_id: rootId, main_rel: job.main, max: MAX_DIAGNOSTICS },
      });
      const ds = (r.structuredContent as { diagnostics?: Diagnostic[] } | undefined)?.diagnostics;
      for (const d of (ds ?? []).filter((d) => d.severity === 'error')) {
        extra.push(finding(d.message, 'error', d.path ? `${d.path}:${d.line}` : undefined));
      }
    } catch {
      /* the log already says why it failed */
    }
  }
  if (p.status !== 'cancelled') {
    try {
      const r = await app.callServerTool({
        name: 'offline_readiness',
        arguments: { root_id: rootId },
      });
      const o = r.structuredContent as Readiness | undefined;
      if (o && o.state === 'ready') extra.push(finding('Compiles offline', 'ok'));
      else if (o && READINESS_TEXT[o.state])
        extra.push(finding(`${READINESS_TEXT[o.state]} ${o.needs.join(', ')}`.trim(), 'warning'));
    } catch {
      /* readiness is a bonus */
    }
  }
  if (extra.length) {
    findings.append(...extra);
    findings.hidden = false;
  }
}

function schedule(ms = POLL_MS): void {
  clearTimeout(timer);
  if (!done()) timer = setTimeout(() => void poll(), ms);
}

// Polled only while the View is visible and the job runs; pushed events
// replace this once the core compile service has an event sink.
async function poll(): Promise<void> {
  if (!job || done() || inFlight || document.visibilityState !== 'visible') return;
  inFlight = true;
  try {
    const r = await app.callServerTool({
      name: 'compile_poll',
      arguments: { job_id: job.id, tail_lines: TAIL_LINES },
    });
    if (r.isError) {
      // The server no longer knows the job (restarted): nothing to follow.
      setStatus('failed');
      say(`This compile is no longer tracked: ${textOf(r)}`);
      return;
    }
    const p = r.structuredContent as unknown as Poll;
    showPoll(p);
    if (done()) await settle(p);
  } catch {
    /* host busy or closing: try again next tick */
  } finally {
    inFlight = false;
    schedule();
  }
}

document.addEventListener('visibilitychange', () => void poll());

cancel.onclick = async () => {
  if (!job || done()) return;
  cancel.disabled = true;
  cancel.textContent = 'Cancelling…';
  try {
    const r = await app.callServerTool({ name: 'compile_cancel', arguments: { job_id: job.id } });
    // "nothing to cancel": the run just finished, and the next poll says how.
    if (r.isError) say(textOf(r));
  } catch (e) {
    say(`Could not cancel: ${String(e)}`);
  } finally {
    cancel.disabled = false;
    cancel.textContent = 'Cancel';
    schedule(0);
  }
};

app.ontoolinput = (params) => {
  const a = params.arguments as { root_id?: string } | undefined;
  if (a?.root_id) rootId = a.root_id;
};

app.ontoolresult = (result) => {
  if (result.isError) {
    bar.hidden = true;
    return say(textOf(result as CallToolResult));
  }
  const r = result.structuredContent as { job_id?: string; main_rel?: string } | undefined;
  if (!r?.job_id) return say('The compile did not start.');
  job = { id: r.job_id, main: r.main_rel ?? '' };
  startedAt = Date.now();
  where.textContent = job.main;
  bar.hidden = false;
  setStatus('running');
  say('Starting…');
  void poll();
};

function adopt(ctx: McpUiHostContext | undefined): void {
  if (!ctx) return;
  if (ctx.theme) applyDocumentTheme(ctx.theme);
  if (ctx.styles?.variables) applyHostStyleVariables(ctx.styles.variables);
  if (ctx.styles?.css?.fonts) applyHostFonts(ctx.styles.css.fonts);
}
app.onhostcontextchanged = adopt;

app
  .connect()
  .then(() => adopt(app.getHostContext()))
  .catch((e) => say(`Not connected to a host: ${String(e)}`));
