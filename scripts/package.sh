#!/bin/sh
# package.sh — build this host's release packages into one directory.
#
# The one build path: CI runs it on every OS, the Dockerfile runs it inside
# Ubuntu 24.04, and release.sh build --host runs it on a release-ready host.
#   Linux:   .deb + AppImage (the AppImage repacked without libwayland)
#   macOS:   .dmg (ad-hoc signed; the host's own architecture)
#   Windows: NSIS setup .exe (run from Git Bash or MSYS2)
# Expects the toolchains (rust-toolchain.toml, .nvmrc), `npm ci`, and the
# sidecars (scripts/fetch-sidecars.sh) to be in place already.
#
# Usage: sh scripts/package.sh [--out DIR]   (default: out/)
# POSIX sh.
set -eu
unset CDPATH
ROOT=$(cd -- "$(dirname -- "$0")/.." && pwd -P)
OUT="$ROOT/out"
while [ $# -gt 0 ]; do
    case "$1" in
        --out) OUT="${2:?--out needs a dir}"; shift 2 ;;
        *) echo "usage: sh scripts/package.sh [--out DIR]" >&2; exit 2 ;;
    esac
done

die() { printf 'package: %s\n' "$1" >&2; exit 1; }
[ -n "$(ls -A "$ROOT/src-tauri/binaries" 2>/dev/null)" ] \
    || die "no sidecars in src-tauri/binaries; run sh scripts/fetch-sidecars.sh first"

case "$(uname -s)" in
    Linux) BUNDLES=deb,appimage ;;
    Darwin) BUNDLES=dmg ;;
    MINGW* | MSYS* | CYGWIN*) BUNDLES=nsis ;;
    *) die "unsupported host: $(uname -s)" ;;
esac

BUNDLE="$ROOT/src-tauri/target/release/bundle"
# A reused target dir still holds packages from earlier versions.
rm -rf "$BUNDLE"
# linuxdeploy and appimagetool are AppImages; hosts and containers without
# FUSE (CI runners, Docker) must extract and run them instead of mounting.
export APPIMAGE_EXTRACT_AND_RUN=1
# hdiutil create/detach race Spotlight ("Resource busy"); the create-dmg
# Tauri embeds predates upstream's retries. CI-only: leave dev machines alone.
if [ "$BUNDLES" = dmg ] && [ "${CI:-}" = true ]; then
    sudo mdutil -a -i off >/dev/null || echo "package: mdutil off failed; continuing" >&2
fi
# Tauri swallows the script output unless verbose: on failure, re-run the
# bundle step verbose with output captured, so the log names the failing
# call. The compile is cached; other hosts fail fast as before.
BUILD_LOG="${TMPDIR:-/tmp}/maleficium-package-build.log"
if (cd "$ROOT" && npm run tauri build -- --bundles "$BUNDLES"); then
    :
elif [ "$BUNDLES" = dmg ]; then
    echo "package: dmg bundle failed; re-running the bundle step verbose, log in $BUILD_LOG" >&2
    if (cd "$ROOT" && npm run tauri build -- --verbose --bundles "$BUNDLES") >"$BUILD_LOG" 2>&1; then
        echo "package: dmg bundle passed on re-run" >&2
    else
        echo "package: dmg bundle failed twice; last errors:" >&2
        tail -n 30 "$BUILD_LOG" >&2
        exit 1
    fi
else
    exit 1
fi

mkdir -p "$OUT"
case "$BUNDLES" in
    deb,appimage)
        sh "$ROOT/scripts/repack-appimage.sh" "$BUNDLE"/appimage/*.AppImage
        cp "$BUNDLE"/deb/*.deb "$BUNDLE"/appimage/*.AppImage "$OUT"/ ;;
    dmg) cp "$BUNDLE"/dmg/*.dmg "$OUT"/ ;;
    nsis) cp "$BUNDLE"/nsis/*-setup.exe "$OUT"/ ;;
esac
ls -l "$OUT"
