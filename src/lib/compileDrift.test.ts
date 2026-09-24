import { describe, expect, it } from 'vitest';
import {
  bundleDrifted,
  compileLogText,
  missingLine,
  offlineBadge,
  readinessLine,
  type CompileResult,
} from './compile';
import { emit } from './events';
import { parseEventLog, eventsFor, serializeEvent } from './eventlog';
import { setEventTransport, createLocalTransport, transport } from './event-transport';
import type { OfflineReadiness } from './generated/events';

const DRIFTED_RUN: CompileResult = {
  ok: true,
  pdfUrl: '/paper/out.pdf',
  log: '',
  failure: null,
  missing: { reason: 'bundle-changed' },
};

const DRIFTED_READINESS: OfflineReadiness = {
  state: 'unverified',
  needs: ['TeX bundle changed'],
  missing: { reason: 'bundle-changed' },
};

const CLEAN_RUN: CompileResult = {
  ok: true,
  pdfUrl: '/paper/out.pdf',
  log: '',
  failure: null,
  missing: null,
};

const CLEAN_READINESS: OfflineReadiness = { state: 'ready', needs: [], missing: null };

describe('bundle drift is loud', () => {
  it('marks the drifted readiness as drifted', () => {
    expect(bundleDrifted(DRIFTED_READINESS)).toBe(true);
    expect(bundleDrifted(CLEAN_READINESS)).toBe(false);
    expect(bundleDrifted({ state: 'unverified', needs: [], missing: null })).toBe(false);
  });

  it('badges drift as an error naming the pdf mismatch', () => {
    const badge = offlineBadge(DRIFTED_READINESS);
    expect(badge.tone).toBe('error');
    expect(badge.label).toMatch(/bundle changed/i);
    expect(badge.label).toMatch(/pdf/i);
    expect(badge.title).toMatch(/may not match/i);
  });

  it('keeps a plain unverified badge neutral', () => {
    const badge = offlineBadge({ state: 'unverified', needs: [], missing: null });
    expect(badge.tone).toBe('neutral');
    expect(badge.label).toBe('Offline unverified');
  });

  it('ranks a drifted missing event as an error even beside a pdf', () => {
    const line = missingLine(DRIFTED_RUN.missing!, DRIFTED_RUN.ok);
    expect(line).not.toBeNull();
    expect(line!.kind).toBe('error');
    expect(line!.message).toMatch(/may not match/i);
    expect(missingLine({ file: 'x.sty', reason: 'not-cached' }, false)!.kind).toBe('error');
    expect(missingLine({ file: 'x.sty', reason: 'not-cached' }, true)!.kind).toBe('warn');
  });

  it('ranks drifted readiness as an error and ready as success', () => {
    expect(readinessLine(DRIFTED_READINESS).kind).toBe('error');
    expect(readinessLine(CLEAN_READINESS).kind).toBe('success');
    expect(readinessLine({ state: 'needs-network', needs: ['x.sty'], missing: null }).kind).toBe(
      'info',
    );
  });

  it('leads the drifted run log with the mismatch warning', () => {
    const text = compileLogText(DRIFTED_RUN);
    expect(text).toMatch(/may not match/i);
    expect(text).toContain('/paper/out.pdf');
    expect(compileLogText(CLEAN_RUN)).toBe('/paper/out.pdf');
  });

  it('leaves a drifted run loud and a clean run untraced in the event log', () => {
    setEventTransport(createLocalTransport());
    try {
      const drift = missingLine(DRIFTED_RUN.missing!, DRIFTED_RUN.ok)!;
      emit({
        scope: 'compile',
        kind: drift.kind,
        actor: 'user',
        message: drift.message,
        event: {
          action: 'compile.missing',
          target: 'main.tex',
          file: null,
          reason: 'bundle-changed',
        },
      });
      const ready = readinessLine(DRIFTED_READINESS);
      emit({
        scope: 'compile',
        kind: ready.kind,
        actor: 'system',
        message: ready.message,
        event: {
          action: 'offline.readiness',
          root: '/paper',
          state: 'unverified',
          needs: ['TeX bundle changed'],
        },
      });
      const log = parseEventLog(transport().snapshot().map(serializeEvent).join(''));
      const missing = eventsFor(log, 'compile.missing');
      expect(missing).toHaveLength(1);
      expect(missing[0]!.kind).toBe('error');
      const readiness = eventsFor(log, 'offline.readiness');
      expect(readiness).toHaveLength(1);
      expect(readiness[0]!.kind).toBe('error');
      expect(readiness[0]!.message).toMatch(/bundle changed/i);
    } finally {
      setEventTransport(createLocalTransport());
    }
  });

  it('leaves no drift trace for a clean run', () => {
    setEventTransport(createLocalTransport());
    try {
      expect(missingLine(CLEAN_RUN.missing as never, CLEAN_RUN.ok)).toBeNull();
      const ready = readinessLine(CLEAN_READINESS);
      emit({
        scope: 'compile',
        kind: ready.kind,
        actor: 'system',
        message: ready.message,
        event: { action: 'offline.readiness', root: '/paper', state: 'ready', needs: [] },
      });
      const log = parseEventLog(transport().snapshot().map(serializeEvent).join(''));
      expect(eventsFor(log, 'compile.missing')).toHaveLength(0);
      expect(eventsFor(log, 'offline.readiness')[0]!.kind).toBe('success');
      expect(log.map((e) => e.message).join('\n')).not.toMatch(/bundle changed/i);
    } finally {
      setEventTransport(createLocalTransport());
    }
  });
});
