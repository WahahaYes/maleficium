// Builds one built-in widget runtime into a self-contained index.html under
// embed-runtime/built/<name>/, the generated runtime host a bundle's
// widgets/<id>/index.html is made from. Offline by construction: everything is
// inlined from node_modules, nothing is fetched. The output is committed;
// rerun `npm run build:runtimes` after editing one (embed-runtime/src/
// fresh.test.ts fails on a stale build).
import { defineConfig } from 'vite';
import { viteSingleFile } from 'vite-plugin-singlefile';
import { readFileSync } from 'node:fs';
import process from 'node:process';
import type { Plugin } from 'vite';

/** Every runtime, by its directory under embed-runtime/src/. */
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

/** What a fork's viewer reads from the page's other scripts: three.js
 *  and its addons from `THREE`, the bridge from `mfwBridge`. */
const isBridge = (id: string) => /[\\/]bridge(\.ts)?$/.test(id);
const forkGlobal = (id: string) =>
  isBridge(id) ? 'mfwBridge' : id === 'three' || id.startsWith('three/') ? 'THREE' : null;

/** three.js's licence beside the vendored script (the fork's `licenseFile`). */
function threeLicense(): Plugin {
  return {
    name: 'three-license',
    generateBundle() {
      this.emitFile({
        type: 'asset',
        fileName: 'LICENSE',
        source: readFileSync('node_modules/three/LICENSE', 'utf8'),
      });
    },
  };
}

export default defineConfig(({ mode }) => {
  // The bridge authors copy into their own runtimes: bridge.ts as one
  // classic script, committed beside the built-ins the scaffold copies it from.
  if (mode === 'bridge') {
    return {
      build: {
        outDir: 'embed-runtime/built/bridge',
        emptyOutDir: true,
        target: 'es2020',
        lib: {
          entry: 'embed-runtime/src/bridge.ts',
          formats: ['iife'],
          name: 'mfwBridge',
          fileName: () => 'bridge.js',
        },
      },
    };
  }
  // A `model@1` fork (the scaffold's `from: model@1`): three.js as a
  // vendored classic script with its licence, and the viewer as a readable
  // classic script over it and the bridge, both committed for the scaffold.
  if (mode === 'fork-three') {
    return {
      plugins: [threeLicense()],
      build: {
        outDir: 'embed-runtime/built/model-fork/vendor/three',
        emptyOutDir: true,
        target: 'es2020',
        chunkSizeWarningLimit: 4000,
        lib: {
          entry: 'embed-runtime/src/model/three-vendor.ts',
          formats: ['iife'],
          name: 'THREE',
          fileName: () => 'three.js',
        },
      },
    };
  }
  if (mode === 'fork-viewer') {
    return {
      build: {
        outDir: 'embed-runtime/built/model-fork',
        emptyOutDir: false,
        target: 'es2020',
        minify: false,
        lib: {
          entry: 'embed-runtime/src/model/main.ts',
          formats: ['iife'],
          name: 'mfwModelViewer',
          fileName: () => 'viewer.js',
        },
        rollupOptions: {
          external: (id: string) => forkGlobal(id) !== null,
          output: { globals: (id: string) => forkGlobal(id) ?? id },
        },
      },
    };
  }
  const runtime = process.env.RUNTIME ?? (RUNTIMES.includes(mode) ? mode : 'table');
  return {
    root: `embed-runtime/src/${runtime}`,
    plugins: [viteSingleFile(), ...(CLASSIC.includes(runtime) ? [classicScript()] : [])],
    build: {
      outDir: `../../built/${runtime}`,
      emptyOutDir: true,
      target: 'es2020',
      // vega and three.js are large; they ship inline in the one file by design.
      chunkSizeWarningLimit: 4000,
      rollupOptions: {
        input: `embed-runtime/src/${runtime}/index.html`,
        ...(CLASSIC.includes(runtime) ? { output: { format: 'iife' as const } } : {}),
      },
    },
  };
});
