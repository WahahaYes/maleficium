import { describe, expect, it } from 'vitest';
import type { BusEvent } from '../generated/events';
import { initialStates, noteFor, reduce, wantsSlot } from './state';

const ev = (event: BusEvent['event']): BusEvent => ({
  at: 0,
  scope: 'preview',
  kind: 'info',
  actor: 'system',
  message: '',
  event,
});

const entries = [
  { id: 'a', declaresNetwork: false, network: false },
  { id: 'b', poster: 'no-runtime' as const, declaresNetwork: false, network: false },
];

describe('widget states', () => {
  it('starts approved widgets idle and the rest as posters', () => {
    expect(initialStates(entries, false)).toEqual({
      a: { phase: 'idle', active: false },
      b: { phase: 'poster', active: false },
    });
    expect(initialStates(entries, true).a.phase).toBe('poster');
  });

  it('follows the host through a freeze, a kill and a quarantine', () => {
    let s = initialStates(entries, false);
    s = reduce(s, ev({ action: 'widget.launched', id: 'a', network: false, contained: true }));
    expect(s.a.phase).toBe('starting');
    s = reduce(s, ev({ action: 'widget.loaded', id: 'a', ms: 300 }));
    s = reduce(s, ev({ action: 'widget.active', id: 'a', on: true, why: 'host' }));
    expect(s.a).toEqual({ phase: 'live', active: true });
    s = reduce(s, ev({ action: 'widget.unresponsive', id: 'a', gapMs: 2100 }));
    expect(s.a.phase).toBe('unresponsive');
    s = reduce(
      s,
      ev({ action: 'widget.killed', id: 'a', reason: 'frozen', gapMs: 5100, kills: 1 }),
    );
    expect(s.a).toEqual({ phase: 'stopped', active: false });
    s = reduce(s, ev({ action: 'widget.quarantined', id: 'a' }));
    s = reduce(s, ev({ action: 'widget.killed', id: 'a', reason: 'frozen', kills: 2 }));
    expect(s.a.phase).toBe('quarantined');
    expect(wantsSlot(s.a.phase)).toBe(false);
  });

  it('ignores events for unknown widgets and other actions', () => {
    const s = initialStates(entries, false);
    expect(reduce(s, ev({ action: 'widget.loaded', id: 'zz', ms: 1 }))).toBe(s);
    expect(reduce(s, ev({ action: 'preview.zoom', mode: 'percent', percent: 100 }))).toBe(s);
  });

  it('notes only what the user needs to know', () => {
    expect(noteFor('live', entries[0])).toBeNull();
    expect(noteFor('poster', entries[1])).toBe('Poster only');
    expect(noteFor('poster', entries[0])).toBeNull();
    expect(noteFor('unresponsive', entries[0])).toBe('Not responding');
  });
});
