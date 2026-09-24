import { describe, it, expect } from 'vitest';
import { hashRoot, joinPath, appTrashDir, dialogStart } from './paths';
import { previewKindFor, isPreviewable } from './files';

describe('app-local paths', () => {
  it('hashRoot is deterministic 8-hex', () => {
    expect(hashRoot('/home/u/paper')).toBe(hashRoot('/home/u/paper'));
    expect(hashRoot('/home/u/paper')).toMatch(/^[0-9a-f]{8}$/);
    expect(hashRoot('/home/u/other')).not.toBe(hashRoot('/home/u/paper'));
  });
  it('trash dirs shard per root under the app home', () => {
    const t = appTrashDir('/app/data', '/home/u/paper');
    expect(t.startsWith('/app/data/maleficium-trash/')).toBe(true);
    // Same root → same shard; different roots → different shards.
    expect(appTrashDir('/app/data', '/home/u/other')).not.toBe(t);
    // Never inside the project dir.
    expect(t.startsWith('/home/u/paper')).toBe(false);
  });
  it('dialogs start at home unless given an absolute path', () => {
    expect(dialogStart(undefined, '/home/u')).toBe('/home/u');
    expect(dialogStart('', '/home/u')).toBe('/home/u');
    expect(dialogStart('main.pdf', '/home/u')).toBe('/home/u/main.pdf');
    expect(dialogStart('/work/talks', '/home/u')).toBe('/work/talks');
  });
  it('joinPath collapses duplicate slashes', () => {
    expect(joinPath('/a/', '/b', 'c')).toBe('/a/b/c');
  });
});

describe('preview classification', () => {
  it('routes images, video, pdf away from the editor', () => {
    expect(previewKindFor('/r/figs/diagram.png')).toBe('image');
    expect(previewKindFor('/r/clip.MP4')).toBe('video');
    expect(previewKindFor('/r/paper.pdf')).toBe('pdf');
    expect(isPreviewable('/r/figs/diagram.png')).toBe(true);
  });
  it('keeps text editable, unknowns honest binary', () => {
    expect(previewKindFor('/r/main.tex')).toBe('text');
    expect(isPreviewable('/r/main.tex')).toBe(false);
    expect(previewKindFor('/r/refs.bib')).toBe('text');
    expect(previewKindFor('/r/notes.xyz')).toBe('binary');
    expect(isPreviewable('/r/notes.xyz')).toBe(true);
    expect(previewKindFor('/r/noext')).toBe('text');
    expect(isPreviewable('/r/noext')).toBe(false);
  });
});
