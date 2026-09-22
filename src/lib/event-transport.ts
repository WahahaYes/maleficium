// event-transport.ts — delivery for the app bus.
//
// Emitters publish typed events here (through `emit`); readers subscribe or
// take a snapshot. The local impl keeps the newest events in memory for this
// window. A bridge to another process or a hosted event stream is a second
// impl behind the same interface, so no subscriber is re-cut when delivery
// moves.

import type { BusEvent } from './generated/events';

export interface EventTransport {
  publish(e: BusEvent): void;
  /** Deliver every later event to `cb`; returns the unsubscribe. */
  subscribe(cb: (e: BusEvent) => void): () => void;
  /** Retained events, oldest first. */
  snapshot(): BusEvent[];
  /** Drop retained events (subscribers stay). */
  clear(): void;
}

/** In-process delivery retaining the newest `cap` events. */
export function createLocalTransport(cap = 500): EventTransport {
  const buf: BusEvent[] = [];
  const subs = new Set<(e: BusEvent) => void>();
  return {
    publish(e) {
      buf.push(e);
      if (buf.length > cap) buf.splice(0, buf.length - cap);
      subs.forEach((cb) => cb(e));
    },
    subscribe(cb) {
      subs.add(cb);
      return () => {
        subs.delete(cb);
      };
    },
    snapshot: () => [...buf],
    clear() {
      buf.length = 0;
    },
  };
}

// Local delivery is the default so every module can emit from load time;
// a platform with other delivery swaps it at boot.
let impl: EventTransport = createLocalTransport();

export function setEventTransport(next: EventTransport) {
  impl = next;
}

export function transport(): EventTransport {
  return impl;
}
