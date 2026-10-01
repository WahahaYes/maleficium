// Offline pins: the runtimes ship as single files with every library inlined,
// so nothing may reference the network, and every vendored library must carry
// a permissive license (RULES section 5, recorded in NOTICE).
import { readFileSync, readdirSync, statSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';
import { RUNTIMES } from '../../vite.runtimes.config';

// URL-shaped strings that vendored libraries carry but never load: XML
// namespaces, a default `$schema` value, and a comment. Adding to this list is
// a review decision; a new URL in a built runtime fails the test below.
const INERT = new Set([
  'http://www.w3.org/2000/svg',
  'http://www.w3.org/2000/xmlns/',
  'http://www.w3.org/1999/xlink',
  'https://vega.github.io/schema/vega/v6.json',
  'https://github.com/vega/vega-lite/issues/2415',
]);
const URL_RE = /(?:https?|wss?|ftp):\/\/[^\s"'`)\\<>]+|\/\/[a-z0-9-]+(?:\.[a-z0-9-]+)+\//gi;
const built = (name: string) =>
  readFileSync(`src-tauri/widget-runtimes/${name}/index.html`, 'utf8');

function sourceFiles(dir: string): string[] {
  return readdirSync(dir).flatMap((f) => {
    const p = join(dir, f);
    if (statSync(p).isDirectory()) return sourceFiles(p);
    return /\.(ts|html|css)$/.test(f) && !f.endsWith('.test.ts') ? [p] : [];
  });
}

describe('no external URLs', () => {
  it('first-party runtime source has none', () => {
    for (const f of sourceFiles('src/widget-runtimes')) {
      expect(readFileSync(f, 'utf8').match(URL_RE) ?? [], f).toEqual([]);
    }
  });

  it.each(RUNTIMES)('the built %s runtime has no URL beyond the inert allowlist', (name) => {
    const found = new Set((built(name).match(URL_RE) ?? []).map((u) => u.replace(/[.,;]+$/, '')));
    for (const u of found) expect(INERT.has(u), `unexpected URL in ${name}: ${u}`).toBe(true);
  });

  it.each(RUNTIMES)(
    'the built %s runtime loads nothing: no src, href, import or worker',
    (name) => {
      const html = built(name);
      expect(html).not.toMatch(
        /<(?:script|img|iframe|link|source|video|audio|embed|object)\b[^>]*\b(?:src|href|data)=/i,
      );
      expect(html).not.toMatch(
        /@import|importScripts|new Worker\(|<meta[^>]+http-equiv=["']?refresh/i,
      );
      expect(html).not.toMatch(/url\(\s*["']?(?:https?:|\/\/)/i);
    },
  );
});

describe('vendored licenses', () => {
  // RULES section 5 allows permissive licenses; this is the runtime-contract allowlist.
  const ALLOWED = new Set([
    'MIT',
    'BSD-2-Clause',
    'BSD-3-Clause',
    'Apache-2.0',
    'ISC',
    'Zlib',
    '0BSD',
    'CC0-1.0',
    'Unlicense',
  ]);
  type Lock = {
    packages: Record<string, { license?: string; dependencies?: Record<string, string> }>;
  };
  const lock = JSON.parse(readFileSync('package-lock.json', 'utf8')) as Lock;

  it('the vega, vega-lite and vega-interpreter closure is permissive and complete', () => {
    const seen = new Map<string, string>();
    const queue = ['vega', 'vega-lite', 'vega-interpreter'];
    while (queue.length) {
      const name = queue.pop()!;
      if (seen.has(name)) continue;
      const entry = lock.packages[`node_modules/${name}`];
      expect(entry, `${name} is in the lockfile`).toBeDefined();
      seen.set(name, entry.license ?? 'MISSING');
      queue.push(...Object.keys(entry.dependencies ?? {}));
    }
    expect(seen.size).toBeGreaterThan(50);
    const bad = [...seen].filter(([, l]) => !ALLOWED.has(l));
    expect(bad, 'packages outside the permissive allowlist').toEqual([]);
  });

  it('the lockfile pins the declared runtime libraries', () => {
    const pkg = JSON.parse(readFileSync('package.json', 'utf8')) as {
      devDependencies: Record<string, string>;
    };
    for (const dep of ['vega', 'vega-lite', 'vega-interpreter']) {
      expect(pkg.devDependencies[dep], dep).toBeDefined();
    }
  });
});
