import { describe, expect, it } from 'vitest';
import { INITIAL_REFRESH, onAppEvent, onPoll, type OutputStamp } from './externalRefresh';

const A: OutputStamp = { mtimeMs: 1, bytes: 10 };
const B: OutputStamp = { mtimeMs: 2, bytes: 12 };
const C: OutputStamp = { mtimeMs: 3, bytes: 12 };

describe('external refresh', () => {
  it('adopts the first stamp, then reloads when someone else rewrites the pdf', () => {
    let r = onPoll(INITIAL_REFRESH, A);
    expect(r.reload).toBe(false);
    r = onPoll(r.state, A);
    expect(r.reload).toBe(false);
    r = onPoll(r.state, B);
    expect(r.reload).toBe(true);
    r = onPoll(r.state, B);
    expect(r.reload).toBe(false);
  });

  it("never reloads for the app's own compile", () => {
    let s = onPoll(INITIAL_REFRESH, A).state;
    s = onAppEvent(s, { action: 'compile.start', target: 'main.tex' });
    // The pdf is rewritten mid-compile: ignored while compiling.
    let r = onPoll(s, B);
    expect(r.reload).toBe(false);
    s = onAppEvent(r.state, { action: 'compile.finish', target: 'main.tex', ok: true, ms: 5 });
    // First poll after the own finish adopts; the own finish already refreshed.
    r = onPoll(s, B);
    expect(r.reload).toBe(false);
    r = onPoll(r.state, C);
    expect(r.reload).toBe(true);
  });

  it('ignores a pdf that disappears (clean outputs)', () => {
    const s = onPoll(INITIAL_REFRESH, A).state;
    expect(onPoll(s, null).reload).toBe(false);
  });
});
