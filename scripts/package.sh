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
(cd "$ROOT" && npm run tauri build -- --bundles "$BUNDLES")

mkdir -p "$OUT"
case "$BUNDLES" in
    deb,appimage)
        sh "$ROOT/scripts/repack-appimage.sh" "$BUNDLE"/appimage/*.AppImage
        cp "$BUNDLE"/deb/*.deb "$BUNDLE"/appimage/*.AppImage "$OUT"/ ;;
    dmg) cp "$BUNDLE"/dmg/*.dmg "$OUT"/ ;;
    nsis) cp "$BUNDLE"/nsis/*-setup.exe "$OUT"/ ;;
esac
ls -l "$OUT"
