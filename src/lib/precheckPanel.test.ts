import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Finding } from './generated/structure';
import { setAppStore } from './app-store';
import { localAppStore } from './app-store.web';
import {
  findingsKey,
  loadPrecheckPopup,
  parsePrecheckPopup,
  savePrecheckPopup,
  shouldPop,
} from './precheckPanel';

setAppStore(localAppStore);
const mem: Record<string, string> = {};
vi.stubGlobal('localStorage', {
  getItem: (k: string) => mem[k] ?? null,
  setItem: (k: string, v: string) => {
    mem[k] = v;
  },
  removeItem: (k: string) => {
    delete mem[k];
  },
});
beforeEach(() => {
  for (const k of Object.keys(mem)) delete mem[k];
});

const nopkg: Finding = { kind: 'not-in-bundle', name: 'nopkg.sty', path: 'main.tex', line: 3 };
const biber: Finding = {
  kind: 'external-tool',
  name: 'biber',
  path: 'main.tex',
  line: 4,
  suggestion: 'backend=bibtex',
};

describe('findingsKey', () => {
  it('is empty for no findings and ignores order', () => {
    expect(findingsKey([])).toBe('');
    expect(findingsKey([nopkg, biber])).toBe(findingsKey([biber, nopkg]));
  });
  it('changes when a finding moves or is added', () => {
    const k = findingsKey([nopkg]);
    expect(findingsKey([{ ...nopkg, line: 5 }])).not.toBe(k);
    expect(findingsKey([nopkg, biber])).not.toBe(k);
  });
});

describe('shouldPop', () => {
  const k = findingsKey([nopkg, biber]);
  it('pops a new set on a user compile', () => {
    expect(shouldPop({ enabled: true, popup: true, key: k, lastShown: undefined })).toBe(true);
    expect(shouldPop({ enabled: true, popup: true, key: k, lastShown: findingsKey([nopkg]) })).toBe(
      true,
    );
  });
  it('does not pop the set already shown for the target', () => {
    expect(shouldPop({ enabled: true, popup: true, key: k, lastShown: k })).toBe(false);
  });
  it('never pops for auto-compile or warm-open', () => {
    expect(shouldPop({ enabled: true, popup: false, key: k, lastShown: undefined })).toBe(false);
  });
  it('never pops with the setting off or no findings', () => {
    expect(shouldPop({ enabled: false, popup: true, key: k, lastShown: undefined })).toBe(false);
    expect(shouldPop({ enabled: true, popup: true, key: '', lastShown: undefined })).toBe(false);
  });
});

describe('popup setting', () => {
  it('defaults on and round-trips through the store', () => {
    expect(parsePrecheckPopup(null)).toBe(true);
    expect(loadPrecheckPopup()).toBe(true);
    savePrecheckPopup(false);
    expect(loadPrecheckPopup()).toBe(false);
    savePrecheckPopup(true);
    expect(loadPrecheckPopup()).toBe(true);
  });
});
