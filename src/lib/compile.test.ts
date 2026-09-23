import { describe, expect, it } from 'vitest';
import { describeMissing, offlineBadge } from './compile';
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

describe('offlineBadge', () => {
  it('reads the three headline states the way the badge shows them', () => {
    expect(offlineBadge({ state: 'ready', needs: [], missing: null }).label).toBe('Ready offline');
    expect(
      offlineBadge({
        state: 'needs-network',
        needs: ['booktabs.sty'],
        missing: { file: 'booktabs.sty', reason: 'not-cached' },
      }).label,
    ).toBe('Needs network for: booktabs.sty');
    const biber = offlineBadge({
      state: 'needs-tool',
      needs: ['biber'],
      missing: { file: 'biber', reason: 'external-tool' },
    });
    expect(biber.label).toBe('Needs biber');
    expect(biber.title).toContain('backend=bibtex');
  });

  it('tones blocked as an error and unverified as neutral', () => {
    expect(offlineBadge({ state: 'blocked', needs: ['x.sty'], missing: null }).tone).toBe('error');
    expect(offlineBadge({ state: 'unverified', needs: [], missing: null }).tone).toBe('neutral');
  });
});
