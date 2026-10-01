import { describe, it, expect, vi } from 'vitest';
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
import { invoke } from '@tauri-apps/api/core';
import { resolveMainFileTauri, setMainFile } from './mainFile.tauri';
import { exportBundle } from './compile';
import { matchesCompile, matchesForwardSync, menuChordId, zoomChord, KEYMAP } from './keymap';

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

describe('main-file adapter', () => {
  it('resolves through the core contract', async () => {
    vi.mocked(invoke).mockResolvedValueOnce({
      op: 'mainResolve',
      result: { main: '/r/main.tex', source: 'scan', candidates: [] },
    });
    const res = await resolveMainFileTauri('1a2b3c4d', '/r/ch.tex');
    expect(invoke).toHaveBeenLastCalledWith('core_request', {
      req: {
        op: 'mainResolve',
        params: { rootId: '1a2b3c4d', openedAbs: '/r/ch.tex' },
      },
    });
    expect(res).toMatchObject({ mainFile: '/r/main.tex', source: 'scan' });
  });
  it('stores the association as a rel under the grant root id', async () => {
    vi.mocked(invoke).mockResolvedValueOnce({ op: 'mainSetAssociation', result: null });
    // The frontend never hashes: it passes the id the grant returned, even
    // for non-ASCII roots (UTF-8 backend hash of '/home/josé/thèse').
    await setMainFile('53bf67b2', '/home/josé/thèse', '/home/josé/thèse/chapitres/thèse.tex');
    expect(invoke).toHaveBeenLastCalledWith('core_request', {
      req: {
        op: 'mainSetAssociation',
        params: { rootId: '53bf67b2', rel: 'chapitres/thèse.tex' },
      },
    });
  });
});

describe('paper bundle export adapter', () => {
  it('sends the profile and cap and nothing that could approve a download', async () => {
    const done = {
      path: '/out/p',
      profile: 'folder',
      bytes: 10,
      widgets: 1,
      assets: 2,
      warnings: [],
    };
    vi.mocked(invoke).mockResolvedValueOnce({ op: 'exportBundle', result: done });
    const r = await exportBundle('1a2b3c4d', 'main.tex', '/out/p', 'folder');
    expect(invoke).toHaveBeenLastCalledWith('core_request', {
      req: {
        op: 'exportBundle',
        params: {
          rootId: '1a2b3c4d',
          mainRel: 'main.tex',
          dest: '/out/p',
          profile: 'folder',
          sizeCapBytes: null,
        },
      },
    });
    expect(r).toEqual(done);
    // Red control: the core, not the adapter, refuses a destination inside
    // the project, so a refusal comes back as a rejected call.
    vi.mocked(invoke).mockRejectedValueOnce('export destination is inside the project');
    await expect(exportBundle('1a2b3c4d', 'main.tex', '/r/x', 'single-file')).rejects.toMatch(
      /inside the project/,
    );
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
