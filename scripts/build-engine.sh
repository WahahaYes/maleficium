#!/bin/sh
# Build the bundled engine, maleficium-engine, for this host and put it in
# src-tauri/binaries/ (untracked) under the Rust triple the app looks it up by
# (core::sidecar_triple). It is the only sidecar: Tectonic and latexml are
# linked from source (versions pinned by src-tauri/engine/Cargo.lock) and the
# SyncTeX parser is vendored (src-tauri/engine/synctex), all on the nightly
# pinned by src-tauri/engine/rust-toolchain.toml. The build needs a few GB of
# RAM: cap it with CARGO_BUILD_JOBS on a small machine.
# Then the latexml format dumps (`maleficium-engine dump`, from the bundle pinned
# in src-tauri/core/src/engine.rs) into src-tauri/resources/dumps/, which the
# app bundles as resources; they are stamped with scripts/dumps-key.sh so an
# unchanged pin is not regenerated (CI caches that directory). Generating them
# needs the network (about 11 MB of bundle files, a couple of minutes).
# Then a smoke test: compile a two-page document and query it both ways
# (view -> Page:, edit -> Input:/Line:) through the built engine.
# Idempotent: the cargo build is incremental, and skipped when the installed
# binary is stamped with the current scripts/engine-key.sh (CI restores it from
# a cache keyed on that).
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
# macOS: link the C libraries (ICU, freetype, graphite2, libpng, ...) statically
# from .a files (Homebrew's, plus graphite2 built below), so the engine needs no Homebrew on the user's Mac.
# tectonic_dep_support's semi-static mode links every library whose lib<name>.a
# is on the pkg-config path statically and leaves the system ones (libc++, libz)
# dynamic; PKG_CONFIG_ALL_STATIC makes pkg-config report the private
# dependencies too. package-smoke.py fails if the engine links a non-system dylib.
# Homebrew's graphite2 ships only a dylib, so build a static one from the pinned
# release (small, cmake) and put it first on the pkg-config path.
if [ "$(uname -s)" = Darwin ]; then
    export TECTONIC_PKGCONFIG_FORCE_SEMI_STATIC=1 PKG_CONFIG_ALL_STATIC=1
    g2_prefix="${CARGO_TARGET_DIR:-$ROOT/src-tauri/engine/target}/static-graphite2"
    if [ ! -f "$g2_prefix/lib/libgraphite2.a" ]; then
        need cmake
        say "building a static graphite2 1.3.14"
        g2_src=$(mktemp -d "${TMPDIR:-/tmp}/graphite2-XXXXXX")
        curl -fsSL -o "$g2_src/g.tgz" https://github.com/silnrsi/graphite/releases/download/1.3.14/graphite2-1.3.14.tgz \
            || die "cannot download graphite2"
        [ "$(shasum -a 256 "$g2_src/g.tgz" | cut -d' ' -f1)" = f99d1c13aa5fa296898a181dff9b82fb25f6cc0933dbaa7a475d8109bd54209d ] \
            || die "graphite2 checksum mismatch"
        tar -xzf "$g2_src/g.tgz" -C "$g2_src"
        cmake -S "$g2_src/graphite2-1.3.14" -B "$g2_src/build" -DBUILD_SHARED_LIBS=OFF -DCMAKE_BUILD_TYPE=Release \
            -DCMAKE_INSTALL_PREFIX="$g2_prefix" >/dev/null || die "graphite2 configure failed"
        cmake --build "$g2_src/build" >/dev/null && cmake --install "$g2_src/build" >/dev/null || die "graphite2 build failed"
        rm -rf "$g2_src"
    fi
    PKG_CONFIG_PATH="$g2_prefix/lib/pkgconfig${PKG_CONFIG_PATH:+:$PKG_CONFIG_PATH}"
    export PKG_CONFIG_PATH
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

engine="$BIN/maleficium-engine-$TRIPLE$EXE"
engine_key=$(sh "$ROOT/scripts/engine-key.sh") || die "cannot read the engine key"
if [ -x "$engine" ] && [ "$(cat "$BIN/.engine-stamp" 2>/dev/null)" = "$engine_key" ]; then
    say "$TRIPLE: engine up to date ($engine_key)"
else
    say "$TRIPLE: building src-tauri/engine ($engine_key)"
    # The engine is its own workspace with its own toolchain pin, so build it
    # from its directory.
    rm -f "$BIN/.engine-stamp"
    (cd "$ROOT/src-tauri/engine" && cargo build --release) || die "engine build failed"
    mkdir -p "$BIN"
    install -m 755 "${CARGO_TARGET_DIR:-$ROOT/src-tauri/engine/target}/release/maleficium-engine$EXE" "$engine"
    printf '%s\n' "$engine_key" >"$BIN/.engine-stamp"
    say "$TRIPLE: installed"
fi

DUMPS="$ROOT/src-tauri/resources/dumps"
dumps_key=$(sh "$ROOT/scripts/dumps-key.sh") || die "cannot read the dump key"
bundle_url=$(sh "$ROOT/scripts/dumps-key.sh" url) || die "cannot read the bundle pin"
if [ "$(cat "$DUMPS/.stamp" 2>/dev/null)" = "$dumps_key" ] && ls "$DUMPS"/latex.*.dump.txt >/dev/null 2>&1; then
    say "dumps up to date ($dumps_key)"
else
    say "generating format dumps ($dumps_key)"
    mkdir -p "$DUMPS"
    rm -f "$DUMPS"/*.dump.txt "$DUMPS"/*.version "$DUMPS/.stamp"
    "$engine" dump --out "$DUMPS" --bundle "$bundle_url" \
        --cache "$ROOT/src-tauri/engine/target/dump-cache" >/dev/null 2>&1 || die "dump failed"
    ls "$DUMPS"/plain.*.dump.txt "$DUMPS"/latex.*.dump.txt "$DUMPS"/texlive.*.version >/dev/null \
        || die "dump wrote no dumps"
    printf '%s\n' "$dumps_key" >"$DUMPS/.stamp"
fi

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
# Convert with no TeX installed: the engine answers latexml's kpsewhich calls
# from the pinned bundle.
printf '\\documentclass{article}\n\\begin{document}\n\\section{Smoke}\nHello $x^2$.\n\\end{document}\n' >"$doc/convert.tex"
(cd "$doc" && "$engine" convert convert.tex --out convert.html --bundle "$bundle_url" \
    --cache "$ROOT/src-tauri/engine/target/dump-cache" --dumps "$DUMPS" >/dev/null 2>&1) || die "smoke convert failed"
grep -q 'ltx_title_section' "$doc/convert.html" || die "smoke convert produced no section"
say "$TRIPLE: smoke ok"
