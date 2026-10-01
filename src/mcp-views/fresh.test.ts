// The Views are embedded in the MCP server from committed builds, so cargo
// needs no node. This fails when a View's source changed without
// `npm run build:views`.
import { readFileSync } from 'node:fs';
import { build } from 'vite';
import { describe, expect, it } from 'vitest';
import { VIEWS } from '../../vite.views.config';

type Output = { output: { fileName: string; source: string }[] };

describe('committed MCP Views', () => {
  it.each(VIEWS)(
    '%s.html matches a fresh build of its source',
    async (view) => {
      const out = (await build({
        configFile: 'vite.views.config.ts',
        mode: view,
        logLevel: 'silent',
        build: { write: false },
      })) as unknown as Output[] | Output;
      const outputs = Array.isArray(out) ? out.flatMap((o) => o.output) : out.output;
      const html = outputs.find((o) => o.fileName.endsWith(`${view}.html`));
      expect(html, `the build produced ${view}.html`).toBeDefined();
      const committed = readFileSync(`src-tauri/mcp/views/${view}.html`, 'utf8');
      expect(committed === html!.source, 'run npm run build:views and commit the result').toBe(
        true,
      );
    },
    60_000,
  );
});
