# Building Maleficium from source

Release target: Linux x86_64, `.deb` and AppImage. Both routes below need network access during the build (npm, crates.io, Tectonic release assets, SyncTeX source).

Run every command below from the repo root.

## Docker (reproducible)

```sh
docker build --output type=local,dest=../maleficium-release .
```

This builds on Ubuntu 24.04 with pinned Node and Rust (see `Dockerfile`), and writes the `.deb` and `.AppImage` into `../maleficium-release/`. A cold build takes a while; later builds reuse the dependency layers and a Cargo cache mount.

## On the host

### Toolchain

- Node 22 (the Docker build pins 22.23.2; Vite needs `^20.19.0 || >=22.12.0`)
- Rust 1.98.1 or newer stable (`rustup toolchain install 1.98.1`)

### System packages

Ubuntu 24.04 / Debian 13:

```sh
sudo apt install build-essential curl wget file git pkg-config python3 xz-utils \
  libwebkit2gtk-4.1-dev libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev \
  zlib1g-dev libgraphite2-3
```

`zlib1g-dev` provides the static zlib the SyncTeX build links; `libgraphite2-3` is a runtime dependency of the bundled Tectonic.

Fedora and Arch: install the Tauri 2 Linux prerequisites listed at <https://v2.tauri.app/start/prerequisites/#linux>, plus git, python3, graphite2 and a static zlib (Fedora: `zlib-ng-compat-static`).

### Build

```sh
npm ci
sh scripts/fetch-sidecars.sh
npm run tauri build -- --bundles deb,appimage
```

`fetch-sidecars.sh` puts the sha256-pinned Tectonic and a SyncTeX built from pinned source into `src-tauri/binaries/`; the bundler packs both next to the app binary.

Artifacts land in:

- `src-tauri/target/release/bundle/deb/*.deb`
- `src-tauri/target/release/bundle/appimage/*.AppImage`

If the AppImage step fails with a FUSE error, run the build with `APPIMAGE_EXTRACT_AND_RUN=1` set.

## Cutting a release

Releases are cut by hand with `scripts/release.sh`; there is no CI. From a clean `main`:

```sh
sh scripts/release.sh 0.2.0 --notes-file /tmp/notes.md
```

This bumps the version in `package.json`, `src-tauri/Cargo.toml`, and `src-tauri/tauri.conf.json` (with their lockfiles), adds the entry to `CHANGELOG.md`, commits as `release X.Y.Z`, tags, and builds.

- Without `--notes-file`, the notes are the commit subjects since the last tag.
- The build runs in Docker by default. `--host` builds on this machine, `--out` sets the artifact directory, and `--skip-build` stops after the tag.
- Nothing leaves the machine unless you pass `--publish`, which pushes `main` and the tag and creates the GitHub release with the `.deb` and `.AppImage` attached.
