// state.ts — what the preview shows for each widget, folded from the
// session and the host's events. Pure.

import type { WidgetPlanEntry } from '../generated/api';
import type { BusEvent } from '../generated/events';

export type WidgetPhase =
  /** Not approved, no runtime here, or no widget host: the PDF poster. */
  | 'poster'
  /** Approved, waiting for a live slot. */
  | 'idle'
  | 'starting'
  | 'live'
  | 'unresponsive'
  | 'suspended'
  /** Killed or crashed this session; a reload may start it again. */
  | 'stopped'
  | 'quarantined';

export interface WidgetView {
  phase: WidgetPhase;
  active: boolean;
}

export type WidgetStates = Record<string, WidgetView>;

/** The starting state for a session's entries. */
export function initialStates(entries: WidgetPlanEntry[], unavailable: boolean): WidgetStates {
  const out: WidgetStates = {};
  for (const e of entries) {
    out[e.id] = { phase: unavailable || e.poster ? 'poster' : 'idle', active: false };
  }
  return out;
}

/** Applies one host event; anything else leaves the states unchanged. */
export function reduce(states: WidgetStates, e: BusEvent): WidgetStates {
  const ev = e.event;
  if (!ev.action.startsWith('widget.') || !('id' in ev)) return states;
  const cur = states[ev.id];
  if (!cur) return states;
  const set = (patch: Partial<WidgetView>): WidgetStates => ({
    ...states,
    [ev.id]: { ...cur, ...patch },
  });
  switch (ev.action) {
    case 'widget.launched':
      return set({ phase: 'starting', active: false });
    case 'widget.loaded':
    case 'widget.responsive':
      return set({ phase: 'live' });
    case 'widget.unresponsive':
      return set({ phase: 'unresponsive' });
    case 'widget.suspended':
      return set({ phase: 'suspended', active: false });
    case 'widget.killed':
    case 'widget.exited':
    case 'widget.launch-failed':
      return cur.phase === 'quarantined' ? states : set({ phase: 'stopped', active: false });
    case 'widget.quarantined':
      return set({ phase: 'quarantined', active: false });
    case 'widget.active':
      return set({ active: ev.on });
    default:
      return states;
  }
}

/** Live widgets need a slot; posters never take one. */
export function wantsSlot(phase: WidgetPhase): boolean {
  return phase !== 'poster' && phase !== 'quarantined' && phase !== 'stopped';
}

/** The short note shown beside a slot, if any. */
export function noteFor(phase: WidgetPhase, entry: WidgetPlanEntry | undefined): string | null {
  switch (phase) {
    case 'poster':
      return entry?.poster === 'no-runtime' ? 'Poster only' : null;
    case 'unresponsive':
      return 'Not responding';
    case 'stopped':
      return 'Stopped';
    case 'quarantined':
      return 'Stopped for this session';
    default:
      return null;
  }
}
