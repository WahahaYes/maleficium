# Building Maleficium from source

Release targets: Linux x86_64 (`.deb` and AppImage), macOS aarch64 and x86_64 (`.dmg`), and Windows x86_64 (NSIS installer). Every route below needs network access during the build (npm, crates.io, Tectonic release assets, SyncTeX source).

Run every command below from the repo root.

Every route runs the same build script, `scripts/package.sh`, which builds this host's packages (Linux `.deb` + AppImage, macOS `.dmg`, Windows NSIS setup) into `out/`. Toolchain pins live in one place each: `rust-toolchain.toml` (rustup picks it up) and `.nvmrc`.

## Docker (reproducible Linux)

```sh
docker build --output type=local,dest=../maleficium-release .
```

This runs `package.sh` on Ubuntu 24.04, the same base CI builds on and the oldest the packages support (the bundled Linux Tectonic needs glibc 2.39), and writes the `.deb` and `.AppImage` into `../maleficium-release/`. A cold build takes a while; later builds reuse the dependency layers and a Cargo cache mount.

## On the host

### Toolchain

- Node from `.nvmrc` (Vite needs `^20.19.0 || >=22.12.0`)
- Rust from `rust-toolchain.toml` (`rustup toolchain install` in the repo root)

### Linux

System packages (Ubuntu 24.04 or newer, Debian 13 or newer), from the list CI and the Dockerfile use:

```sh
xargs -a scripts/linux-deps.txt sudo apt-get install -y --no-install-recommends
```

`zlib1g-dev` provides the static zlib the SyncTeX build links; `libgraphite2-3` is a runtime dependency of the bundled Tectonic. Fedora and Arch: install the Tauri 2 Linux prerequisites listed at <https://v2.tauri.app/start/prerequisites/#linux>, plus git, python3, graphite2 and a static zlib (Fedora: `zlib-ng-compat-static`).

A host build links against the host's glibc, so for packages meant for other machines prefer Docker or CI.

### macOS

Needs Xcode Command Line Tools (`xcode-select --install`) and git. Build on the architecture you are targeting. SyncTeX links the system zlib dynamically, since macOS does not support fully static binaries. The app is ad-hoc signed (`bundle.macOS.signingIdentity` is `-`), which Apple silicon requires before it will run a binary at all.

### Windows

Needs Rust's MSVC toolchain (Visual Studio Build Tools, "Desktop development with C++"), Git Bash, and [MSYS2](https://www.msys2.org/) for the SyncTeX build. Fetch the sidecars in an MSYS2 **MINGW64** shell:

```sh
pacman -S --needed curl git python tar mingw-w64-x86_64-gcc mingw-w64-x86_64-zlib
sh scripts/fetch-sidecars.sh
```

and run the rest from Git Bash.

### Build

```sh
npm ci
sh scripts/fetch-sidecars.sh      # Windows: in MSYS2, as above
sh scripts/package.sh             # packages land in out/
```

`fetch-sidecars.sh` puts the sha256-pinned Tectonic and a SyncTeX built from pinned source into `src-tauri/binaries/`; the bundler packs both next to the app binary. On Linux, `package.sh` also repacks the AppImage without the `libwayland-*` libraries the bundler copies in: they clash with a newer host Mesa and leave the window blank.

## Checking

```sh
npm run check                     # lint, format, types, vitest, cargo fmt/clippy/test
python3 e2e/package-smoke.py out  # install out/'s packages and prove they run
```

`package-smoke.py` installs the way a user would (Linux: the `.deb` in a clean `ubuntu:24.04` container, so needs Docker; macOS: the `.dmg`; Windows: a silent NSIS install), compiles a page through the installed `maleficium-mcp`, and requires the app's first launch to open the welcome project in its event log. See `e2e/README.md`.

## CI

`.github/workflows/build.yml` runs on each pull request: one job per target (`ubuntu-24.04`, `macos-15` for aarch64, `macos-15-intel` for x86_64, `windows-2025`). Only toolchain setup differs between them; each then runs `package.sh`, `package-smoke.py`, and `npm run check`, and uploads its packages plus a `screenshot-*` of the smoke launch (macOS, Windows).

`.github/workflows/release.yml` drafts a release. Run it from the Actions tab on `main` with a version whose manifests and `CHANGELOG.md` section are already on `main`. It checks both, fails if that version is already tagged or published, runs `build.yml`, and creates a **draft** GitHub release, targeting that commit, with every package attached. Publishing the draft creates the `vX.Y.Z` tag. Rerunning it for the same version replaces the draft's assets.

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
