# Maleficium

Desktop-native LaTeX editor. TypeScript + React + MUI frontend, Tauri 2 + Rust backend. Linux-first, fully offline via a bundled Tectonic engine.

## Screenshots

![Edit, compile, and preview side by side](docs/screenshots/compile-preview.png)
![New project from a template](docs/screenshots/template-gallery.png)
![First-run welcome tour](docs/screenshots/welcome-tour.png)

## Features

- Editor with LaTeX-aware search, replace preview, and undo-safe apply.
- One-key compile with live progress and a pre-compile problem check.
- Side-by-side PDF preview with SyncTeX forward and inverse search.
- Project file tree, quick file finder, and command palette.
- Ten starter templates (article, report, book, beamer, letter, CV, resume, journal, assignment, welcome).
- Curated color themes with a searchable picker.
- Revision history stored in Rust, shared by the app and its automation API.

## Install (Linux x86_64)

From the [releases page](https://github.com/wahahayes/maleficium/releases):

```sh
sudo apt install ./Maleficium_0.1.0_amd64.deb
```

or run the AppImage:

```sh
chmod +x Maleficium_0.1.0_amd64.AppImage
./Maleficium_0.1.0_amd64.AppImage
```

If the AppImage fails with a FUSE error, prefix the run with
`APPIMAGE_EXTRACT_AND_RUN=1`.

## Build from source

See [BUILDING.md](BUILDING.md) for the Docker and host routes.

## Offline and privacy

The first compile downloads the Tectonic engine bundle (about 300
files) and needs network access. After that the app works fully
offline. That bundle fetch is the only network call the app makes:
no telemetry, no auto-updater, nothing else leaves the machine.

## Status and known limits

0.1.0 ships Linux x86_64 only (`.deb` and AppImage); macOS and
Windows builds wait for a later release. There is no update
mechanism yet — watch the releases page. 0.1.x makes no user-data
compatibility promises between builds.

## License

Apache 2.0 — see `LICENSE`, including the accreditation notice.
Contributions: see [CONTRIBUTING.md](CONTRIBUTING.md).
