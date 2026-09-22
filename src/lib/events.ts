// events.ts — the app bus front door: every surface reports here with a
// typed event. Delivery and retention live behind `transport()`.
//
// Payload types are generated from the Rust catalog (src-tauri/events);
// nothing here declares an event shape of its own.

import { transport } from './event-transport';
import type { AppEvent, BusEvent } from './generated/events';

/** An emit: `at` defaults to now. */
type EmitInput = Omit<BusEvent, 'at'> & { at?: number };

export function emit(e: EmitInput): BusEvent {
  const full: BusEvent = { ...e, at: e.at ?? Date.now() };
  transport().publish(full);
  return full;
}

/** The event's payload when it carries `action`, else null. */
export function eventOf<A extends AppEvent['action']>(
  e: BusEvent,
  action: A,
): Extract<AppEvent, { action: A }> | null {
  return e.event.action === action ? (e.event as Extract<AppEvent, { action: A }>) : null;
}

// Rendered window over the retained events: newest 100, oldest first.
export const STREAM_CAP = 100;
export function tailEvents(evts: BusEvent[]): BusEvent[] {
  return evts.slice(-STREAM_CAP);
}
