import { describe, it, expect } from 'vitest';
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join } from 'node:path';

const SRC = join(__dirname, '..');

function sources(dir = SRC): string[] {
  return readdirSync(dir).flatMap((name) => {
    const p = join(dir, name);
    if (statSync(p).isDirectory()) return name === 'test' ? [] : sources(p);
    return /\.tsx?$/.test(name) && !/\.test\.tsx?$/.test(name) ? [p] : [];
  });
}

// A ref created with a placeholder function is a forward reference: code
// defined earlier calls it, and it is filled in once the real function
// exists. If that assignment is lost, every caller silently does nothing
// (cross-file jumps did, from a refactor until 2026-09-28).
describe('placeholder function refs', () => {
  it('are each assigned a real function', () => {
    const files = sources().map((f) => [f, readFileSync(f, 'utf8')] as const);
    // A hook may fill a ref the component passes in, so look in every file.
    const all = files.map(([, t]) => t).join('\n');
    const missing: string[] = [];
    for (const [file, text] of files) {
      const decl = /const (\w+Ref) = useRef<[^;]*?>\(\s*(?:async\s*)?\(\)\s*=>\s*\{\s*\}\s*\)/g;
      for (const m of text.matchAll(decl)) {
        if (!new RegExp(`\\b${m[1]}\\.current\\s*=[^=]`).test(all)) {
          missing.push(`${file.slice(SRC.length + 1)}: ${m[1]}`);
        }
      }
    }
    expect(missing).toEqual([]);
  });
});
