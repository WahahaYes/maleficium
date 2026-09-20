# e2e/ — proof harnesses + driver scripts (product test code)

Separated from `src/` and `src-tauri/`: this folder drives the built app
or replicates its derivations — it never ships in the product bundle.

## Contents

- `project-footprint.sh` — static project-footprint audit (no window needed).
  Copies `playground/simple/` to a scratch git repo, replicates each
  app-local path derivation in bash, and asserts porcelain discipline at
  every step. Run: `./e2e/project-footprint.sh` from `maleficium/`.
  Also pins statically: no `$HOME` in `capabilities/`, `security.csp`
  enforced, `opener` absent incl. lockfiles.
- `driver-run.sh` — live run over the stdio sidecar: grant → compile →
  poll → synctex → delete → undo over JSON-RPC against a scratch copy of
  `playground/simple/`, then asserts porcelain discipline + artifact homes.

## Conventions

- Harnesses resolve the repo root from their own path (`DEVROOT=…/..`) —
  no hardcoded absolute paths, no writes outside OS tmp.
- Never commit megabyte fixtures: reuse `playground/simple/` (small sources)
  or generate into tmp at runtime.
- Name tests for the functionality, never the session or slice.
