import { describe, it, expect } from 'vitest';
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join } from 'node:path';

const SRC = join(__dirname, '..');

/** Every non-test module under src/. */
function sources(dir = SRC): string[] {
  return readdirSync(dir).flatMap((name) => {
    const p = join(dir, name);
    if (statSync(p).isDirectory()) return name === 'test' ? [] : sources(p);
    return /\.tsx?$/.test(name) && !/\.test\.tsx?$/.test(name) ? [p] : [];
  });
}

describe('outdir containment', () => {
  it('no frontend module derives, names, or reads engine outputs', () => {
    const offenders = sources().flatMap((p) => {
      const text = readFileSync(p, 'utf8');
      return [/appCacheDir/, /maleficium-out/, /['"`]\/tmp/]
        .filter((re) => re.test(text))
        .map((re) => `${p.slice(SRC.length + 1)}: ${re}`);
    });
    expect(offenders).toEqual([]);
  });

  it('scans the real tree', () => {
    expect(sources().length).toBeGreaterThan(20);
  });
});
