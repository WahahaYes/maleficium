import { describe, it, expect } from 'vitest';
import { joinPath, appTrashDir, dialogStart } from './paths';
import { previewKindFor, isPreviewable } from './files';

describe('app-local paths', () => {
  it('trash dirs shard by the grant root id under the app home', () => {
    // The backend's id is used verbatim: the frontend never hashes paths.
    expect(appTrashDir('/app/data', '53bf67b2')).toBe('/app/data/maleficium-trash/53bf67b2');
    expect(appTrashDir('/app/data', '1a2b3c4d')).not.toBe(appTrashDir('/app/data', '53bf67b2'));
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
