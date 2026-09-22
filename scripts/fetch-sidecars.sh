#!/bin/sh
# Put the bundled engine sidecars in src-tauri/binaries/ (untracked).
#   Tectonic 0.17.0: official release assets, each extracted binary checked
#     against the pinned sha256 below.
#   SyncTeX: built from pinned upstream source with our c-auto.h shim, then
#     smoke-tested (view -> Page:, edit -> Input:/Line:) against a document
#     the fetched Tectonic compiles. The hash below is what this recipe
#     produced on gcc 15 + zlib 1.3.1; another toolchain may differ, so a
#     mismatch warns and the smoke test decides.
# Idempotent: a present binary with the pinned hash is left alone.
# Needs network, curl, tar, python3; SyncTeX also needs git, gcc, static zlib.
# POSIX sh.
set -eu
unset CDPATH
ROOT=$(cd -- "$(dirname -- "$0")/.." && pwd -P)
BIN="$ROOT/src-tauri/binaries"
TECTONIC_VERSION=0.17.0
TECTONIC_URL="https://github.com/tectonic-typesetting/tectonic/releases/download/tectonic%40$TECTONIC_VERSION"
SYNCTEX_REPO=https://github.com/jlaurens/synctex.git
SYNCTEX_REV=04cf8e3e8665ff203248d7af78ee1129afbc1b64
SYNCTEX_TRIPLE=x86_64-unknown-linux-gnu
SYNCTEX_SHA=5e7c245223faf271a3fa8b9963d8b5f223b18c99f0d88c5dbd97579c69ccf883

# triple  sha256-of-extracted-binary
TECTONIC_PINS="
x86_64-unknown-linux-gnu 2b3a86250906c92ed0a3ae8aaa454ec55bd6cede8593b3e549640177f6aecaa3
aarch64-unknown-linux-musl 19a2b763e5875fffefaa193c42e460ceba5983d027b6d50b0094130d26ba4e4a
x86_64-apple-darwin 0d888b4cb5607830f33e4ede37e613c1d0b9251644c209ce78ae807c976637e4
aarch64-apple-darwin b52b5a730e2b0b33087304f7720f649603953f270a6b1c88bb031e1ae01f7f9c
x86_64-pc-windows-msvc 99ffcfdbf1ebf8bdda9e791942e3d06aedb12463fddc33f07de6f5211c8bf08d
"

die() { echo "fetch-sidecars: $1" >&2; exit 1; }
say() { echo "fetch-sidecars: $1"; }
need() { command -v "$1" >/dev/null 2>&1 || die "missing tool: $1"; }
sha() { sha256sum "$1" | cut -d' ' -f1; }

need curl; need tar; need python3; need sha256sum
mkdir -p "$BIN"
WORK=$(mktemp -d "${TMPDIR:-/tmp}/maleficium-sidecars-XXXXXX")
trap 'rm -rf "$WORK"' EXIT

echo "$TECTONIC_PINS" | while read -r triple pin; do
    [ -n "$triple" ] || continue
    case "$triple" in
        *windows*) exe=".exe"; archive="tectonic-$TECTONIC_VERSION-$triple.zip" ;;
        *) exe=""; archive="tectonic-$TECTONIC_VERSION-$triple.tar.gz" ;;
    esac
    dest="$BIN/tectonic-$triple$exe"
    if [ -f "$dest" ] && [ "$(sha "$dest")" = "$pin" ]; then
        say "tectonic $triple present"
        continue
    fi
    say "tectonic $triple: downloading"
    curl -sSfL -o "$WORK/$archive" "$TECTONIC_URL/$archive" || die "download failed: $archive"
    mkdir -p "$WORK/$triple"
    case "$archive" in
        *.zip) python3 -c 'import sys, zipfile; zipfile.ZipFile(sys.argv[1]).extractall(sys.argv[2])' "$WORK/$archive" "$WORK/$triple" ;;
        *) tar -xzf "$WORK/$archive" -C "$WORK/$triple" ;;
    esac
    got=$(sha "$WORK/$triple/tectonic$exe")
    [ "$got" = "$pin" ] || die "tectonic $triple: sha256 $got, expected $pin"
    install -m 755 "$WORK/$triple/tectonic$exe" "$dest"
    say "tectonic $triple: verified"
done

dest="$BIN/synctex-$SYNCTEX_TRIPLE"
if [ -f "$dest" ] && [ "$(sha "$dest")" = "$SYNCTEX_SHA" ]; then
    say "synctex $SYNCTEX_TRIPLE present"
    exit 0
fi
need git; need gcc
say "synctex $SYNCTEX_TRIPLE: building $SYNCTEX_REV"
src="$WORK/synctex-src"
git init -q "$src"
git -C "$src" fetch -q --depth 1 "$SYNCTEX_REPO" "$SYNCTEX_REV" || die "synctex source fetch failed"
git -C "$src" checkout -q FETCH_HEAD
(cd "$src" && gcc -O2 -static -s -I. -I"$ROOT/scripts/synctex-shim" -o "$WORK/synctex" \
    synctex_main.c synctex_parser.c synctex_parser_utils.c -lz -lm) || die "synctex build failed (static zlib installed?)"

# Smoke: compile a two-page document with the host Tectonic, then query both ways.
host_tectonic="$BIN/tectonic-$SYNCTEX_TRIPLE"
[ -x "$host_tectonic" ] || die "smoke test needs $host_tectonic"
doc="$WORK/smoke"
mkdir -p "$doc"
printf '\\documentclass{article}\n\\begin{document}\nfirst page\n\\newpage\nsecond page\n\\end{document}\n' >"$doc/smoke.tex"
(cd "$doc" && "$host_tectonic" -X compile smoke.tex --synctex >/dev/null 2>&1) || die "smoke compile failed"
view=$("$WORK/synctex" view -i "5:1:$doc/smoke.tex" -o "$doc/smoke.pdf")
echo "$view" | grep -q '^Page:2$' || die "synctex view smoke failed: $view"
edit=$("$WORK/synctex" edit -o "1:100:100:$doc/smoke.pdf")
echo "$edit" | grep -q '^Input:' && echo "$edit" | grep -q '^Line:' || die "synctex edit smoke failed: $edit"
got=$(sha "$WORK/synctex")
[ "$got" = "$SYNCTEX_SHA" ] || say "synctex: sha256 $got differs from the reference build (toolchain differs); smoke test passed"
install -m 755 "$WORK/synctex" "$dest"
say "synctex $SYNCTEX_TRIPLE: built and smoke-tested"
