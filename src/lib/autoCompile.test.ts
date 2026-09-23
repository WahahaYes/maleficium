import { describe, expect, it } from 'vitest';
import {
  AUTO_COMPILE_DEBOUNCE_MS as D,
  INITIAL_AUTO,
  parseAutoCompile,
  stepAuto,
  type AutoInput,
  type AutoState,
} from './autoCompile';

function run(inputs: AutoInput[], s: AutoState = INITIAL_AUTO) {
  const fired: number[] = [];
  for (const [n, i] of inputs.entries()) {
    const r = stepAuto(s, i);
    s = r.state;
    if (r.fire) fired.push(n);
  }
  return { s, fired };
}

describe('auto-compile scheduler', () => {
  it('debounces saves into one run a second after the last', () => {
    const { fired } = run([
      { kind: 'save', at: 0 },
      { kind: 'save', at: 500 },
      { kind: 'tick', at: 1000 },
      { kind: 'tick', at: 500 + D },
    ]);
    expect(fired).toEqual([3]);
  });

  it('never overlaps a run and queues exactly one follow-up', () => {
    const { s, fired } = run([
      { kind: 'save', at: 0 },
      { kind: 'tick', at: D },
      { kind: 'start' },
      { kind: 'save', at: D + 10 },
      { kind: 'save', at: D + 20 },
      { kind: 'tick', at: 5 * D },
      { kind: 'finish', at: 6 * D },
      { kind: 'tick', at: 6 * D + 10 },
      { kind: 'tick', at: 7 * D },
      { kind: 'start' },
      { kind: 'finish', at: 8 * D },
      { kind: 'tick', at: 20 * D },
    ]);
    expect(fired).toEqual([1, 8]);
    expect(s.queued).toBe(false);
    expect(s.dueAt).toBeNull();
  });

  it('holds back while a manual run is in flight', () => {
    const { fired } = run([
      { kind: 'start' },
      { kind: 'save', at: 0 },
      { kind: 'tick', at: 5 * D },
      { kind: 'finish', at: 5 * D },
      { kind: 'tick', at: 6 * D },
    ]);
    expect(fired).toEqual([4]);
  });

  it('does nothing when off, and a switch or disable drops what was pending', () => {
    expect(
      run([
        { kind: 'enable', on: false },
        { kind: 'save', at: 0 },
        { kind: 'tick', at: D },
      ]).fired,
    ).toEqual([]);
    expect(
      run([{ kind: 'save', at: 0 }, { kind: 'reset' }, { kind: 'tick', at: D }]).fired,
    ).toEqual([]);
    expect(
      run([
        { kind: 'save', at: 0 },
        { kind: 'enable', on: false },
        { kind: 'enable', on: true },
        { kind: 'tick', at: D },
      ]).fired,
    ).toEqual([]);
  });

  it('is on unless stored off', () => {
    expect(parseAutoCompile(null)).toBe(true);
    expect(parseAutoCompile('true')).toBe(true);
    expect(parseAutoCompile('false')).toBe(false);
  });
});
