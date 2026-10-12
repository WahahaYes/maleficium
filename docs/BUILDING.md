# Building Maleficium from source

Release targets are Linux x86_64 (`.deb` and AppImage), macOS aarch64 and x86_64 (`.dmg`), and Windows x86_64 (NSIS installer). Every build needs network access (npm, crates.io). Run every command from the repo root.

Every route ends in the same script, `scripts/package.sh`, which builds this host's packages into `out/`. Toolchain pins live in `rust-toolchain.toml` and `.nvmrc`.

## Docker (reproducible Linux)

```sh
docker build --output type=local,dest=../maleficium-release .
```

This runs `package.sh` on Ubuntu 24.04 and writes the `.deb` and `.AppImage` into `../maleficium-release/`. Ubuntu 24.04 is the oldest base the packages support, because the bundled Tectonic needs glibc 2.39. Later builds reuse the dependency layers and a Cargo cache.

## On the host

### Toolchain

- Node from `.nvmrc`.
- Rust from `rust-toolchain.toml`: run `rustup toolchain install` in the repo root.
- The engine in `src-tauri/engine` is its own workspace with its own pinned nightly. Run `rustup toolchain install` there too. Building it needs a C and C++ compiler, libclang headers, and a few GB of RAM; set `CARGO_BUILD_JOBS=3` on a small machine.

### Linux

On Ubuntu 24.04 or newer, or Debian 13 or newer, install the packages CI and the Dockerfile use:

```sh
xargs -a scripts/linux-deps.txt sudo apt-get install -y --no-install-recommends
```

The engine links ICU, freetype, fontconfig, graphite2, libpng, libxml2 and libxslt from the system. The `-dev` packages in that list are build dependencies, and the `.deb` declares the matching libraries as runtime dependencies (`bundle.linux.deb.depends` in `src-tauri/tauri.conf.json`).

On Fedora or Arch, install the [Tauri 2 Linux prerequisites](https://v2.tauri.app/start/prerequisites/#linux), plus git, python3, graphite2 and a static zlib (Fedora: `zlib-ng-compat-static`).

A host build links against the host's glibc. For packages meant for other machines, use Docker or CI.

### macOS

Install Xcode Command Line Tools (`xcode-select --install`) and git, plus the engine's libraries from Homebrew (the `brew install` line in `.github/workflows/build.yml`). Build on the architecture you target. The app is ad-hoc signed, which Apple silicon requires before it runs a binary.

### Windows

Install Rust's MSVC toolchain (Visual Studio Build Tools, "Desktop development with C++") and Git Bash. The engine's C libraries come from vcpkg with the static triplet `x64-windows-static-md`. The ports and environment variables are in `.github/workflows/build.yml`.

### Build

```sh
npm ci
sh scripts/build-engine.sh
sh scripts/package.sh             # packages land in out/
```

`build-engine.sh` builds `maleficium-engine` into `src-tauri/binaries/` and smoke-tests it. The engine holds Tectonic and latexml, pinned by `src-tauri/engine/Cargo.lock`, and a vendored SyncTeX parser. It is the app's only sidecar.

`build-engine.sh` then generates latexml's format dumps into `src-tauri/resources/dumps/`, which the app bundles as resources. The dumps come from the TeX bundle pinned in `src-tauri/core/src/engine.rs`, so the first run needs the network for a couple of minutes. They are valid for one latexml version and one bundle pin. `scripts/dumps-key.sh` names that pair, and an unchanged pair is not regenerated.

`package.sh` refuses to run without the engine or the dumps. The installed engine finds the dumps in the app's resources; `--dumps` or `MALEFICIUM_DUMP_DIR` points it elsewhere.

`maleficium-engine convert` needs no TeX installation: it resolves every file latexml asks for from the pinned bundle. `e2e/convert-run.sh` proves this with the host TeX hidden.

On Linux, `package.sh` repacks the AppImage without the `libwayland-*` libraries the bundler copies in. They clash with a newer host Mesa and leave the window blank.

## Checking

```sh
npm run check                     # lint, format, types, vitest, cargo fmt/clippy/test
python3 e2e/package-smoke.py out  # install out/'s packages and prove they run
```

`package-smoke.py` installs the packages the way a user would, compiles through the installed engine, and checks the first launch. On Linux it needs Docker. See [e2e/README.md](../e2e/README.md).

## CI

[`.github/workflows/build.yml`](../.github/workflows/build.yml) runs on pull requests and on pushes to `main`. It skips the jobs a change cannot affect, based on the paths it touches.

- Each release target builds on its own OS: `package.sh`, then `package-smoke.py`, then `npm run check`. Only toolchain setup differs between them. Each uploads its packages and a screenshot of the smoke launch.
- An e2e job runs the interactive-papers suites on Linux, the same way `scripts/ci-e2e.sh` runs them locally.
- Changed vendored papers and examples are recompiled.
- A final `ci` job passes only if every job the change needed passed.

[`.github/workflows/release.yml`](../.github/workflows/release.yml) runs when a `vX.Y.Z` tag is pushed. It checks that the tag is on `main` and that the manifests and `CHANGELOG.md` are at X.Y.Z, runs `build.yml` on the tagged commit, and publishes the GitHub release with every package attached and the CHANGELOG section as its notes. A failed run can be rerun from the Actions tab.

## Cutting a release

`main` is protected: changes land only by pull request after the required checks pass. `v*` tags cannot be moved or deleted, and only repository admins can create them.

As changes land, each PR adds its notes under `## Unreleased` at the top of `CHANGELOG.md`. They become the GitHub release notes, so write them for users. Then, from any clean checkout with `npm ci` done:

```sh
sh scripts/release.sh X.Y.Z
```

That command:

1. Cuts `release-X.Y.Z` from `origin/main` in a scratch worktree, leaving your checkout untouched. It bumps the manifests and their lockfiles, renames `## Unreleased` to `## X.Y.Z - <date>`, commits `release X.Y.Z`, pushes, and opens the release PR.
2. Waits for the PR's required checks and merges exactly the commit that was built.
3. Tags the merge commit `vX.Y.Z` and pushes the tag as you.
4. Waits for `release.yml` to publish the release, then prints its URL.

Options and behavior:

- `--notes-file FILE` uses FILE as the notes instead of `## Unreleased`.
- `--no-push` stops after the release commit and keeps the worktree for inspection.
- Rerunning resumes where a run stopped. On a failed check it prints the failing steps and says whether to rerun or fix.
- It refuses a version number written into app source or docs (`scripts/release-meta.py stale`, also run in CI). The app reads its version from `package.json` at build time.

`sh scripts/release.sh build [--host] [--out DIR]` builds this checkout's Linux packages, in Docker or with `--host` on this machine, to smoke with `e2e/package-smoke.py` before a release. It publishes nothing.
