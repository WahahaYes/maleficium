// Builds every committed embed-runtime output, one vite build per mode of
// embed-runtime/vite.config.ts: each widget runtime (an embed-runtime/src/<name>/
// with a runtime.json), then the other modes listed below.
import { existsSync, readdirSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import process from 'node:process';

const runtimes = readdirSync('embed-runtime/src')
  .filter((d) => existsSync(`embed-runtime/src/${d}/runtime.json`))
  .sort();
for (const mode of [...runtimes, 'liveeq', 'bridge', 'expr', 'fork-three', 'fork-viewer']) {
  const r = spawnSync(
    process.execPath,
    [
      'node_modules/vite/bin/vite.js',
      'build',
      '--config',
      'embed-runtime/vite.config.ts',
      '--mode',
      mode,
    ],
    { stdio: 'inherit' },
  );
  if (r.status !== 0) process.exit(r.status ?? 1);
}
