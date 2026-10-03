#!/bin/sh
# Put the bundled engine sidecars in src-tauri/binaries/ (untracked).
#   maleficium-engine: built for this host from src-tauri/engine (Tectonic and
#     latexml linked from source, versions pinned by its Cargo.lock, on the
#     nightly pinned by its rust-toolchain.toml), then installed under the
#     Rust triple the app looks it up by. Peak memory is a few GB: cap it with
#     CARGO_BUILD_JOBS on a small machine.
#   SyncTeX: built for this host from pinned upstream source with our
#     c-auto.h shim, then smoke-tested (view -> Page:, edit -> Input:/Line:)
#     against a document the built engine compiles. The hash below is
#     what this recipe produced on Linux with gcc 15 + zlib 1.3.1; another
#     toolchain or OS differs, so a mismatch warns and the smoke test decides.
#     Hosts: Linux x86_64 (static, gcc), macOS arm64/x86_64 (cc, system
#     zlib), Windows x86_64 under MSYS2 MINGW64 (static, gcc + zlib).
# Idempotent: the engine build is incremental, and a SyncTeX binary with the
# pinned hash is left alone.
# Needs network, cargo (via rustup), a C and C++ compiler and libclang headers
# for the engine; SyncTeX also needs git and zlib (static on Linux and
# Windows).
# POSIX sh.
set -eu
unset CDPATH
ROOT=$(cd -- "$(dirname -- "$0")/.." && pwd -P)
BIN="$ROOT/src-tauri/binaries"
SYNCTEX_REPO=https://github.com/jlaurens/synctex.git
SYNCTEX_REV=04cf8e3e8665ff203248d7af78ee1129afbc1b64
SYNCTEX_SHA=5e7c245223faf271a3fa8b9963d8b5f223b18c99f0d88c5dbd97579c69ccf883


die() { echo "fetch-sidecars: $1" >&2; exit 1; }
say() { echo "fetch-sidecars: $1"; }
need() { command -v "$1" >/dev/null 2>&1 || die "missing tool: $1"; }
# macOS ships shasum, not sha256sum.
if command -v sha256sum >/dev/null 2>&1; then
    sha() { sha256sum "$1" | cut -d' ' -f1; }
else
    sha() { shasum -a 256 "$1" | cut -d' ' -f1; }
fi

# The SyncTeX sidecar is built for the host, named with the Rust triple the
# app looks it up by (core::sidecar_triple).
case "$(uname -s) $(uname -m)" in
    ("Linux x86_64") SYNCTEX_TRIPLE=x86_64-unknown-linux-gnu; EXE="" ;;
    ("Darwin arm64") SYNCTEX_TRIPLE=aarch64-apple-darwin; EXE="" ;;
    ("Darwin x86_64") SYNCTEX_TRIPLE=x86_64-apple-darwin; EXE="" ;;
    (MINGW64*" x86_64") SYNCTEX_TRIPLE=x86_64-pc-windows-msvc; EXE=".exe" ;;
    (*) die "no SyncTeX recipe for $(uname -s) $(uname -m)" ;;
esac

need cargo
mkdir -p "$BIN"
WORK=$(mktemp -d "${TMPDIR:-/tmp}/maleficium-sidecars-XXXXXX")
trap 'rm -rf "$WORK"' EXIT

say "engine $SYNCTEX_TRIPLE: building src-tauri/engine"
# The engine is its own workspace with its own toolchain pin, so build it from
# its directory. bindgen needs the C compiler's headers; point it at them when
# libclang does not find stddef.h on its own.
if [ -z "${BINDGEN_EXTRA_CLANG_ARGS:-}" ] && command -v gcc >/dev/null 2>&1; then
    gcc_inc=$(dirname "$(gcc -print-file-name=include/stddef.h)")
    [ -f "$gcc_inc/stddef.h" ] && export BINDGEN_EXTRA_CLANG_ARGS="-I$gcc_inc"
fi
(cd "$ROOT/src-tauri/engine" && cargo build --release) || die "engine build failed"
install -m 755 "$ROOT/src-tauri/engine/target/release/maleficium-engine$EXE" \
    "$BIN/maleficium-engine-$SYNCTEX_TRIPLE$EXE"
say "engine $SYNCTEX_TRIPLE: installed"

dest="$BIN/maleficium-synctex-$SYNCTEX_TRIPLE$EXE"
if [ -f "$dest" ] && [ "$(sha "$dest")" = "$SYNCTEX_SHA" ]; then
    say "synctex $SYNCTEX_TRIPLE present"
    exit 0
fi
# SHIM: extra headers MinGW lacks (scripts/synctex-shim/win). LIBS: the
# parser's Windows path helpers (PathFindFileNameA) live in shlwapi.
SHIM="-I$ROOT/scripts/synctex-shim"; LIBS="-lz -lm"
case "$SYNCTEX_TRIPLE" in
    (*apple-darwin) CC=cc; LINK="" ;;
    (*windows*) CC=gcc; LINK="-static"; SHIM="$SHIM -I$ROOT/scripts/synctex-shim/win"; LIBS="$LIBS -lshlwapi" ;;
    (*) CC=gcc; LINK="-static" ;;
esac
need git; need "$CC"
say "synctex $SYNCTEX_TRIPLE: building $SYNCTEX_REV"
src="$WORK/synctex-src"
git init -q "$src"
git -C "$src" fetch -q --depth 1 "$SYNCTEX_REPO" "$SYNCTEX_REV" || die "synctex source fetch failed"
git -C "$src" checkout -q FETCH_HEAD
# $LINK, $SHIM and $LIBS are unquoted on purpose: $LINK is empty on macOS,
# where -static is unsupported; the others hold several flags.
# shellcheck disable=SC2086
(cd "$src" && "$CC" -O2 $LINK -s -I. $SHIM -o "$WORK/synctex$EXE" \
    synctex_main.c synctex_parser.c synctex_parser_utils.c $LIBS) || die "synctex build failed (zlib installed?)"

# Smoke: compile a two-page document with the host engine, then query both ways.
host_engine="$BIN/maleficium-engine-$SYNCTEX_TRIPLE$EXE"
[ -x "$host_engine" ] || die "smoke test needs $host_engine"
doc="$WORK/smoke"
mkdir -p "$doc"
printf '\\documentclass{article}\n\\begin{document}\nfirst page\n\\newpage\nsecond page\n\\end{document}\n' >"$doc/smoke.tex"
(cd "$doc" && "$host_engine" compile smoke.tex --synctex >/dev/null 2>&1) || die "smoke compile failed"
# Queried as the app does (core/synctex.rs): from inside the output dir,
# absolute tex path, relative pdf. pwd -W gives MSYS2 the native C:/ form,
# which a native Windows binary needs. tr: Windows output ends in CRLF.
# -P: macOS's temp dir sits behind the /var -> /private/var symlink, and the
# engine records the resolved path.
native_doc=$(cd "$doc" && { pwd -W 2>/dev/null || pwd -P; })
view=$(cd "$doc" && "$WORK/synctex$EXE" view -i "5:1:$native_doc/smoke.tex" -o "smoke.pdf" | tr -d '\r')
echo "$view" | grep -q '^Page:2$' || die "synctex view smoke failed: $view"
edit=$(cd "$doc" && "$WORK/synctex$EXE" edit -o "1:100:100:smoke.pdf" | tr -d '\r')
{ echo "$edit" | grep -q '^Input:' && echo "$edit" | grep -q '^Line:'; } || die "synctex edit smoke failed: $edit"
got=$(sha "$WORK/synctex$EXE")
[ "$got" = "$SYNCTEX_SHA" ] || say "synctex: sha256 $got differs from the reference build (toolchain differs); smoke test passed"
install -m 755 "$WORK/synctex$EXE" "$dest"
say "synctex $SYNCTEX_TRIPLE: built and smoke-tested"
