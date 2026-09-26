import { describe, it, expect } from 'vitest';
import { trashName } from './file-history';

describe('trashName', () => {
  it('encodes the project-relative path and stamp', () => {
    expect(trashName('/p', '/p/sub/a.tex', 1234)).toBe('a.tex__sub__a.tex__1234');
    expect(trashName('/p/', '/p/main.tex', 7)).toBe('main.tex__main.tex__7');
  });

  it('matches the core format the MCP delete writes', () => {
    // Same literal as src-tauri/src/core/fs.rs `mcp_delete_lands_in_app_trash_dir`,
    // so core `undo_trash` can read rel back out of an app-trashed entry.
    const name = trashName('/home/u/paper', '/home/u/paper/sub/a.tex', 1700000000000);
    expect(name.startsWith('a.tex__sub__a.tex__')).toBe(true);
    expect(name.slice('a.tex__sub__a.tex__'.length)).toMatch(/^\d+$/);
  });

  it('escapes _ and % so __ is only a separator', () => {
    expect(trashName('/p', '/p/my__notes.tex', 1)).toBe('my%5F%5Fnotes.tex__my%5F%5Fnotes.tex__1');
    expect(trashName('/p', '/p/a__b/x.tex', 1)).toBe('x.tex__a%5F%5Fb__x.tex__1');
    expect(trashName('/p', '/p/_lead/trail_.tex', 1)).toBe(
      'trail%5F.tex__%5Flead__trail%5F.tex__1',
    );
    expect(trashName('/p', '/p/100%.tex', 1)).toBe('100%25.tex__100%25.tex__1');
    expect(trashName('/p', '/p/%5F.tex', 1)).toBe('%255F.tex__%255F.tex__1');
    // Same literal as src-tauri/src/core/fs.rs
    // `trash_name_round_trips_underscores_and_percent`, which decodes it.
    expect(trashName('/p', '/p/a__b/my__notes.tex', 1234)).toBe(
      'my%5F%5Fnotes.tex__a%5F%5Fb__my%5F%5Fnotes.tex__1234',
    );
  });

  it('rejects a path outside the project root', () => {
    expect(() => trashName('/p', '/q/a.tex', 1)).toThrow(/not in project/);
    expect(() => trashName('/p', '/px/a.tex', 1)).toThrow(/not in project/);
  });
});
