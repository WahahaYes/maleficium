// themeCatalog.test.ts — vendored index is complete and unique.

import { describe, expect, it } from 'vitest';
import { THEME_CATALOG } from './themeCatalog';
import { parseVscodeTheme, resolveTier1 } from './vscodeTheme';

describe('theme catalog', () => {
  it('lists ten bundled themes with unique ids and parseable sources', () => {
    expect(THEME_CATALOG).toHaveLength(10);
    expect(new Set(THEME_CATALOG.map((t) => t.id)).size).toBe(10);
    for (const t of THEME_CATALOG) {
      expect(t.source.length).toBeGreaterThan(1000);
      const t1 = resolveTier1(parseVscodeTheme(t.source));
      expect(t1.isDark).toBe(t.type === 'dark');
      expect(t1.background).toMatch(/^#/);
    }
  });
});
