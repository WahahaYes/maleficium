# embed-runtime

Everything Maleficium copies into an exported paper or into a user's project lives here, under one licence: [MIT No Attribution](LICENSE) (MIT-0). The rest of Maleficium is AGPL-3.0. Keeping this code separate means a paper you publish carries no licence obligations from the tool that made it.

| Folder | What ships | Where it lands |
| --- | --- | --- |
| `reader/` | `reader.js`, `reader.css`, `mark.svg`, `size-reporter.js` | inlined into every exported bundle's `index.html` and widget documents |
| `src/` | TypeScript sources of the widget bridge and the built-in runtimes (chart, table, model, video), the `model@1` fork entry and its sample | built into `built/`; the fork entry and sample are copied by the runtime scaffold |
| `built/` | the generated runtime hosts, `bridge.js` and the `model@1` fork (`npm run build:runtimes`; committed) | inlined into bundles, or copied into a project by the scaffold |
| `tex/` | `maleficium-interactive.sty` | every new project, through the templates |
| `samples/` | the custom-runtime samples documented in [docs/RUNTIMES.md](../docs/RUNTIMES.md); `caption-overlay@1` seeds the scaffold | copied by authors and the scaffold into their runtimes |
| `vite.config.ts` | the runtime build | not shipped |

Third-party code that the runtimes inline or vendor (Vega, three.js and their dependencies) keeps its own licence; see [NOTICE](../NOTICE) and `built/model-fork/vendor/three/LICENSE`.

## The boundary

- Code here imports only from inside this folder or from npm packages with a permissive licence. `src/boundary.test.ts` and `src/offline.test.ts` check both.
- The app embeds this code with `include_str!`/`include_bytes!` and writes no script or style of its own into a bundle. `src-tauri/core/tests/embed_boundary.rs` checks the exporter and the scaffold.
- Contributions here are accepted under MIT-0. Moving code in from the rest of the repo relicenses it, so move only code whose authors agree to MIT-0.
