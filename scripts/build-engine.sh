#!/bin/sh
# Build the bundled engine, maleficium-engine, for this host and put it in
# src-tauri/binaries/ (untracked) under the Rust triple the app looks it up by
# (core::sidecar_triple). It is the only sidecar: Tectonic and latexml are
# linked from source (versions pinned by src-tauri/engine/Cargo.lock) and the
# SyncTeX parser is vendored (src-tauri/engine/synctex), all on the nightly
# pinned by src-tauri/engine/rust-toolchain.toml. The build needs a few GB of
# RAM: cap it with CARGO_BUILD_JOBS on a small machine.
# Then a smoke test: compile a two-page document and query it both ways
# (view -> Page:, edit -> Input:/Line:) through the built engine.
# Idempotent: the cargo build is incremental.
# Needs network, cargo (via rustup), a C and C++ compiler, and libclang headers.
# POSIX sh.
set -eu
unset CDPATH
ROOT=$(cd -- "$(dirname -- "$0")/.." && pwd -P)
BIN="$ROOT/src-tauri/binaries"

die() { echo "build-engine: $1" >&2; exit 1; }
say() { echo "build-engine: $1"; }
need() { command -v "$1" >/dev/null 2>&1 || die "missing tool: $1"; }

case "$(uname -s) $(uname -m)" in
    ("Linux x86_64") TRIPLE=x86_64-unknown-linux-gnu; EXE="" ;;
    ("Darwin arm64") TRIPLE=aarch64-apple-darwin; EXE="" ;;
    ("Darwin x86_64") TRIPLE=x86_64-apple-darwin; EXE="" ;;
    (MINGW64*" x86_64") TRIPLE=x86_64-pc-windows-msvc; EXE=".exe" ;;
    (*) die "no engine build for $(uname -s) $(uname -m)" ;;
esac

# An MSYS2 shell does not inherit rustup's cargo from the Windows PATH.
if ! command -v cargo >/dev/null 2>&1 && [ -n "${USERPROFILE:-}" ] && command -v cygpath >/dev/null 2>&1; then
    PATH="$PATH:$(cygpath -u "$USERPROFILE")/.cargo/bin"
fi
need cargo

# Homebrew's icu4c is keg-only, so pkg-config does not see it unless asked.
# Set here, not in the CI workflow: a later setup step overwrites that env.
if [ "$(uname -s)" = Darwin ] && command -v brew >/dev/null 2>&1; then
    icu_pc="$(brew --prefix icu4c 2>/dev/null)/lib/pkgconfig"
    if [ -d "$icu_pc" ]; then
        PKG_CONFIG_PATH="$icu_pc${PKG_CONFIG_PATH:+:$PKG_CONFIG_PATH}"
        export PKG_CONFIG_PATH
    fi
fi
# On Linux bindgen needs the C compiler's headers; point it at them when
# libclang does not find stddef.h on its own. Not elsewhere: on Windows a
# MinGW gcc on PATH would feed bindgen headers that clash with MSVC's.
if [ "$(uname -s)" = Linux ] && [ -z "${BINDGEN_EXTRA_CLANG_ARGS:-}" ] && command -v gcc >/dev/null 2>&1; then
    gcc_inc=$(dirname "$(gcc -print-file-name=include/stddef.h)")
    [ -f "$gcc_inc/stddef.h" ] && export BINDGEN_EXTRA_CLANG_ARGS="-I$gcc_inc"
fi
# latexml's kpathsea crate wants libkpathsea or a kpsewhich on PATH at build
# time. The product ships no TeX distribution, so build without either; the
# package resolution it would give is a separate decision.
export KPATHSEA_SKIP_TOOLCHAIN_CHECK=1

say "$TRIPLE: building src-tauri/engine"
# The engine is its own workspace with its own toolchain pin, so build it from
# its directory.
(cd "$ROOT/src-tauri/engine" && cargo build --release) || die "engine build failed"
mkdir -p "$BIN"
install -m 755 "${CARGO_TARGET_DIR:-$ROOT/src-tauri/engine/target}/release/maleficium-engine$EXE" \
    "$BIN/maleficium-engine-$TRIPLE$EXE"
say "$TRIPLE: installed"

engine="$BIN/maleficium-engine-$TRIPLE$EXE"
doc=$(mktemp -d "${TMPDIR:-/tmp}/maleficium-engine-smoke-XXXXXX")
trap 'rm -rf "$doc"' EXIT
printf '\\documentclass{article}\n\\begin{document}\nfirst page\n\\newpage\nsecond page\n\\end{document}\n' >"$doc/smoke.tex"
(cd "$doc" && "$engine" compile smoke.tex --synctex >/dev/null 2>&1) || die "smoke compile failed"
# Queried as the app does (core/synctex.rs): from inside the output dir,
# absolute tex path, relative pdf. pwd -W gives MSYS2 the native C:/ form,
# which a native Windows binary needs. tr: Windows output ends in CRLF.
# -P: macOS's temp dir sits behind the /var -> /private/var symlink, and the
# engine records the resolved path.
native_doc=$(cd "$doc" && { pwd -W 2>/dev/null || pwd -P; })
view=$(cd "$doc" && "$engine" synctex view -i "5:1:$native_doc/smoke.tex" -o "smoke.pdf" | tr -d '\r')
echo "$view" | grep -q '^Page:2$' || die "synctex view smoke failed: $view"
edit=$(cd "$doc" && "$engine" synctex edit -o "1:100:100:smoke.pdf" | tr -d '\r')
{ echo "$edit" | grep -q '^Input:' && echo "$edit" | grep -q '^Line:'; } || die "synctex edit smoke failed: $edit"
say "$TRIPLE: smoke ok"
