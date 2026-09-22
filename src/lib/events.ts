// events.ts — the app bus: every surface reports here with a typed event.
//
// Payload types are generated from the Rust catalog (src-tauri/events);
// nothing here declares an event shape of its own.

import type { AppEvent, BusEvent } from './generated/events';

/** An emit: `at` defaults to now. */
type EmitInput = Omit<BusEvent, 'at'> & { at?: number };

const buf: BusEvent[] = [];
const subs = new Set<(e: BusEvent) => void>();
export function emit(e: EmitInput): BusEvent {
  const full: BusEvent = { ...e, at: e.at ?? Date.now() };
  buf.push(full);
  if (buf.length > 500) buf.splice(0, buf.length - 500);
  subs.forEach((cb) => cb(full));
  return full;
}
export function subscribe(cb: (e: BusEvent) => void) {
  subs.add(cb);
  return () => {
    subs.delete(cb);
  };
}
export function list(): BusEvent[] {
  return [...buf];
}
export function clear() {
  buf.length = 0;
}

/** The event's payload when it carries `action`, else null. */
export function eventOf<A extends AppEvent['action']>(
  e: BusEvent,
  action: A,
): Extract<AppEvent, { action: A }> | null {
  return e.event.action === action ? (e.event as Extract<AppEvent, { action: A }>) : null;
}

// Rendered window over the buffer: newest 100, oldest first.
export const STREAM_CAP = 100;
export function tailEvents(evts: BusEvent[]): BusEvent[] {
  return evts.slice(-STREAM_CAP);
}
