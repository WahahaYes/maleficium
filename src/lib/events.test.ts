import { describe, it, expect, beforeEach } from 'vitest';
import * as events from './events';

describe('events bus', () => {
  beforeEach(() => {
    events.clear();
  });

  it('assigns Date.now-ish at when omitted', () => {
    const before = Date.now();
    const e = events.emit({ scope: 'app', kind: 'info', message: 'hello' });
    const after = Date.now();
    expect(e.at).toBeGreaterThanOrEqual(before);
    expect(e.at).toBeLessThanOrEqual(after);
  });

  it('preserves explicit at', () => {
    const explicitAt = 12345;
    const e = events.emit({ scope: 'app', kind: 'info', message: 'hello', at: explicitAt });
    expect(e.at).toBe(explicitAt);
  });

  it('keeps only 500 events, evicting oldest', () => {
    events.clear();
    for (let i = 1; i <= 600; i++) {
      events.emit({ scope: 'app', kind: 'info', message: `m${i}` });
    }
    const list = events.list();
    expect(list.length).toBe(500);
    expect(list[0].message).toBe('m101');
    expect(list[list.length - 1].message).toBe('m600');
  });

  it('subscriber receives events', () => {
    events.clear();
    const received: events.BusEvent[] = [];
    const unsub = events.subscribe((e) => {
      received.push(e);
    });
    events.emit({ scope: 'app', kind: 'info', message: 'first' });
    events.emit({ scope: 'app', kind: 'info', message: 'second' });
    expect(received.length).toBe(2);
    expect(received[0].message).toBe('first');
    expect(received[1].message).toBe('second');
    unsub();
    events.emit({ scope: 'app', kind: 'info', message: 'third' });
    expect(received.length).toBe(2);
  });

  it('unsub stops receiving events', () => {
    events.clear();
    const received: events.BusEvent[] = [];
    const unsub = events.subscribe((e) => {
      received.push(e);
    });
    events.emit({ scope: 'app', kind: 'info', message: 'before unsub' });
    unsub();
    events.emit({ scope: 'app', kind: 'info', message: 'after unsub' });
    expect(received.length).toBe(1);
    expect(received[0].message).toBe('before unsub');
  });

  it('clear empties the list', () => {
    events.clear();
    events.emit({ scope: 'app', kind: 'info', message: 'test' });
    expect(events.list().length).toBe(1);
    events.clear();
    expect(events.list().length).toBe(0);
  });
});