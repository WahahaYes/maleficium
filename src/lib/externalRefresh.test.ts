import { describe, expect, it } from 'vitest';
import {
  INITIAL_REFRESH,
  onAppEvent,
  onPoll,
  watchTarget,
  type OutputStamp,
} from './externalRefresh';

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

  it('reloads when someone else compiles a project this app never built', () => {
    // No pdf when the project opens: the first poll adopts the absence.
    let r = onPoll(INITIAL_REFRESH, null);
    expect(r.reload).toBe(false);
    r = onPoll(r.state, null);
    expect(r.reload).toBe(false);
    r = onPoll(r.state, A);
    expect(r.reload).toBe(true);
  });
});

describe('watch target', () => {
  const main = { rootId: 'r1', rootPath: '/p', mainRel: 'main.tex' };
  const other = { rootId: 'r1', rootPath: '/p', mainRel: 'ch/one.tex' };

  it('watches the shown document', () => {
    expect(watchTarget({ url: '/out/one.pdf', source: other }, main)).toEqual({
      key: 'r1:ch/one.tex',
      source: other,
      url: '/out/one.pdf',
    });
  });

  it('watches the main file, with no pdf yet, while nothing is shown', () => {
    const want = { key: 'r1:main.tex', source: main, url: null };
    expect(watchTarget(null, main)).toEqual(want);
    expect(watchTarget({ url: null, source: other }, main)).toEqual(want);
  });

  it('watches nothing with no document and no project', () => {
    expect(watchTarget(null, null)).toBeNull();
  });
});
