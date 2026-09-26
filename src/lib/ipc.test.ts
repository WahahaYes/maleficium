import { describe, it, expect, vi, beforeEach } from 'vitest';
import { getMainFileFor, setMainFileFor } from './mainFile.store';
import { resolveMainFileTauri, setMainFile } from './mainFile.tauri';
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
    const id = '1a2b3c4d';
    expect(getMainFileFor(id)).toBeNull();
    setMainFileFor(id, 'main.tex');
    expect(getMainFileFor(id)).toBe('main.tex');
    setMainFileFor(id, 'ch/main.tex');
    expect(getMainFileFor(id)).toBe('ch/main.tex');
  });
  it('survives corrupt storage', () => {
    store['maleficium.mainFile.v1'] = '{nope';
    expect(getMainFileFor('1a2b3c4d')).toBeNull();
  });
  it('keys the association by the grant root id, even for non-ASCII roots', async () => {
    // hash_root('/home/josé/thèse') in the backend (UTF-8 bytes). A UTF-16
    // frontend hash gave 6ea6b60c, so the frontend never hashes: it passes
    // the id the grant returned.
    const root = '/home/josé/thèse';
    const rootId = '53bf67b2';
    await setMainFile(rootId, root, root + '/chapitres/thèse.tex');
    expect(getMainFileFor(rootId)).toBe('chapitres/thèse.tex');
    expect(getMainFileFor('6ea6b60c')).toBeNull();
    const res = await resolveMainFileTauri(root, rootId, null);
    expect(res).toMatchObject({ mainFile: root + '/chapitres/thèse.tex', source: 'config' });
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
