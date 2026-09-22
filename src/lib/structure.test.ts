import { describe, it, expect, vi } from 'vitest';
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
import { invoke } from '@tauri-apps/api/core';
import { setStructure, structure } from './structure';
import { desktopStructure } from './structure.tauri';

describe('structure seam', () => {
  it('desktop impl forwards text to the Rust commands', async () => {
    setStructure(desktopStructure);
    vi.mocked(invoke).mockResolvedValueOnce({ entries: [], truncated: 0 });
    await structure().outline('\\section{A}');
    expect(invoke).toHaveBeenLastCalledWith('structure_outline', { text: '\\section{A}' });
    vi.mocked(invoke).mockResolvedValueOnce([]);
    await structure().diagnostics('log', '/r', '/r/sub');
    expect(invoke).toHaveBeenLastCalledWith('structure_diagnostics', {
      log: 'log',
      root: '/r',
      base: '/r/sub',
    });
  });
});
