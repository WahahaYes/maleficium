# e2e/ — proof harnesses + driver scripts (product test code)

Separated from `src/` and `src-tauri/`: this folder drives the built app or replicates its derivations — it never ships in the product bundle.

## Contents

- `project-footprint.sh` — static project-footprint audit (no window needed). Copies `playground/simple/` to a scratch git repo, replicates each app-local path derivation in bash, and asserts porcelain discipline at every step. Run: `./e2e/project-footprint.sh` from `maleficium/`. Also pins statically: no `$HOME` in `capabilities/`, `security.csp` enforced, `opener` absent incl. lockfiles.
- `driver-run.sh` — live run over the stdio sidecar: grant → compile → poll → synctex → delete → undo over JSON-RPC against a scratch copy of `playground/simple/`, then asserts porcelain discipline + artifact homes. A second half drives heavy documents: 1000-file open latency (one directory level, never the tree), cancel mid-compile on a 3000-page document, and the D.5 budgets — pdf load, page render, peak memory — measured through the same pdf.js build and viewport formula the preview uses, and printed as stream lines beside their baselines. Exits nonzero when any check or budget misses. Run: `./e2e/driver-run.sh` from `maleficium/`.
  - Heavy fixtures (3000 pages, 1000 files) are generated from `src/test/make-fixture.ts` into OS tmp on first run and reused after; point `DRIVER_FIXTURES` elsewhere (still tmp) to relocate them. `DRIVER_LOG` sets the JSONL run log, `WARM_ONLY=1` stops after the first compile, `POLL_ROUNDS` bounds it, `MCP_ROOT_OVERRIDE` drives a caller-owned tree in place.
- `stills-run.sh` — mechanical still capture for the shell states (Default / Compiling / Failure; 3000pp deferred, see script header). Launches the app under Xvfb with a contained HOME, opens scratch fixtures hands-free (Ctrl+O; the `?project=` preset skips the dialog), drives compile/failure with Ctrl+R, captures PNGs to OS tmp with `import`. No assertions — stills are filed artifacts for on-demand review. Run: `STILLS_OUT=/tmp/stills ./e2e/stills-run.sh` from `maleficium/` (needs Xvfb, xdotool, ImageMagick).

## Conventions

- Harnesses resolve the repo root from their own path (`DEVROOT=…/..`) — no hardcoded absolute paths, no writes outside OS tmp.
- Never commit megabyte fixtures: reuse `playground/simple/` (small sources) or generate into tmp at runtime.
- Name tests for the functionality, never the session or slice.
