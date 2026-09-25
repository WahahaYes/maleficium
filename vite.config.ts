/// <reference types="vitest/config" />
import { defineConfig, searchForWorkspaceRoot, type Plugin } from 'vite';
import react from '@vitejs/plugin-react';
import process from 'node:process';
import { readFileSync } from 'node:fs';
const host = process.env.TAURI_DEV_HOST;
// scripts/dev.sh picks a free pair and exports DEV_PORT; e2e/stills-run.sh
// exports its STILLS_PORT. Unset means tauri.conf.json's devUrl, 1420.
const port = Number(process.env.DEV_PORT ?? 1420);

// Stills harness: one build and one vite serve every launch, so the project
// preset cannot ride in devUrl (a different devUrl is a different build). The
// harness writes the preset path to STILLS_PRESET_FILE before each launch,
// and a bare `/` redirects to `/?project=<it>`. Dev server only; unset, inert.
function stillsPreset(file: string | undefined): Plugin {
  return {
    name: 'stills-preset',
    apply: 'serve',
    configureServer(server) {
      if (!file) return;
      server.middlewares.use((req, res, next) => {
        if (req.method !== 'GET' || req.url !== '/') return next();
        let preset = '';
        try {
          preset = readFileSync(file, 'utf8').trim();
        } catch {
          /* no preset written: a launch with no project */
        }
        if (!preset) return next();
        res.statusCode = 302;
        res.setHeader('Location', `/?project=${encodeURIComponent(preset)}`);
        res.end();
      });
    },
  };
}

// https://vite.dev/config/
export default defineConfig(() => ({
  plugins: [react(), stillsPreset(process.env.STILLS_PRESET_FILE)],
  // A scratch worktree shares node_modules by symlink: its dep optimizer writes
  // to VITE_CACHE_DIR, never the node_modules/.vite a live dev server serves.
  cacheDir: process.env.VITE_CACHE_DIR ?? 'node_modules/.vite',

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. the devUrl names this exact port, fail if it is not available
  server: {
    port,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: 'ws',
          host,
          port: port + 1,
        }
      : undefined,
    // A scratch worktree's symlinked node_modules resolves outside its root:
    // VITE_FS_ALLOW names that real directory so vite serves it (pdf.js worker).
    fs: {
      allow: [
        searchForWorkspaceRoot(process.cwd()),
        ...(process.env.VITE_FS_ALLOW ? [process.env.VITE_FS_ALLOW] : []),
      ],
    },
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`
      ignored: ['**/src-tauri/**'],
    },
  },
  // Unit tests: pure logic and injected-IO hooks under src/. Node, not a DOM:
  // nothing here renders; components are proven by the stills harness.
  test: {
    environment: 'node',
    include: ['src/**/*.test.{ts,tsx}'],
    pool: 'forks',
    testTimeout: 5_000,
  },
}));
