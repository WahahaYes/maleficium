import { beforeEach, describe, expect, it, vi } from 'vitest';

const requestMock = vi.fn();
vi.mock('./core-request.tauri', () => ({ request: (...a: unknown[]) => requestMock(...a) }));

import {
  describeInstall,
  findingNeedsInstall,
  INTERACTIVE_PACKAGE,
  installInteractive,
  missingNeedsInstall,
} from './interactiveInstall';

beforeEach(() => requestMock.mockReset());

describe('findingNeedsInstall', () => {
  it('matches only the interactive package missing from the bundle', () => {
    const f = {
      kind: 'not-in-bundle',
      name: INTERACTIVE_PACKAGE,
      path: 'main.tex',
      line: 2,
    } as const;
    expect(findingNeedsInstall(f)).toBe(true);
    expect(findingNeedsInstall({ ...f, name: 'nopkg.sty' })).toBe(false);
    expect(findingNeedsInstall({ ...f, kind: 'system-font' })).toBe(false);
  });
});

describe('missingNeedsInstall', () => {
  it('matches only that file with the not-in-bundle reason', () => {
    expect(missingNeedsInstall({ file: INTERACTIVE_PACKAGE, reason: 'not-in-bundle' })).toBe(true);
    expect(missingNeedsInstall({ file: 'x.sty', reason: 'not-in-bundle' })).toBe(false);
    expect(missingNeedsInstall({ file: INTERACTIVE_PACKAGE, reason: 'fetch-failed' })).toBe(false);
    expect(missingNeedsInstall(null)).toBe(false);
  });
});

describe('installInteractive', () => {
  it('asks the backend with the overwrite flag it was given', async () => {
    requestMock.mockResolvedValue({ file: INTERACTIVE_PACKAGE, outcome: 'needs-confirmation' });
    const r = await installInteractive('root1', false);
    expect(requestMock).toHaveBeenCalledWith('interactiveInstall', {
      rootId: 'root1',
      overwrite: false,
    });
    expect(r.outcome).toBe('needs-confirmation');
    await installInteractive('root1', true);
    expect(requestMock).toHaveBeenLastCalledWith('interactiveInstall', {
      rootId: 'root1',
      overwrite: true,
    });
  });
});

describe('describeInstall', () => {
  it('says what happened', () => {
    expect(describeInstall('installed')).toContain('installed');
    expect(describeInstall('already-current')).toContain('up to date');
    expect(describeInstall('needs-confirmation')).toContain('confirm');
  });
});
