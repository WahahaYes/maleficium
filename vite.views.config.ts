// Builds one MCP App View into a self-contained html file under
// src-tauri/mcp/views/, which maleficium-mcp embeds with include_str!. The
// mode (or VIEW) names the View: `vite build --config vite.views.config.ts
// --mode compile`. The output is committed so cargo builds need no node:
// rerun `npm run build:views` (every View) after editing one; the mcp
// tests and src/mcp-views/fresh.test.ts fail on a stale build.
import { defineConfig } from 'vite';
import { viteSingleFile } from 'vite-plugin-singlefile';
import process from 'node:process';

/** Every View, by its directory under src/mcp-views/. */
export const VIEWS = ['snippet', 'compile'];

export default defineConfig(({ mode }) => {
  const view = process.env.VIEW ?? (VIEWS.includes(mode) ? mode : 'snippet');
  return {
    root: `src/mcp-views/${view}`,
    plugins: [viteSingleFile()],
    build: {
      outDir: '../../../src-tauri/mcp/views',
      emptyOutDir: false,
      target: 'es2020',
      rollupOptions: { input: `src/mcp-views/${view}/${view}.html` },
    },
  };
});
