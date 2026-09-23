import { describe, it, expect, vi, beforeEach } from 'vitest';
import { getMainFileFor, setMainFileFor } from './mainFile.store';
import { hashRoot } from './paths';
import { matchesCompile, matchesForwardSync, menuChordId, zoomChord, KEYMAP } from './keymap';

import { setAppStore } from './app-store';
import { localAppStore } from './app-store.web';

setAppStore(localAppStore);

const store: Record<string, string> = {};
vi.stubGlobal('localStorage', {
  getItem: (k: string) => store[k] ?? null,
  setItem: (k: string, v: string) => {
    store[k] = v;
  },
  removeItem: (k: string) => {
    delete store[k];
  },
});

beforeEach(() => {
  for (const k of Object.keys(store)) delete store[k];
});

function keyEvent(init: Partial<KeyboardEvent> & { key: string }): KeyboardEvent {
  return {
    ctrlKey: false,
    metaKey: false,
    shiftKey: false,
    altKey: false,
    target: null,
    ...init,
  } as KeyboardEvent;
}

describe('main-file store round-trip', () => {
  it('persists the explicit association per project root', () => {
    const id = hashRoot('/r');
    expect(getMainFileFor(id)).toBeNull();
    setMainFileFor(id, 'main.tex');
    expect(getMainFileFor(id)).toBe('main.tex');
    setMainFileFor(id, 'ch/main.tex');
    expect(getMainFileFor(id)).toBe('ch/main.tex');
  });
  it('survives corrupt storage', () => {
    store['maleficium.mainFile.v1'] = '{nope';
    expect(getMainFileFor(hashRoot('/r'))).toBeNull();
  });
});

describe('keymap chords', () => {
  it('matches compile and forward-sync chords', () => {
    expect(matchesCompile(keyEvent({ key: 'r', ctrlKey: true }))).toBe(true);
    expect(matchesCompile(keyEvent({ key: 'r', ctrlKey: true, shiftKey: true }))).toBe(false);
    expect(matchesForwardSync(keyEvent({ key: 'j', ctrlKey: true, altKey: true }))).toBe(true);
    expect(matchesForwardSync(keyEvent({ key: 'F', ctrlKey: true, shiftKey: true }))).toBe(false);
    expect(matchesForwardSync(keyEvent({ key: 'j' }))).toBe(false);
  });
  it('maps menu chords to registry ids', () => {
    expect(menuChordId(keyEvent({ key: 'o', ctrlKey: true }))).toBe('file.open-project');
    expect(menuChordId(keyEvent({ key: 's', ctrlKey: true }))).toBe('file.save');
    expect(menuChordId(keyEvent({ key: 'g', ctrlKey: true }))).toBe('selection.go-to-line');
    expect(menuChordId(keyEvent({ key: 'p', ctrlKey: true }))).toBe('file.quick-open');
    expect(menuChordId(keyEvent({ key: 'P', ctrlKey: true, shiftKey: true }))).toBe(
      'view.command-palette',
    );
    expect(menuChordId(keyEvent({ key: 'F', ctrlKey: true, shiftKey: true }))).toBe(
      'search.find-in-project',
    );
    expect(menuChordId(keyEvent({ key: 'f', ctrlKey: true }))).toBe('edit.find');
    expect(menuChordId(keyEvent({ key: 'x', ctrlKey: true }))).toBeNull();
  });
  it('chord table stays unique and labeled', () => {
    const ids = KEYMAP.map((k) => k.id);
    expect(new Set(ids).size).toBe(ids.length);
    for (const k of KEYMAP) expect(k.keys.length).toBeGreaterThan(0);
  });
});

describe('zoom chords', () => {
  it('maps Ctrl+= / Ctrl++ / Ctrl+- / Ctrl+0 and ignores bare or Alt keys', () => {
    expect(zoomChord(keyEvent({ key: '=', ctrlKey: true }))).toBe('in');
    expect(zoomChord(keyEvent({ key: '+', ctrlKey: true, shiftKey: true }))).toBe('in');
    expect(zoomChord(keyEvent({ key: '-', ctrlKey: true }))).toBe('out');
    expect(zoomChord(keyEvent({ key: '0', ctrlKey: true }))).toBe('fit-width');
    expect(zoomChord(keyEvent({ key: '=' }))).toBeNull();
    expect(zoomChord(keyEvent({ key: '-', ctrlKey: true, altKey: true }))).toBeNull();
  });
});
