# Maleficium

Desktop-native LaTeX editor. TypeScript + React + MUI frontend, Tauri 2 + Rust backend. Linux-first, fully offline via a bundled Tectonic engine.

Pre-release: no shipped version, no user-data compatibility between builds.

## Develop

Prerequisites: Node, Rust, and system webkit2gtk for Tauri. The engine sidecars are not in git: fetching them needs network, curl, python3, and (for SyncTeX) git, gcc and static zlib.

- `npm install`
- `sh scripts/fetch-sidecars.sh` — once: Tectonic release binaries (sha256-checked) and SyncTeX built from pinned source into `src-tauri/binaries/`
- `npm run dev:desktop` — desktop app
- `npm run dev` — web-only Vite frontend

Both pick the first free port pair from 1420 (`scripts/dev.sh`), so parallel instances run side by side.

Release builds (`.deb`, AppImage, on the host or in Docker): see [BUILDING.md](BUILDING.md).

## Verify (run in this dir before claiming behavior change)

- `./node_modules/.bin/tsc --noEmit --skipLibCheck`
- `npm test -- --run` and `cargo test --manifest-path src-tauri/Cargo.toml --lib`
- `npm run build` and `cargo check --manifest-path src-tauri/Cargo.toml`
- `./e2e/project-footprint.sh`

GUI-only claims stay unconfirmed without a human click-through.

## Layout

- `src/` — frontend; `src-tauri/` — Rust backend.
- `e2e/` — committed proof harnesses and driver configs (product test code). Generated output lives only in OS tmp, never committed.

## License

Apache 2.0 — see `LICENSE`, including the accreditation notice.
