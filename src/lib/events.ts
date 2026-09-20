export type EventScope = 'compile' | 'preview' | 'fs' | 'app';
export type EventKind = 'info' | 'progress' | 'success' | 'error' | 'warn';
export interface BusEvent {
  scope: EventScope;
  kind: EventKind;
  at: number;
  message: string;
  data?: unknown;
}
const buf: BusEvent[] = [];
const subs = new Set<(e: BusEvent) => void>();
export function emit(e: Omit<BusEvent, 'at'> & { at?: number }) {
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

// Rendered window over the buffer: newest 100, oldest first.
export const STREAM_CAP = 100;
export function tailEvents(evts: BusEvent[]): BusEvent[] {
  return evts.slice(-STREAM_CAP);
}
