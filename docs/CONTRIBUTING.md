# Contributing to Maleficium

Thanks for helping. This page covers setup, the checks every change must pass, and the house rules for code, comments, and commits.

## Setup

Install the toolchain and system packages listed in [BUILDING.md](BUILDING.md), then:

```sh
npm ci
sh scripts/fetch-sidecars.sh   # bundled Tectonic + SyncTeX
npm run dev:desktop            # the app, with hot reload
```

`npm run dev` serves the frontend alone in a browser. Both dev commands pick a free port, so several checkouts can run side by side.

`sh scripts/playground.sh` fills an ignored `playground/` with projects to open by hand: one per built-in template, plus a copy of the `e2e/fixtures/simple` test fixture. It keeps any project folder that already exists; delete one to get a fresh copy.

Recommended: `pre-commit install` runs the format, lint, and type checks on each commit, and checks each commit message.

## Before you commit

Run these from the repo root:

```sh
./node_modules/.bin/tsc --noEmit --skipLibCheck
npm run lint && npm run format:check
npm test
cargo test --manifest-path src-tauri/Cargo.toml --workspace
npm run build
cargo check --manifest-path src-tauri/Cargo.toml --workspace
e2e/project-footprint.sh
```

A GUI change is not confirmed until someone has clicked through it in the real app on a desktop.

## Commits

One concern per commit, with a single-line message: lowercase, no prefix, no trailing period.

- Good: `associate latex main file`, `restore dpr-folded preview scale`
- Bad: `feat: associate main file`, `fix preview scale bug`, two concerns in one commit

No trailers such as `Co-Authored-By`; the commit-msg hook rejects them. `package.json` and `package-lock.json` always change in the same commit. Release commits are `release X.Y.Z` (see [BUILDING.md](BUILDING.md)).

## Code comments

A comment says what the code does now, in a line or two. It does not mention earlier versions, bug fixes, dates, tickets, or other files. That history belongs in commits and issues.

## No legacy code

Before 1.0 there is nothing to stay backward compatible with. When a behavior moves, the old path is deleted in the same change: no fallbacks, aliases, re-exports, or `@deprecated` shims.

Fallbacks that keep the app working on a supported setup are fine, such as copy-then-delete across devices or a download when there is no native save dialog.

## License

Maleficium is Apache 2.0, and contributions are accepted under the same license. Bundled third-party files (themes, engine binaries) need a compatible license and an entry in [NOTICE](../NOTICE).

## Templates

Built-in templates live in `src-tauri/templates/` and are CC0. A new one needs the CC0 header and a row in the table in [src-tauri/templates/README.md](../src-tauri/templates/README.md), which records where each template came from.

## README screenshots

The images in `docs/screenshots/` are generated, not hand-taken. After a visible UI change, run `sh scripts/generate-readme-screenshots.sh`: it builds the current commit in a separate worktree, opens `docs/screenshots/demo-project/` in the app under Xvfb, and rewrites the PNGs. Needs Xvfb, xdotool, and ImageMagick.

## Styling

Style only through theme tokens: palette keys, spacing multipliers, and the shape scale. No hardcoded colors, hand-written shadows, or custom CSS (the one exception is the SyncTeX highlight in `src/App.css`). Depth comes from divider borders and the flat shadow scale, not elevation.

Appearance settings flow through `AppearancePrefs` into the theme factory. Bundled color themes live in `src/assets/themes/`.

## Tests and harnesses

Tests are product code: unit tests sit beside their modules (`*.test.ts`) and in `src/test/`; end-to-end harnesses live in `e2e/`. Harness output goes to the OS temp dir and is never committed. See [e2e/README.md](../e2e/README.md) for what each harness proves and how to run it.

Rust build caches grow without limit. `npm run clean` drops the incremental caches; add `-- --all` to remove the target dirs, or `-- --dry-run` to see sizes first.
