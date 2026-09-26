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

  it('rejects a path outside the project root', () => {
    expect(() => trashName('/p', '/q/a.tex', 1)).toThrow(/not in project/);
    expect(() => trashName('/p', '/px/a.tex', 1)).toThrow(/not in project/);
  });
});
