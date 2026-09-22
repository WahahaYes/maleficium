import { describe, expect, it } from 'vitest';
import { describeMissing } from './compile';
import type { MissingReason } from './generated/structure';

const REASONS: MissingReason[] = [
  'not-cached',
  'fetch-failed',
  'not-in-bundle',
  'bundle-unreachable',
  'bundle-invalid',
  'cache-empty',
  'bundle-changed',
  'system-font',
  'external-tool',
  'shell-escape-required',
];

describe('describeMissing', () => {
  it('names the file for every file-bearing reason', () => {
    for (const reason of REASONS) {
      const text = describeMissing({ file: 'booktabs.sty', reason });
      expect(text.length).toBeGreaterThan(0);
      if (!reason.startsWith('bundle-') && reason !== 'cache-empty') {
        expect(text, reason).toContain('booktabs.sty');
      }
    }
  });

  it('tells apart fixable-by-network from not-fixable-by-network', () => {
    expect(describeMissing({ file: 'x.sty', reason: 'not-in-bundle' })).toMatch(/cannot help/);
    expect(describeMissing({ file: 'x.sty', reason: 'not-cached' })).toMatch(/network/);
    expect(describeMissing({ reason: 'cache-empty' })).toMatch(/needs network/);
  });

  it('steers biber users to the bundled bibtex backend', () => {
    expect(describeMissing({ file: 'biber', reason: 'external-tool' })).toContain('backend=bibtex');
    expect(describeMissing({ file: 'pygmentize', reason: 'external-tool' })).not.toContain(
      'bibtex',
    );
  });
});
