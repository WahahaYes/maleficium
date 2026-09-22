/// <reference types="vitest/config" />
import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import process from 'node:process';
const host = process.env.TAURI_DEV_HOST;
// scripts/dev.sh picks a free pair and exports DEV_PORT. Unset means a
// bare `tauri dev` (e.g. e2e/stills-run.sh): tauri.conf.json's devUrl, 1420.
const port = Number(process.env.DEV_PORT ?? 1420);

// https://vite.dev/config/
export default defineConfig(() => ({
  plugins: [react()],

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
