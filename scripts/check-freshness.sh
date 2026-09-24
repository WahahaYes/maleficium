#!/bin/sh
# Warn when the dev loop may run stale artifacts: a debug app binary older
# than the sources, or a missing sidecar for this host. With --all, also
# compare the packaged frontend dist against the sources (release builds).
# Always exits 0: the toolchain rebuilds what it can; these warnings exist
# so a stale-looking run is diagnosed in seconds.
# POSIX sh (npm runs scripts under sh/dash).
set -eu
unset CDPATH
ROOT=$(cd -- "$(dirname -- "$0")/.." && pwd)
BIN="$ROOT/src-tauri/target/debug/tauri-app"
ALL=0
if [ "${1:-}" = "--all" ]; then ALL=1; fi

warn() { printf '%s\n' "$1" >&2; }

newer_than() {
    ref="$1"
    shift
    for root in "$@"; do
        [ -e "$root" ] || continue
        if [ -f "$root" ]; then
            [ "$root" -nt "$ref" ] && printf '%s\n' "$root"
        else
            find "$root" -type f -newer "$ref" 2>/dev/null || true
        fi
    done
}

if [ ! -x "$BIN" ]; then
    warn "FRESHNESS: no debug app binary yet ($BIN missing) — the first dev:desktop build creates it."
else
    # shellcheck disable=SC2046
    stale=$(newer_than "$BIN" "$ROOT/src" "$ROOT/src-tauri/src" "$ROOT/src-tauri/structure" "$ROOT/src-tauri/Cargo.toml" "$ROOT/src-tauri/Cargo.lock" "$ROOT/src-tauri/tauri.conf.json" "$ROOT/src-tauri/capabilities" "$ROOT/src-tauri/build.rs" "$ROOT/package.json" "$ROOT/vite.config.ts" "$ROOT/tsconfig.json" "$ROOT/index.html" | head -n 10)
    if [ -n "$stale" ]; then
        warn "FRESHNESS WARN: sources newer than $BIN — tauri dev rebuilds, but if the app looks stale: stop, rebuild, restart, retest. Newest:"
        warn "$stale"
    else
        echo "freshness: app binary newer than sources."
    fi
fi

os=$(uname -s 2>/dev/null || echo unknown)
arch=$(uname -m 2>/dev/null || echo unknown)
case "$os/$arch" in
    Linux/x86_64) triple="x86_64-unknown-linux-gnu" ;;
    Linux/aarch64) triple="aarch64-unknown-linux-musl" ;;
    Darwin/arm64) triple="aarch64-apple-darwin" ;;
    Darwin/x86_64) triple="x86_64-apple-darwin" ;;
    MINGW*/x86_64 | MSYS*/x86_64 | CYGWIN*/x86_64) triple="x86_64-pc-windows-msvc" ;;
    *) triple="" ;;
esac
if [ -z "$triple" ]; then
    warn "FRESHNESS: unknown host $os/$arch — cannot verify sidecars; compile fails honestly if one is missing."
else
    exe=""
    case "$os" in MINGW* | MSYS* | CYGWIN*) exe=".exe" ;; esac
    if [ -x "$ROOT/src-tauri/binaries/maleficium-tectonic-$triple$exe" ]; then
        echo "freshness: tectonic sidecar present for $triple."
    else
        warn "FRESHNESS ERROR: tectonic sidecar missing or not executable: src-tauri/binaries/maleficium-tectonic-$triple$exe — run: sh scripts/fetch-sidecars.sh"
    fi
    if [ -x "$ROOT/src-tauri/binaries/maleficium-synctex-$triple$exe" ]; then
        echo "freshness: synctex sidecar present for $triple."
    else
        warn "FRESHNESS WARN: synctex sidecar missing for $triple — SyncTeX degrades to an honest no-match on unbuilt triples (x86_64 Linux: sh scripts/fetch-sidecars.sh)."
    fi
fi

if [ "$ALL" -eq 1 ]; then
    if [ ! -f "$ROOT/dist/index.html" ]; then
        warn "FRESHNESS: dist/index.html missing — npm run build creates it (tauri build runs it first)."
    else
        # shellcheck disable=SC2046
        stale_dist=$(newer_than "$ROOT/dist/index.html" "$ROOT/src" "$ROOT/index.html" | head -n 10)
        if [ -n "$stale_dist" ]; then
            warn "FRESHNESS WARN: sources newer than dist/ — packaging now would ship stale UI; run npm run build first. Newest:"
            warn "$stale_dist"
        else
            echo "freshness: dist newer than sources."
        fi
    fi
fi
exit 0
