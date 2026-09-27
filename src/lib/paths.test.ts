import { describe, it, expect } from 'vitest';
import {
  appTrashDir,
  baseName,
  dialogStart,
  dirName,
  hasDir,
  isAbsolutePath,
  joinPath,
  relUnder,
  safeName,
} from './paths';
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

// Every helper runs on both native forms on every host: a path's own prefix
// picks its rules, so Windows cases need no Windows machine.
describe('native path forms', () => {
  it('knows absolute paths in either form', () => {
    for (const p of ['/home/u', 'C:\\Users\\u', 'c:/Users/u', '\\\\srv\\share']) {
      expect(isAbsolutePath(p), p).toBe(true);
    }
    for (const p of ['main.tex', 'sub/main.tex', 'C:main.tex', '..\\x.tex']) {
      expect(isAbsolutePath(p), p).toBe(false);
    }
  });
  it('splits base and dir by the path form', () => {
    expect(baseName('/home/u/p/main.tex')).toBe('main.tex');
    expect(dirName('/home/u/p/main.tex')).toBe('/home/u/p');
    expect(baseName('C:\\Users\\u\\p\\main.tex')).toBe('main.tex');
    expect(dirName('C:\\Users\\u\\p\\main.tex')).toBe('C:\\Users\\u\\p');
    expect(baseName('C:\\p/sub/a.tex')).toBe('a.tex');
    // `\` is a legal Unix name character, never a separator there.
    expect(baseName('/home/u/a\\b.tex')).toBe('a\\b.tex');
    expect(baseName('untitled.tex')).toBe('untitled.tex');
    expect(dirName('untitled.tex')).toBe('');
  });
  it('tells a located file from a bare untitled name', () => {
    expect(hasDir('/p/a.tex')).toBe(true);
    expect(hasDir('C:\\p\\a.tex')).toBe(true);
    expect(hasDir('untitled.tex')).toBe(false);
  });
  it('joins in the base path form, converting rels on Windows', () => {
    expect(joinPath('/home/u/p', 'sub/a.tex')).toBe('/home/u/p/sub/a.tex');
    expect(joinPath('C:\\Users\\u\\p', 'sub/a.tex')).toBe('C:\\Users\\u\\p\\sub\\a.tex');
    expect(joinPath('C:\\', 'a.tex')).toBe('C:\\a.tex');
    expect(joinPath('\\\\srv\\share', 'p', 'a.tex')).toBe('\\\\srv\\share\\p\\a.tex');
  });
  it('rebases onto a root as a /-rel, or null outside it', () => {
    expect(relUnder('/home/u/p', '/home/u/p/sub/a.tex')).toBe('sub/a.tex');
    expect(relUnder('/home/u/p', '/home/u/paper/a.tex')).toBeNull();
    expect(relUnder('/', '/etc/x')).toBe('etc/x');
    const win = 'C:\\Users\\u\\p';
    expect(relUnder(win, 'C:\\Users\\u\\p\\sub\\a.tex')).toBe('sub/a.tex');
    expect(relUnder(win, 'C:/Users/u/p/sub/a.tex')).toBe('sub/a.tex');
    expect(relUnder(win, 'c:\\users\\U\\P\\a.tex')).toBe('a.tex');
    expect(relUnder(win, 'C:\\Users\\u\\paper\\a.tex')).toBeNull();
    expect(relUnder(win, 'D:\\Users\\u\\p\\a.tex')).toBeNull();
    expect(relUnder('', '/a')).toBeNull();
  });
  it('dialogs keep Windows absolutes and anchor rels at a Windows home', () => {
    expect(dialogStart('D:\\talks', 'C:\\Users\\u')).toBe('D:\\talks');
    expect(dialogStart('main.pdf', 'C:\\Users\\u')).toBe('C:\\Users\\u\\main.pdf');
  });
  it('typed names never carry a separator of either form', () => {
    expect(safeName(' a/b\\c.tex ')).toBe('a_b_c.tex');
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
