// The Views are embedded in the MCP server from committed builds, so cargo
// needs no node. This fails when a View's source changed without
// `npm run build:views`.
import { readFileSync } from 'node:fs';
import { build } from 'vite';
import { describe, expect, it } from 'vitest';

describe('committed MCP Views', () => {
  it('snippet.html matches a fresh build of its source', async () => {
    const out = (await build({
      configFile: 'vite.views.config.ts',
      logLevel: 'silent',
      build: { write: false },
    })) as unknown as
      | { output: { fileName: string; source: string }[] }[]
      | { output: { fileName: string; source: string }[] };
    const outputs = Array.isArray(out) ? out.flatMap((o) => o.output) : out.output;
    const html = outputs.find((o) => o.fileName.endsWith('snippet.html'));
    expect(html, 'the build produced snippet.html').toBeDefined();
    const committed = readFileSync('src-tauri/mcp/views/snippet.html', 'utf8');
    expect(committed === html!.source, 'run npm run build:views and commit the result').toBe(true);
  }, 60_000);
});
