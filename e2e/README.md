# e2e/ — committed proof harnesses + driver configs (product test code)

Admitted by RULES §4 (user decision 2026-09-14). Clearly separated from
`src/` and `src-tauri/`: this folder drives the built app or replicates its
derivations — it never ships in the product bundle.

## Contents

- `project-footprint.sh` — static project-footprint audit (no window needed).
  Copies `playground/simple/` to a scratch git
  repo, mirrors `hashRoot`/`appTrashDir`/`appOutDir`/`out_dir_for`, and
  asserts porcelain discipline at every step. Run:
  `./e2e/project-footprint.sh` from `maleficium/`. Green 2026-09-14.
  (Full writeup: `notes/08-devloop/2026-09-14-footprint-harness.md`.)
  Also pins the trust boundary statically: no `$HOME` in `capabilities/`,
  `security.csp` enforced, `opener` absent incl. lockfiles.

## Conventions

- Harnesses resolve the repo root from their own path (`DEVROOT=…/..`) —
  no hardcoded absolute paths, no writes outside OS tmp.
- Never commit fixtures with megabytes: reuse `playground/simple/` (small sources)
  or generate into tmp at runtime (per `04-scale` policy).
- Name tests for the functionality, never the session or slice
  (no stage tags in filenames, describes, or messages).
- Driver-driven runs land here next — see the 08-devloop plan. Embedded
  provider (`@wdio/tauri-service`, no external driver) is the chosen route;
  standalone `tauri-driver` is blocked (no `webkit2gtk-driver` in Ubuntu
  resolute).
