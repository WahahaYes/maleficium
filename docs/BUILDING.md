# Building Maleficium from source

Release targets: Linux x86_64 (`.deb` and AppImage), macOS aarch64 and x86_64 (`.dmg`), and Windows x86_64 (NSIS installer). Every route below needs network access during the build (npm, crates.io, Tectonic release assets, SyncTeX source).

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
sh scripts/repack-appimage.sh src-tauri/target/release/bundle/appimage/*.AppImage
```

The repack drops the `libwayland-*` libraries the bundler copies into the AppImage: they clash with a newer host Mesa and leave the window blank. The Docker build and `release.sh` run it for you.

`fetch-sidecars.sh` puts the sha256-pinned Tectonic and a SyncTeX built from pinned source into `src-tauri/binaries/`; the bundler packs both next to the app binary.

Artifacts land in:

- `src-tauri/target/release/bundle/deb/*.deb`
- `src-tauri/target/release/bundle/appimage/*.AppImage`

If the AppImage step fails with a FUSE error, run the build with `APPIMAGE_EXTRACT_AND_RUN=1` set.

## macOS

Needs Xcode Command Line Tools (`xcode-select --install`), Node 22, Rust 1.98.1, and git. Build on the architecture you are targeting:

```sh
npm ci
sh scripts/fetch-sidecars.sh
npm run tauri build -- --bundles dmg
```

SyncTeX links the system zlib dynamically, since macOS does not support fully static binaries. The app is ad-hoc signed (`bundle.macOS.signingIdentity` is `-`), which Apple silicon requires before it will run a binary at all. The `.dmg` lands in `src-tauri/target/release/bundle/dmg/`.

## Windows

Needs Node 22, Rust 1.98.1 with the MSVC toolchain (Visual Studio Build Tools, "Desktop development with C++"), and [MSYS2](https://www.msys2.org/) for the SyncTeX build. In an MSYS2 **MINGW64** shell:

```sh
pacman -S --needed curl git python tar mingw-w64-x86_64-gcc mingw-w64-x86_64-zlib
sh scripts/fetch-sidecars.sh
```

Then, from PowerShell or cmd:

```sh
npm ci
npm run tauri build -- --bundles nsis
```

The installer lands in `src-tauri/target/release/bundle/nsis/`.

## CI

`.github/workflows/build.yml` builds every target above on each pull request: Linux through the Dockerfile, macOS on `macos-15` (aarch64) and `macos-15-intel` (x86_64), and Windows on `windows-2025`. Each job's artifacts can be downloaded from the run page.

`.github/workflows/release.yml` drafts a release. Run it from the Actions tab on `main` with a version whose manifests and `CHANGELOG.md` section are already on `main`. It checks both, fails if that version is already tagged or published, runs `build.yml`, and creates a **draft** GitHub release, targeting that commit, with every artifact attached. Publishing the draft creates the `vX.Y.Z` tag. Rerunning it for the same version replaces the draft's assets.

## Cutting a release

`scripts/release.sh` prepares a release locally and can build and publish the Linux artifacts by itself. For all platforms, prepare with it and draft the release from CI (see above). From a clean `main`:

```sh
sh scripts/release.sh 0.2.0 --notes-file /tmp/notes.md
```

This bumps the version in `package.json`, `src-tauri/Cargo.toml`, and `src-tauri/tauri.conf.json` (with their lockfiles), adds the entry to `CHANGELOG.md`, commits as `release X.Y.Z`, tags, and builds.

- Without `--notes-file`, the notes are the commit subjects since the last tag.
- The build runs in Docker by default. `--host` builds on this machine, `--out` sets the artifact directory, and `--skip-build` stops after the tag.
- Rerunning for the same version is safe. Manifests already at that version are left as they are, an existing `CHANGELOG.md` section for it is kept unless `--notes-file` is passed, which replaces that section in place. If `vX.Y.Z` already tags `HEAD`, the script skips straight to build and publish (delete the local tag first to change its notes). A `vX.Y.Z` tag on any other commit is an error.
- Nothing leaves the machine unless you pass `--publish`, which pushes `main` and the tag and creates the GitHub release with the `.deb` and `.AppImage` attached.
