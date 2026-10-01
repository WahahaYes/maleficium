// Builds one built-in widget runtime into a self-contained index.html under
// src-tauri/widget-runtimes/<name>/, the generated runtime host a bundle's
// widgets/<id>/index.html is made from. Offline by construction: everything is
// inlined from node_modules, nothing is fetched. The output is committed;
// rerun `npm run build:runtimes` after editing one (src/widget-runtimes/
// fresh.test.ts fails on a stale build).
import { defineConfig } from 'vite';
import { viteSingleFile } from 'vite-plugin-singlefile';
import process from 'node:process';

/** Every runtime, by its directory under src/widget-runtimes/. */
export const RUNTIMES = ['table', 'chart'];

export default defineConfig(({ mode }) => {
  const runtime = process.env.RUNTIME ?? (RUNTIMES.includes(mode) ? mode : 'table');
  return {
    root: `src/widget-runtimes/${runtime}`,
    plugins: [viteSingleFile()],
    build: {
      outDir: `../../../src-tauri/widget-runtimes/${runtime}`,
      emptyOutDir: true,
      target: 'es2020',
      // vega is large; it ships inline in the one file by design.
      chunkSizeWarningLimit: 4000,
      rollupOptions: { input: `src/widget-runtimes/${runtime}/index.html` },
    },
  };
});
