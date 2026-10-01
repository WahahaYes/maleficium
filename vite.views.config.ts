// Builds each MCP App View into one self-contained html file under
// src-tauri/mcp/views/, which maleficium-mcp embeds with include_str!. The
// output is committed so cargo builds need no node: rerun after editing a
// View (`npm run build:views`); scripts/check-freshness.sh and the mcp tests
// fail on a stale build.
import { defineConfig } from 'vite';
import { viteSingleFile } from 'vite-plugin-singlefile';
import process from 'node:process';

const view = process.env.VIEW ?? 'snippet';

export default defineConfig({
  root: `src/mcp-views/${view}`,
  plugins: [viteSingleFile()],
  build: {
    outDir: '../../../src-tauri/mcp/views',
    emptyOutDir: false,
    target: 'es2020',
    rollupOptions: { input: `src/mcp-views/${view}/${view}.html` },
  },
});
