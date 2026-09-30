import { describe, it, expect, vi } from 'vitest';
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
import { invoke } from '@tauri-apps/api/core';
import { setStructure, structure } from './structure';
import { desktopStructure } from './structure.tauri';

describe('structure seam', () => {
  it('desktop impl forwards text to the Rust commands', async () => {
    setStructure(desktopStructure);
    vi.mocked(invoke).mockResolvedValueOnce({
      op: 'structureOutline',
      result: { entries: [], truncated: 0 },
    });
    await structure().outline('\\section{A}');
    expect(invoke).toHaveBeenLastCalledWith('core_request', {
      req: { op: 'structureOutline', params: { text: '\\section{A}' } },
    });
    vi.mocked(invoke).mockResolvedValueOnce({ op: 'structureDiagnostics', result: [] });
    await structure().diagnostics('log', '/r', '/r/sub');
    expect(invoke).toHaveBeenLastCalledWith('core_request', {
      req: {
        op: 'structureDiagnostics',
        params: {
          log: 'log',
          root: '/r',
          base: '/r/sub',
        },
      },
    });
  });
});
