# Contributing to Maleficium

## Commits

One concern per commit. Brief single line, lowercase, no prefix, no trailing period.

- Good: `associate latex main file`, `restore dpr-folded preview scale`
- Bad: `feat: associate main file`, `fix preview scale bug`, two concerns in one commit

Stage only your own files — never `add -A` / `commit -a`. `package.json` +
`package-lock.json` always ship in the same commit, in sync. Release
commits use the message `release X.Y.Z` (see `BUILDING.md`).

## Code comments

Comments state what the code does now, in one or two lines. Never
reference prior state, bugfixes, history, dates, tickets, or other
locations in the codebase — history and rationale live outside the repo.

## Verify before claiming (run in this dir)

- `./node_modules/.bin/tsc --noEmit --skipLibCheck` (never `npx tsc`)
- `npm test -- --run` and `cargo test --manifest-path src-tauri/Cargo.toml --lib`
- `npm run build` and `cargo check --manifest-path src-tauri/Cargo.toml`
- `e2e/project-footprint.sh`

GUI-only claims stay unconfirmed without a click-through on a real desktop.

## No legacy

Pre-1.0 there is nothing to be backward-compatible with. Moving a
behavior deletes the old path in the same change — no fallbacks,
aliases, re-exports, `@deprecated` shims, or dual-read resolvers. Real
runtime fallbacks for supported environments (cross-device
copy-then-delete, web-fallback save dialog, honest non-repo
degradation) are robustness, not legacy — keep those.

## Licenses

Reference material is semantic inspiration only — never copy code or
license text. Only MIT-style patterns may be followed closely.

## Styling

Theme tokens are the only styling source: palette keys, spacing
multipliers, and shape scale — never hardcoded hex, manual shadows, or
custom CSS outside the SyncTeX exception in `App.css`. Depth comes from
divider borders and the flattened shadow scale, not elevation.
Appearance prefs flow through `AppearancePrefs` into the theme factory;
vendored palettes live under `src/assets/themes/`.

## Tests and harnesses

Tests (`*.test.ts`, `src/test/`, `e2e/`) are product code. `e2e/` holds
the proof harnesses and driver configs; generated output lives only in
OS tmp, never committed. No demo or tooling-only code in the tree.

Harness runs go through `e2e/worktree-run.sh`: one reused worktree and
its own cargo target under `/var/tmp`, so a run never rebuilds your dev
checkout. Stills build the app once per run, and the cold-compile state
reads the TeX bundle through `e2e/bundle-mirror.py` (network only on its
first fill). Cargo never prunes its caches: `npm run clean` drops the
incremental ones, `-- --all` removes the target dirs, `-- --dry-run`
shows sizes first; it refuses while a build or harness is running.
