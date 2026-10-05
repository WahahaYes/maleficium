// The runtimes ship from committed builds. This fails when a runtime's source
// changed without `npm run build:runtimes`.
import { readFileSync } from 'node:fs';
import { build } from 'vite';
import { describe, expect, it } from 'vitest';
import { RUNTIMES } from '../../vite.runtimes.config';

type Output = { output: { fileName: string; source?: string; code?: string }[] };

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
      expect(
        committed === (html!.source ?? html!.code),
        'run npm run build:runtimes and commit the result',
      ).toBe(true);
    },
    60_000,
  );

  // The bridge ships as a classic script authors copy into their own
  // runtimes; like the built-ins it is committed and goes stale loudly.
  it('bridge/bridge.js matches a fresh build of bridge.ts', async () => {
    const out = (await build({
      configFile: 'vite.runtimes.config.ts',
      mode: 'bridge',
      logLevel: 'silent',
      build: { write: false },
    })) as unknown as Output[] | Output;
    const outputs = Array.isArray(out) ? out.flatMap((o) => o.output) : out.output;
    const js = outputs.find((o) => o.fileName === 'bridge.js');
    expect(js, 'the build produced bridge.js').toBeDefined();
    const committed = readFileSync('src-tauri/widget-runtimes/bridge/bridge.js', 'utf8');
    expect(
      committed === (js!.source ?? js!.code),
      'run npm run build:runtimes and commit the result',
    ).toBe(true);
  }, 60_000);
});
