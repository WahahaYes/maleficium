// The licence boundary: embed-runtime/ is MIT-0 and ships inside papers and
// projects, so nothing in it may import from the AGPL app around it. A
// relative import or a local src/href must resolve inside embed-runtime/;
// bare specifiers are npm packages, whose licences offline.test.ts pins.
import { readFileSync, readdirSync, statSync } from 'node:fs';
import { dirname, join, relative, resolve, sep } from 'node:path';
import { describe, expect, it } from 'vitest';

const ROOT = resolve('embed-runtime');

function files(dir: string): string[] {
  return readdirSync(dir).flatMap((f) => {
    const p = join(dir, f);
    if (statSync(p).isDirectory()) return f === 'built' ? [] : files(p);
    return /\.(ts|mts|js|mjs|html)$/.test(f) ? [p] : [];
  });
}

/** Module specifiers and local resource references in one file. */
function references(text: string): string[] {
  const found: string[] = [];
  const module =
    /(?:\bimport\s+(?:[^'"`;]*?\sfrom\s+)?|\bexport\s+[^'"`;]*?\sfrom\s+|\bimport\s*\(\s*|\brequire\s*\(\s*)['"]([^'"]+)['"]/g;
  for (const m of text.matchAll(module)) found.push(m[1]);
  for (const m of text.matchAll(/\b(?:src|href)=["']([^"']+)["']/g)) found.push(m[1]);
  return found;
}

const local = (spec: string) =>
  spec.startsWith('.') || (spec.startsWith('/') && !spec.startsWith('//'));

describe('embed-runtime imports nothing from outside it', () => {
  const all = files(ROOT);

  it('scans the sources, samples and reader', () => {
    const rel = all.map((f) => relative(ROOT, f));
    expect(rel).toContain(join('src', 'bridge.ts'));
    expect(rel).toContain(join('reader', 'reader.js'));
    expect(rel.some((f) => f.startsWith(`samples${sep}`))).toBe(true);
  });

  it.each(all.map((f) => [relative(ROOT, f), f]))('%s', (_name, file) => {
    const outside = references(readFileSync(file, 'utf8'))
      .filter(local)
      .filter((spec) => {
        const target = spec.startsWith('/') ? spec : resolve(dirname(file), spec);
        return !(target === ROOT || target.startsWith(ROOT + sep));
      });
    expect(outside, 'move the code into embed-runtime/ or drop the import').toEqual([]);
  });
});
