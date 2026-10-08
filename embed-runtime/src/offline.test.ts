// Offline pins: the runtimes ship as single files with every library inlined,
// so nothing may reference the network, and every vendored library must carry
// a permissive license (RULES section 5, recorded in NOTICE).
import { readFileSync, readdirSync, statSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';
import { RUNTIMES } from '../vite.config';

// URL-shaped strings that vendored libraries carry but never load: XML
// namespaces, a default `$schema` value, and a comment. Adding to this list is
// a review decision; a new URL in a built runtime fails the test below.
const INERT = new Set([
  'http://www.w3.org/2000/svg',
  'http://www.w3.org/2000/xmlns/',
  'http://www.w3.org/1999/xlink',
  'https://vega.github.io/schema/vega/v6.json',
  'https://github.com/vega/vega-lite/issues/2415',
  'http://www.w3.org/1999/xhtml',
  'https://jcgt.org/published/0007/04/01/', // a citation in three.js
]);
const URL_RE = /(?:https?|wss?|ftp):\/\/[^\s"'`)\\<>]+|\/\/[a-z0-9-]+(?:\.[a-z0-9-]+)+\//gi;
const externalUrls = (html: string) =>
  [...new Set((html.match(URL_RE) ?? []).map((u) => u.replace(/[.,;]+$/, '')))].filter(
    (u) => !INERT.has(u),
  );
/** Load positions in a built runtime: a tag that fetches, an @import, a worker, a refresh, a remote url(). */
function loads(html: string): string[] {
  const found: string[] = [];
  const tag =
    /<(?:script|img|iframe|link|source|video|audio|embed|object)\b[^>]*\b(?:src|href|data)=/gi;
  found.push(...(html.match(tag) ?? []));
  const rest =
    /@import|importScripts|new Worker\(|<meta[^>]+http-equiv=["']?refresh|url\(\s*["']?(?:https?:|\/\/)/gi;
  found.push(...(html.match(rest) ?? []));
  return found;
}
const built = (name: string) => readFileSync(`embed-runtime/built/${name}/index.html`, 'utf8');

function sourceFiles(dir: string): string[] {
  return readdirSync(dir).flatMap((f) => {
    const p = join(dir, f);
    if (statSync(p).isDirectory()) return sourceFiles(p);
    return /\.(ts|html|css)$/.test(f) && !f.endsWith('.test.ts') ? [p] : [];
  });
}

describe('no external URLs', () => {
  it('first-party runtime source has none', () => {
    for (const f of sourceFiles('embed-runtime/src')) {
      expect(readFileSync(f, 'utf8').match(URL_RE) ?? [], f).toEqual([]);
    }
  });

  it.each(RUNTIMES)('the built %s runtime has no URL beyond the inert allowlist', (name) => {
    expect(externalUrls(built(name)), `unexpected URL in ${name}`).toEqual([]);
  });

  it.each(RUNTIMES)(
    'the built %s runtime loads nothing: no src, href, import or worker',
    (name) => {
      expect(loads(built(name)), name).toEqual([]);
    },
  );

  it.each(['model', 'video'])('the built %s runtime is one classic script', (name) => {
    const html = built(name);
    expect(html.match(/<script\b[^>]*>/g)).toEqual(['<script>']);
    expect(html).not.toMatch(/type=["']module["']|\bimport\s*\(|\bimport\.meta/);
  });

  // Red control: the same checks must fail a runtime that pulls a library from a CDN.
  it('red control: a runtime that loads three.js from a CDN trips both greps', () => {
    const bad = built('video').replace(
      '<body>',
      '<body><script src="https://cdn.jsdelivr.net/npm/three@0.186.1/build/three.module.js"></script>',
    );
    expect(externalUrls(bad)).toContain(
      'https://cdn.jsdelivr.net/npm/three@0.186.1/build/three.module.js',
    );
    expect(loads(bad).length).toBeGreaterThan(0);
    expect(loads('<link rel="stylesheet" href="x.css"><style>@import "y.css"</style>').length).toBe(
      2,
    );
  });
});

describe('vendored licenses', () => {
  // RULES section 5 allows permissive licenses; this is the runtime-contract allowlist.
  const ALLOWED = new Set([
    'MIT',
    'MIT-0',
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

  /** Every package reachable from the roots through the lockfile's dependencies. */
  function closure(roots: string[]): Map<string, string> {
    const seen = new Map<string, string>();
    const queue = [...roots];
    while (queue.length) {
      const name = queue.pop()!;
      if (seen.has(name)) continue;
      const entry = lock.packages[`node_modules/${name}`];
      expect(entry, `${name} is in the lockfile`).toBeDefined();
      seen.set(name, entry.license ?? 'MISSING');
      queue.push(...Object.keys(entry.dependencies ?? {}));
    }
    return seen;
  }

  it('the vega, vega-lite and vega-interpreter closure is permissive and complete', () => {
    const seen = closure(['vega', 'vega-lite', 'vega-interpreter']);
    expect(seen.size).toBeGreaterThan(50);
    const bad = [...seen].filter(([, l]) => !ALLOWED.has(l));
    expect(bad, 'packages outside the permissive allowlist').toEqual([]);
  });

  it('three.js is MIT, has no dependencies, and its types closure is permissive', () => {
    expect(closure(['three'])).toEqual(new Map([['three', 'MIT']]));
    // @types/three is build-time only (never inlined); its closure is checked all the same.
    const types = closure(['@types/three']);
    expect(types.size).toBeGreaterThan(3);
    expect([...types].filter(([, l]) => !ALLOWED.has(l))).toEqual([]);
  });

  it('the lockfile pins the declared runtime libraries', () => {
    const pkg = JSON.parse(readFileSync('package.json', 'utf8')) as {
      devDependencies: Record<string, string>;
    };
    for (const dep of ['vega', 'vega-lite', 'vega-interpreter', 'three', '@types/three']) {
      expect(pkg.devDependencies[dep], dep).toBeDefined();
    }
  });
});
