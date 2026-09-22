import { describe, it, expect, beforeEach } from 'vitest';
import * as events from './events';
import type { BusEvent } from './generated/events';

describe('events bus', () => {
  beforeEach(() => {
    events.clear();
  });

  it('assigns Date.now-ish at when omitted', () => {
    const before = Date.now();
    const e = events.emit({
      scope: 'app',
      kind: 'info',
      actor: 'system',
      message: 'hello',
      event: { action: 'file.load', path: 'p' },
    });
    const after = Date.now();
    expect(e.at).toBeGreaterThanOrEqual(before);
    expect(e.at).toBeLessThanOrEqual(after);
  });

  it('preserves explicit at', () => {
    const explicitAt = 12345;
    const e = events.emit({
      scope: 'app',
      kind: 'info',
      actor: 'system',
      message: 'hello',
      at: explicitAt,
      event: { action: 'file.load', path: 'p' },
    });
    expect(e.at).toBe(explicitAt);
  });

  it('keeps only 500 events, evicting oldest', () => {
    events.clear();
    for (let i = 1; i <= 600; i++) {
      events.emit({
        scope: 'app',
        kind: 'info',
        actor: 'system',
        message: `m${i}`,
        event: { action: 'file.load', path: 'p' },
      });
    }
    const list = events.list();
    expect(list.length).toBe(500);
    expect(list[0].message).toBe('m101');
    expect(list[list.length - 1].message).toBe('m600');
  });

  it('subscriber receives events, unsub stops delivery', () => {
    events.clear();
    const received: BusEvent[] = [];
    const unsub = events.subscribe((e) => {
      received.push(e);
    });
    events.emit({
      scope: 'app',
      kind: 'info',
      actor: 'system',
      message: 'first',
      event: { action: 'file.load', path: 'p' },
    });
    events.emit({
      scope: 'app',
      kind: 'info',
      actor: 'system',
      message: 'second',
      event: { action: 'file.load', path: 'p' },
    });
    expect(received.length).toBe(2);
    expect(received[0].message).toBe('first');
    expect(received[1].message).toBe('second');
    unsub();
    events.emit({
      scope: 'app',
      kind: 'info',
      actor: 'system',
      message: 'third',
      event: { action: 'file.load', path: 'p' },
    });
    expect(received.length).toBe(2);
  });

  it('clear empties the list', () => {
    events.clear();
    events.emit({
      scope: 'app',
      kind: 'info',
      actor: 'system',
      message: 'test',
      event: { action: 'file.load', path: 'p' },
    });
    expect(events.list().length).toBe(1);
    events.clear();
    expect(events.list().length).toBe(0);
  });

  it('renders newest 100 oldest-first, passes short lists through', () => {
    events.clear();
    for (let i = 1; i <= 250; i++) {
      events.emit({
        scope: 'app',
        kind: 'info',
        actor: 'system',
        message: `m${i}`,
        event: { action: 'file.load', path: 'p' },
      });
    }
    const tail = events.tailEvents(events.list());
    expect(tail.length).toBe(100);
    expect(tail[0].message).toBe('m151');
    expect(tail[tail.length - 1].message).toBe('m250');
    const short = events.tailEvents(events.list().slice(0, 3));
    expect(short.map((e) => e.message)).toEqual(['m1', 'm2', 'm3']);
  });
});
