import { describe, it, expect } from 'vitest';
import { createLocalTransport } from './event-transport';
import type { BusEvent } from './generated/events';

const ev = (message: string): BusEvent => ({
  at: 1,
  scope: 'app',
  kind: 'info',
  actor: 'system',
  message,
  event: { action: 'file.load', path: 'p' },
});

describe('local event transport', () => {
  it('delivers to subscribers until they unsubscribe', () => {
    const t = createLocalTransport();
    const seen: string[] = [];
    const off = t.subscribe((e) => seen.push(e.message));
    t.publish(ev('a'));
    off();
    t.publish(ev('b'));
    expect(seen).toEqual(['a']);
  });

  it('retains the newest cap events and clears without dropping subscribers', () => {
    const t = createLocalTransport(3);
    const seen: string[] = [];
    t.subscribe((e) => seen.push(e.message));
    for (const m of ['1', '2', '3', '4']) t.publish(ev(m));
    expect(t.snapshot().map((e) => e.message)).toEqual(['2', '3', '4']);
    t.clear();
    expect(t.snapshot()).toEqual([]);
    t.publish(ev('5'));
    expect(seen).toEqual(['1', '2', '3', '4', '5']);
  });

  it('snapshots are copies', () => {
    const t = createLocalTransport();
    t.publish(ev('a'));
    t.snapshot().pop();
    expect(t.snapshot()).toHaveLength(1);
  });
});
