# Maleficium

Desktop-native LaTeX editor. TypeScript + React + MUI frontend,
Tauri 2 + Rust backend, Linux-first, fully offline (bundled Tectonic).

## Rule 1 — commits

One concern per commit. Brief single line, lowercase, no prefix,
no trailing period.

- Good: `associate latex main file`, `restore dpr-folded preview scale`
- Bad: `feat: associate main file`, `fix preview scale bug`, two concerns in one commit

`package.json` + `package-lock.json` always ship in the same commit,
in sync — never leave the lockfile dirty, never delegate it.
No demo or tooling-only code. Tests (`*.test.ts`, `src/test/`, `e2e/`)
are product code. Generated output lives only in OS tmp, never committed.
`LICENSE` choice and repo `README` stay deferred until the functional POC.

## Rule 2 — code documentation

Comments state what the code does now, in one or two lines. Nothing else.

Never reference in code: prior state, bugfixes, history, dates, session
IDs, ticket numbers, planning docs, other objectives, or any other
location in the codebase — no file paths, symbol backlinks, or section
references in prose. History and rationale live outside the repo, never
in it. Each unit must read fully from its own code plus its own comment.

## Verify before claiming (run in this dir)

- `./node_modules/.bin/tsc --noEmit --skipLibCheck` (never `npx tsc`)
- `npm test -- --run` and `cargo test --manifest-path src-tauri/Cargo.toml --lib`
- `npm run build` and `cargo check --manifest-path src-tauri/Cargo.toml`
- `e2e/project-footprint.sh`
- GUI-only claims stay unconfirmed without a human click-through.

## No legacy

No users, no shipped release: nothing to be backward-compatible with.
Moving a behavior deletes the old path in the same slice — no fallbacks,
aliases, re-exports, `@deprecated` shims, or dual-read resolvers.
Old artifacts are simply not read anymore. Runtime fallbacks for
environments we actively support (cross-device copy-then-delete,
web-fallback save dialog, honest non-repo degradation) are robustness,
not legacy — keep those. End each slice with a
`grep legacy|compat|deprecated|fallback|stub` + dead-export audit.

## Licenses

Reference material is semantic inspiration only — never copy code or
license text. Only MIT-style patterns may be followed closely.
