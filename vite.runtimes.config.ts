// Builds one built-in widget runtime into a self-contained index.html under
// src-tauri/widget-runtimes/<name>/, the generated runtime host a bundle's
// widgets/<id>/index.html is made from. Offline by construction: everything is
// inlined from node_modules, nothing is fetched. The output is committed;
// rerun `npm run build:runtimes` after editing one (src/widget-runtimes/
// fresh.test.ts fails on a stale build).
import { defineConfig } from 'vite';
import { viteSingleFile } from 'vite-plugin-singlefile';
import process from 'node:process';
import type { Plugin } from 'vite';

/** Every runtime, by its directory under src/widget-runtimes/. */
export const RUNTIMES = ['table', 'chart', 'model', 'video'];

/** Runtimes shipped as one classic script: no module graph, nothing that needs CORS. */
const CLASSIC = ['model', 'video'];

/** Turns the inlined module script into a classic one at the end of the body (it needs the DOM parsed). */
function classicScript(): Plugin {
  return {
    name: 'classic-script',
    generateBundle: {
      order: 'post',
      handler(_o, bundle) {
        for (const f of Object.values(bundle)) {
          if (f.type !== 'asset' || !f.fileName.endsWith('.html')) continue;
          const html = String(f.source);
          const m = /<script type="module" crossorigin>([\s\S]*?)<\/script>/.exec(html);
          if (!m) throw new Error(`${f.fileName}: no inlined module script to make classic`);
          const rest = html.slice(0, m.index) + html.slice(m.index + m[0].length);
          f.source = rest.replace('</body>', () => `<script>${m[1]}</script></body>`);
        }
      },
    },
  };
}

export default defineConfig(({ mode }) => {
  const runtime = process.env.RUNTIME ?? (RUNTIMES.includes(mode) ? mode : 'table');
  return {
    root: `src/widget-runtimes/${runtime}`,
    plugins: [viteSingleFile(), ...(CLASSIC.includes(runtime) ? [classicScript()] : [])],
    build: {
      outDir: `../../../src-tauri/widget-runtimes/${runtime}`,
      emptyOutDir: true,
      target: 'es2020',
      // vega and three.js are large; they ship inline in the one file by design.
      chunkSizeWarningLimit: 4000,
      rollupOptions: {
        input: `src/widget-runtimes/${runtime}/index.html`,
        ...(CLASSIC.includes(runtime) ? { output: { format: 'iife' as const } } : {}),
      },
    },
  };
});
