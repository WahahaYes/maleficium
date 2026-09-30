import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

import { invoke } from '@tauri-apps/api/core';
import * as events from './events';
import { transport } from './event-transport';
import { appEventLogDir, eventLogPath } from './paths';

import {
  MAX_LINE_BYTES,
  MAX_LOG_EVENTS,
  MAX_MESSAGE_CHARS,
  eventsFor,
  parseEventLog,
  revisionRecordData,
  revisionRestoreData,
  serializeEvent,
  startEventLog,
} from './eventlog';
import type { BusEvent, LogLine } from './generated/events';

const APP_DATA = '/app/data';
const APP_LOG = eventLogPath(APP_DATA);

/** Batches the recorder sent to the core writer, in order. */
const appended: BusEvent[][] = [];
/** Ops the recorder ran, in order. */
const ops: string[] = [];

/** The core writer behind a mock backend: rotate opens, appends are kept. */
function mockWriter() {
  vi.mocked(invoke).mockImplementation(async (cmd, args) => {
    if (cmd !== 'core_request') throw new Error('unexpected transport ' + String(cmd));
    const req = (args as { req: { op: string; params: { events?: BusEvent[] } } }).req;
    ops.push(req.op);
    if (req.op === 'eventRotate') {
      return { op: 'eventRotate', result: { path: APP_LOG, kept: 0, dropped: 0 } };
    }
    if (req.op === 'eventAppend') {
      appended.push(req.params.events ?? []);
      return { op: 'eventAppend', result: null };
    }
    throw new Error('unexpected op ' + req.op);
  });
}

/** Let the recorder resolve its path, then write everything queued. */
async function settle(log: { flush(): Promise<void> }) {
  for (let i = 0; i < 5; i++) await Promise.resolve();
  await log.flush();
}

describe('event log path', () => {
  it('lives under the app-data dir, never in a project', () => {
    const dir = appEventLogDir(APP_DATA);
    const file = eventLogPath(APP_DATA);
    expect(dir).toBe('/app/data/maleficium-log');
    expect(file).toBe('/app/data/maleficium-log/events.jsonl');
    expect(file.startsWith('/home/u/paper')).toBe(false);
  });
});

describe('event serialization', () => {
  beforeEach(() => {
    transport().clear();
  });

  it('writes one parseable JSON line carrying the structured payload', () => {
    const e = events.emit({
      scope: 'compile',
      kind: 'success',
      actor: 'user',
      message: 'compiled /p/main.pdf',
      event: { action: 'compile.finish', target: '/p/main.tex', ok: true, ms: 1200 },
    });
    const line = serializeEvent(e);
    expect(line.endsWith('\n')).toBe(true);
    expect(line.indexOf('\n')).toBe(line.length - 1);
    const parsed = JSON.parse(line) as LogLine;
    expect(parsed).toMatchObject({
      at: e.at,
      scope: 'compile',
      kind: 'success',
      actor: 'user',
      message: 'compiled /p/main.pdf',
      event: { action: 'compile.finish', target: '/p/main.tex', ok: true, ms: 1200 },
    });
  });

  it('keeps an oversized payload parseable by reducing it to its action', () => {
    const e = events.emit({
      scope: 'compile',
      kind: 'error',
      actor: 'user',
      message: 'x'.repeat(MAX_MESSAGE_CHARS * 2),
      event: {
        action: 'compile.problem',
        rootId: null,
        line: 1,
        message: 'y'.repeat(MAX_LINE_BYTES * 2),
        severity: 'error',
        external: true,
      },
    });
    const line = serializeEvent(e);
    expect(new TextEncoder().encode(line).length).toBeLessThanOrEqual(MAX_LINE_BYTES);
    const parsed = JSON.parse(line) as LogLine;
    expect(parsed.message.length).toBe(MAX_MESSAGE_CHARS);
    expect(parsed.event).toBeUndefined();
    expect(parsed.dropped).toBe('compile.problem');
    expect(eventsFor([parsed], 'compile.problem')).toHaveLength(1);
  });

  it('skips lines that are not events, including a partial tail', () => {
    const text =
      '{"at":1,"scope":"app","kind":"info","actor":"system","message":"a"}\n{"at":2,"sco';
    expect(parseEventLog(text).map((e) => e.message)).toEqual(['a']);
  });
});

describe('event log recording', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    transport().clear();
    appended.length = 0;
    ops.length = 0;
    mockWriter();
  });
  afterEach(() => {
    transport().clear();
  });

  it('opens with a rotate and appends the stream through core', async () => {
    const log = startEventLog();
    try {
      events.emit({
        scope: 'app',
        kind: 'info',
        actor: 'system',
        message: 'one',
        event: { action: 'file.load', path: 'p' },
      });
      events.emit({
        scope: 'fs',
        kind: 'success',
        actor: 'user',
        message: 'two',
        event: { action: 'file.save', path: '/p/a.tex', chars: 1, mode: 'manual' },
      });
      await settle(log);
      // The run starts by rotating, never by truncating: the first backend
      // call opens the shared log, and every call goes through the contract.
      expect(ops[0]).toBe('eventRotate');
      for (const call of vi.mocked(invoke).mock.calls) expect(call[0]).toBe('core_request');
      const text = appended.flat().map(serializeEvent).join('');
      const parsed = parseEventLog(text);
      expect(parsed.filter((e) => e.message === 'one' || e.message === 'two')).toHaveLength(2);
      expect(parsed.map((e) => e.message)).toContain('event log ' + APP_LOG);
      expect(eventsFor(parsed, 'file.save')).toHaveLength(1);
      expect(eventsFor(parsed, 'log.open')[0].event).toMatchObject({
        path: APP_LOG,
        maxEvents: MAX_LOG_EVENTS,
        maxLineBytes: MAX_LINE_BYTES,
      });
    } finally {
      log.stop();
    }
  });

  it('writes the queued remainder on stop', async () => {
    const log = startEventLog();
    await settle(log);
    events.emit({
      scope: 'app',
      kind: 'info',
      actor: 'system',
      message: 'late',
      event: { action: 'file.load', path: 'p' },
    });
    log.stop();
    for (let i = 0; i < 5; i++) await Promise.resolve();
    expect(appended.flat().map((e) => e.message)).toContain('late');
  });

  it('degrades silently when the store is unreachable', async () => {
    vi.mocked(invoke).mockRejectedValue(new Error('no backend'));
    const log = startEventLog();
    try {
      events.emit({
        scope: 'app',
        kind: 'info',
        actor: 'system',
        message: 'still runs',
        event: { action: 'file.load', path: 'p' },
      });
      await expect(settle(log)).resolves.toBeUndefined();
      expect(
        transport()
          .snapshot()
          .map((e) => e.message),
      ).toContain('still runs');
    } finally {
      log.stop();
    }
  });
});

describe('reading a run without watching the window', () => {
  // Saves and restores are provable from the payloads alone: the facts the
  // status bar shows a human are the same facts the log carries.
  it('proves revisions accumulate across three saves', () => {
    const rel = 'chapters/method.tex';
    const outcomes = [
      { stored: true as const, rev: '1', deduped: false },
      { stored: true as const, rev: '2', deduped: false },
      { stored: true as const, rev: '3', deduped: false },
    ];
    const lines = outcomes
      .map((o, i) =>
        serializeEvent({
          scope: 'fs',
          kind: 'info',
          at: 1000 + i,
          actor: 'system',
          message: `revision ${o.rev} of ${rel}`,
          event: revisionRecordData(rel, o, i + 1),
        }),
      )
      .join('');

    const records = eventsFor(parseEventLog(lines), 'revision.record').map((e) =>
      e.event?.action === 'revision.record' ? e.event : null,
    );
    expect(records.map((r) => r?.revisions)).toEqual([1, 2, 3]);
    expect(records.map((r) => r?.rev)).toEqual(['1', '2', '3']);
    expect(records.every((r) => r?.rel === rel && r.stored)).toBe(true);
  });

  it('proves an unchanged save keeps the count still', () => {
    const rel = 'main.tex';
    const line = serializeEvent({
      scope: 'fs',
      kind: 'info',
      at: 2000,
      actor: 'system',
      message: `no revision for ${rel} (unchanged)`,
      event: revisionRecordData(rel, { stored: false, reason: 'unchanged' }, 3),
    });
    const [record] = eventsFor(parseEventLog(line), 'revision.record');
    expect(record.event).toMatchObject({ stored: false, reason: 'unchanged', revisions: 3 });
  });

  it('proves a restore round-trips and stays undoable', () => {
    const rel = 'main.tex';
    const lines = [
      serializeEvent({
        scope: 'fs',
        kind: 'success',
        at: 3000,
        actor: 'system',
        message: 'restored ' + rel,
        event: revisionRestoreData(rel, '1', 42),
      }),
      serializeEvent({
        scope: 'fs',
        kind: 'info',
        at: 3001,
        actor: 'system',
        message: `revision 4 of ${rel}`,
        event: revisionRecordData(rel, { stored: true, rev: '4', deduped: false }, 4),
      }),
    ].join('');

    const parsed = parseEventLog(lines);
    const [restore] = eventsFor(parsed, 'revision.restore');
    expect(restore.event).toMatchObject({ rel, rev: '1', chars: 42 });
    // The replaced text is kept as a revision of its own, so the list grew.
    const [kept] = eventsFor(parsed, 'revision.record');
    expect(kept.event).toMatchObject({ rel, stored: true, revisions: 4 });
  });
});
