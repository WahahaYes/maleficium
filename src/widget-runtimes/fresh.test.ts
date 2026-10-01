// The runtimes ship from committed builds. This fails when a runtime's source
// changed without `npm run build:runtimes`.
import { readFileSync } from 'node:fs';
import { build } from 'vite';
import { describe, expect, it } from 'vitest';
import { RUNTIMES } from '../../vite.runtimes.config';

type Output = { output: { fileName: string; source: string }[] };

describe('committed widget runtimes', () => {
  it.each(RUNTIMES)(
    '%s/index.html matches a fresh build of its source',
    async (name) => {
      const out = (await build({
        configFile: 'vite.runtimes.config.ts',
        mode: name,
        logLevel: 'silent',
        build: { write: false },
      })) as unknown as Output[] | Output;
      const outputs = Array.isArray(out) ? out.flatMap((o) => o.output) : out.output;
      const html = outputs.find((o) => o.fileName.endsWith('index.html'));
      expect(html, `the build produced index.html for ${name}`).toBeDefined();
      const committed = readFileSync(`src-tauri/widget-runtimes/${name}/index.html`, 'utf8');
      expect(committed === html!.source, 'run npm run build:runtimes and commit the result').toBe(
        true,
      );
    },
    60_000,
  );
});
