# embed-runtime

Everything Maleficium copies into an exported paper or into a user's project lives here, under one licence: [MIT No Attribution](LICENSE) (MIT-0). The rest of Maleficium is AGPL-3.0. Keeping this code separate means a paper you publish carries no licence obligations from the tool that made it.

- `reader/`: the reader page script, styles and mark, inlined into every exported bundle and widget document.
- `src/`: TypeScript sources of the widget bridge and the built-in runtimes, built into `built/`. Each built-in's `runtime.json` sits beside its source; the app embeds it (`src-tauri/core/src/builtins.rs`). The runtime scaffold also copies the model fork entry and its sample from here.
- `built/`: the generated runtimes and bridge, committed (`npm run build:runtimes`). Inlined into bundles or copied into a project by the runtime scaffold.
- `tex/`: `maleficium-interactive.sty`, copied into every new project through the templates.
- `samples/`: the custom-runtime samples documented in [docs/RUNTIMES.md](../docs/RUNTIMES.md). One of them seeds the runtime scaffold.
- `vite.config.ts`: the runtime build. Not shipped.

Third-party code that the runtimes inline or vendor (Vega, three.js and their dependencies) keeps its own licence. See [NOTICE](../NOTICE) and `built/model-fork/vendor/three/LICENSE`.

## The boundary

- Code here imports only from inside this folder or from npm packages with a permissive licence. `src/boundary.test.ts` and `src/offline.test.ts` check both.
- The app embeds this code with `include_str!`/`include_bytes!` and writes no script or style of its own into a bundle. `src-tauri/core/tests/embed_boundary.rs` checks the exporter and the scaffold.
- Contributions here are accepted under MIT-0. Moving code in from the rest of the repo relicenses it, so move only code whose authors agree to MIT-0.
