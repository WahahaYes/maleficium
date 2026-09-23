// autoCompile.ts — compile on save: when to start a run.
//
// Pure reducer. A save (while enabled) arms a run one debounce later; saves
// in the window re-arm it. Any run in flight (manual or automatic) holds new
// ones back: saves during it queue exactly one follow-up, armed when it
// finishes. A project switch drops whatever was pending.

export const AUTO_COMPILE_DEBOUNCE_MS = 1000;

export type AutoState = {
  enabled: boolean;
  /** When the armed run starts (epoch ms), if one is armed. */
  dueAt: number | null;
  running: boolean;
  queued: boolean;
};

export type AutoInput =
  | { kind: 'enable'; on: boolean }
  | { kind: 'save'; at: number }
  | { kind: 'tick'; at: number }
  | { kind: 'start' }
  | { kind: 'finish'; at: number }
  | { kind: 'reset' };

export const INITIAL_AUTO: AutoState = {
  enabled: true,
  dueAt: null,
  running: false,
  queued: false,
};

/** Fold one input; `fire` says start an automatic run now. */
export function stepAuto(s: AutoState, i: AutoInput): { state: AutoState; fire: boolean } {
  const keep = (state: AutoState) => ({ state, fire: false });
  switch (i.kind) {
    case 'enable':
      return keep(
        i.on ? { ...s, enabled: true } : { ...s, enabled: false, dueAt: null, queued: false },
      );
    case 'save':
      if (!s.enabled) return keep(s);
      if (s.running) return keep({ ...s, queued: true });
      return keep({ ...s, dueAt: i.at + AUTO_COMPILE_DEBOUNCE_MS });
    case 'tick':
      if (s.dueAt == null || i.at < s.dueAt || s.running || !s.enabled) return keep(s);
      return { state: { ...s, dueAt: null }, fire: true };
    case 'start':
      return keep({ ...s, running: true, dueAt: null });
    case 'finish':
      if (s.queued && s.enabled) {
        return keep({
          ...s,
          running: false,
          queued: false,
          dueAt: i.at + AUTO_COMPILE_DEBOUNCE_MS,
        });
      }
      return keep({ ...s, running: false, queued: false });
    case 'reset':
      return keep({ ...s, dueAt: null, queued: false });
  }
}

/** A stored toggle: on unless explicitly stored off. */
export function parseAutoCompile(raw: string | null): boolean {
  return raw !== 'false';
}
