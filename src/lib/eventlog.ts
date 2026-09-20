// eventlog.ts — the event bus, recorded app-locally as JSONL.
//
// One file, under the app-data dir, truncated when the run starts: it holds
// the current run and nothing else. Every line is one JSON object
// `{at, scope, kind, message, data?}`, and `data.action` names the fact so a
// reader matches on the payload rather than on prose. Retention is the
// newest MAX_LOG_EVENTS lines, each at most MAX_LINE_BYTES.
//
// Nothing is written to the project dir, and a write that fails is dropped
// rather than surfaced.
//
// Read the current run hands-free (Linux app-data home):
//   L=~/.local/share/com.ethan.tauri-app/maleficium-log/events.jsonl
//   cat "$L"                                   # the whole run
//   grep '"action":"compile.finish"' "$L"      # one fact
//   tail -f "$L"                               # live

import { fs } from './fs-provider';
import { appDataDir } from '@tauri-apps/api/path';
import { appEventLogDir, eventLogPath } from './paths';
import { emit, list, subscribe, type BusEvent } from './events';
import type { RecordOutcome } from './history';

/** Newest events kept on disk for one run; older lines are dropped. */
export const MAX_LOG_EVENTS = 2000;
/** Ceiling on one serialized line, newline included. */
export const MAX_LINE_BYTES = 2048;
/** Message text is cut to this many characters before serializing. */
export const MAX_MESSAGE_CHARS = 300;
/** Pending lines are written at this interval. */
const FLUSH_MS = 500;

/** Structured payload: `action` names the fact, the other fields are it. */
export interface EventData {
  action: string;
  [field: string]: unknown;
}

/** One line as it comes back off disk. */
export interface LoggedEvent {
  at: number;
  scope: string;
  kind: string;
  message: string;
  data?: EventData;
}

const enc = new TextEncoder();

function byteLength(s: string): number {
  return enc.encode(s).length;
}

function actionOf(data: unknown): string {
  const a = (data as EventData | undefined)?.action;
  return typeof a === 'string' ? a.slice(0, 64) : 'unknown';
}

/**
 * One JSONL line. A payload that would break the line ceiling is dropped
 * down to its action, so every line stays parseable.
 */
export function serializeEvent(e: BusEvent): string {
  const head = {
    at: e.at,
    scope: e.scope,
    kind: e.kind,
    message: e.message.slice(0, MAX_MESSAGE_CHARS),
  };
  let line = JSON.stringify(e.data === undefined ? head : { ...head, data: e.data });
  if (byteLength(line) + 1 > MAX_LINE_BYTES) {
    line = JSON.stringify({ ...head, data: { action: actionOf(e.data), dropped: true } });
  }
  return line + '\n';
}

/**
 * Newest-wins retention. Returns the lines the file must hold and whether it
 * has to be rewritten rather than appended to.
 */
export function retain(
  kept: readonly string[],
  incoming: readonly string[],
): { lines: string[]; rewrite: boolean } {
  const all = [...kept, ...incoming];
  if (all.length <= MAX_LOG_EVENTS) return { lines: all, rewrite: false };
  return { lines: all.slice(all.length - MAX_LOG_EVENTS), rewrite: true };
}

/** Parse JSONL text; a line that is not one event is skipped. */
export function parseEventLog(text: string): LoggedEvent[] {
  const out: LoggedEvent[] = [];
  for (const raw of text.split('\n')) {
    if (raw === '') continue;
    try {
      const v = JSON.parse(raw) as LoggedEvent;
      if (typeof v.at === 'number' && typeof v.message === 'string') out.push(v);
    } catch {
      /* a partial line is not an event */
    }
  }
  return out;
}

/** Events whose payload names this action, oldest first. */
export function eventsFor(log: readonly LoggedEvent[], action: string): LoggedEvent[] {
  return log.filter((e) => e.data?.action === action);
}

/** Payload for one snapshot attempt, from the store's own outcome. */
export function revisionRecordData(
  rel: string,
  outcome: RecordOutcome,
  revisions: number,
): EventData {
  return outcome.stored
    ? {
        action: 'revision.record',
        rel,
        stored: true,
        rev: outcome.rev,
        deduped: outcome.deduped,
        revisions,
      }
    : { action: 'revision.record', rel, stored: false, reason: outcome.reason, revisions };
}

/** Payload for one restore, carrying the size of the text put back. */
export function revisionRestoreData(rel: string, rev: string, chars: number): EventData {
  return { action: 'revision.restore', rel, rev, chars };
}

export interface EventLog {
  /** Write pending lines now. */
  flush(): Promise<void>;
  /** Stop recording; queued lines are written once more. */
  stop(): void;
}

/**
 * Record the bus to the app-local JSONL file for this run. Events already in
 * the in-memory buffer are included, and an unreachable store simply stops
 * the recording.
 */
export function startEventLog(): EventLog {
  let target: string | null = null;
  let recording = true;
  let flushing = false;
  const kept: string[] = [];
  let pending: string[] = list().map(serializeEvent);

  const unsub = subscribe((e) => {
    if (recording) pending.push(serializeEvent(e));
  });

  const flush = async (): Promise<void> => {
    if (flushing || target === null || pending.length === 0) return;
    flushing = true;
    const batch = pending;
    pending = [];
    const next = retain(kept, batch);
    kept.length = 0;
    kept.push(...next.lines);
    try {
      const body = next.rewrite ? next.lines.join('') : batch.join('');
      await fs().writeBytes(target, enc.encode(body), next.rewrite ? undefined : { append: true });
    } catch {
      /* the log is never the reason the app stops */
    }
    flushing = false;
  };

  const timer = setInterval(() => void flush(), FLUSH_MS);

  void (async () => {
    try {
      const base = await appDataDir();
      if (!recording) return;
      await fs().mkdir(appEventLogDir(base), { recursive: true });
      if (!recording) return;
      const path = eventLogPath(base);
      await fs().writeBytes(path, new Uint8Array());
      target = path;
      emit({
        scope: 'app',
        kind: 'info',
        message: 'event log ' + path,
        data: {
          action: 'log.open',
          path,
          maxEvents: MAX_LOG_EVENTS,
          maxLineBytes: MAX_LINE_BYTES,
        },
      });
    } catch {
      recording = false;
    }
  })();

  return {
    flush,
    stop: () => {
      recording = false;
      unsub();
      clearInterval(timer);
      void flush();
    },
  };
}
