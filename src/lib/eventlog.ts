// eventlog.ts — the event bus, recorded app-locally as JSONL.
//
// One file, under the app-data dir, shared by every writer: the app and
// every agent acting through the automation surface append typed events
// with an actor, and readers tell them apart by it. The core owns the
// writer (whole-line appends, rotation keeping the newest MAX_LOG_EVENTS
// lines); the log is never truncated on start, so a run's segment starts
// at its own `log.open`. Every line is one JSON object
// `{at, scope, kind, actor, message, event}` (the `LogLine` type generated
// from the Rust catalog), and `event.action` names the fact so a reader
// matches on the payload rather than on prose.
//
// Nothing is written to the project dir, and a write that fails is dropped
// rather than surfaced.
//
// Read the current run hands-free (Linux app-data home):
//   L=~/.local/share/io.github.wahahayes.maleficium/maleficium-log/events.jsonl
//   cat "$L"                                   # every run, newest last
//   grep '"action":"compile.finish"' "$L"      # one fact
//   tail -f "$L"                               # live

import { request } from './core-request.tauri';
import { emit } from './events';
import { transport } from './event-transport';
import type { AppEvent, BusEvent, LogLine } from './generated/events';
import type { RecordOutcome } from './history';

/** Newest events kept on disk; older lines are dropped by core rotation. */
export const MAX_LOG_EVENTS = 2000;
/** Ceiling on one serialized line, newline included. */
export const MAX_LINE_BYTES = 2048;
/** Message text is cut to this many characters before serializing. */
export const MAX_MESSAGE_CHARS = 300;
/** Pending lines are written at this interval. */
const FLUSH_MS = 500;

const enc = new TextEncoder();

function byteLength(s: string): number {
  return enc.encode(s).length;
}

/**
 * One JSONL line. A payload that would break the line ceiling is dropped
 * down to its action, so every line stays parseable. Byte-identical in
 * shape to the core writer's `serialize`, which owns the file.
 */
export function serializeEvent(e: BusEvent): string {
  const head = {
    at: e.at,
    scope: e.scope,
    kind: e.kind,
    actor: e.actor,
    message: e.message.slice(0, MAX_MESSAGE_CHARS),
  };
  let line = JSON.stringify({ ...head, event: e.event } satisfies LogLine);
  if (byteLength(line) + 1 > MAX_LINE_BYTES) {
    line = JSON.stringify({ ...head, dropped: e.event.action } satisfies LogLine);
  }
  return line + '\n';
}

/** Parse JSONL text; a line that is not one event is skipped. */
export function parseEventLog(text: string): LogLine[] {
  const out: LogLine[] = [];
  for (const raw of text.split('\n')) {
    if (raw === '') continue;
    try {
      const v = JSON.parse(raw) as LogLine;
      if (typeof v.at === 'number' && typeof v.message === 'string') out.push(v);
    } catch {
      /* a partial line is not an event */
    }
  }
  return out;
}

/** Lines whose event names this action (kept or dropped), oldest first. */
export function eventsFor(log: readonly LogLine[], action: AppEvent['action']): LogLine[] {
  return log.filter((e) => (e.event?.action ?? e.dropped) === action);
}

/** Payload for one snapshot attempt, from the store's own outcome. */
export function revisionRecordData(
  rel: string,
  outcome: RecordOutcome,
  revisions: number,
): AppEvent {
  return { action: 'revision.record', rel, ...outcome, revisions };
}

/** Payload for one restore, carrying the size of the text put back. */
export function revisionRestoreData(rel: string, rev: string, chars: number): AppEvent {
  return { action: 'revision.restore', rel, rev, chars };
}

export interface EventLog {
  /** Write pending lines now. */
  flush(): Promise<void>;
  /** Stop recording; queued lines are written once more. */
  stop(): void;
}

/**
 * Record the bus to the shared JSONL file through the core writer. Events
 * already in the in-memory buffer are included. Opening rotates instead of
 * truncating, so earlier runs' tails survive; an unreachable store simply
 * stops the recording.
 */
export function startEventLog(): EventLog {
  let recording = true;
  let flushing = false;
  let pending: BusEvent[] = transport().snapshot();

  const unsub = transport().subscribe((e) => {
    if (recording) pending.push(e);
  });

  const send = async (batch: BusEvent[]): Promise<void> => {
    try {
      await request('eventAppend', { events: batch });
    } catch {
      /* the log is never the reason the app stops */
    }
  };

  const flush = async (): Promise<void> => {
    if (flushing || !recording || pending.length === 0) return;
    flushing = true;
    const batch = pending;
    pending = [];
    await send(batch);
    flushing = false;
  };

  const timer = setInterval(() => void flush(), FLUSH_MS);

  void (async () => {
    try {
      const report = await request('eventRotate', {});
      if (!recording) return;
      emit({
        scope: 'app',
        kind: 'info',
        actor: 'system',
        message: 'event log ' + report.path,
        event: {
          action: 'log.open',
          path: report.path,
          maxEvents: MAX_LOG_EVENTS,
          maxLineBytes: MAX_LINE_BYTES,
        },
      });
    } catch {
      // No app-data dir (or not writable): the bus runs, the file stays off.
      recording = false;
    }
  })();

  return {
    flush,
    stop: () => {
      const batch = pending;
      pending = [];
      recording = false;
      unsub();
      clearInterval(timer);
      if (batch.length > 0) void send(batch);
    },
  };
}
