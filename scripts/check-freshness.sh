#!/bin/sh
# Warn when the dev loop may run stale artifacts: a debug app binary older
# than the sources, or a missing sidecar for this host. With --all, also
# compare the packaged frontend dist against the sources (release builds).
# With --strict, a missing sidecar exits 1 instead of warning.
# Default mode always exits 0: the toolchain rebuilds what it can; these
# warnings exist so a stale-looking run is diagnosed in seconds.
# POSIX sh (npm runs scripts under sh/dash).
set -eu
unset CDPATH
ROOT=$(cd -- "$(dirname -- "$0")/.." && pwd)
BIN="$ROOT/src-tauri/target/debug/maleficium"
ALL=0
STRICT=0
for arg in "$@"; do
    case "$arg" in
        --all) ALL=1 ;;
        --strict) STRICT=1 ;;
    esac
done

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
    warn "freshness: no debug app binary yet ($BIN missing) — the first dev:desktop build creates it."
else
    # shellcheck disable=SC2046
    stale=$(newer_than "$BIN" "$ROOT/src" "$ROOT/src-tauri/src" "$ROOT/src-tauri/core" "$ROOT/src-tauri/mcp" "$ROOT/src-tauri/structure" "$ROOT/src-tauri/events" "$ROOT/src-tauri/index" "$ROOT/src-tauri/templates" "$ROOT/src-tauri/Cargo.toml" "$ROOT/src-tauri/Cargo.lock" "$ROOT/src-tauri/tauri.conf.json" "$ROOT/src-tauri/capabilities" "$ROOT/src-tauri/build.rs" "$ROOT/package.json" "$ROOT/vite.config.ts" "$ROOT/tsconfig.json" "$ROOT/index.html" | head -n 10)
    if [ -n "$stale" ]; then
        warn "freshness WARN: sources newer than $BIN — tauri dev rebuilds, but if the app looks stale: stop, rebuild, restart, retest. Newest:"
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
    warn "freshness: unknown host $os/$arch — cannot verify sidecars; compile fails honestly if one is missing."
else
    exe=""
    case "$os" in MINGW* | MSYS* | CYGWIN*) exe=".exe" ;; esac
    sidecar="$ROOT/src-tauri/binaries/maleficium-engine-$triple$exe"
    if [ -x "$sidecar" ]; then
        echo "freshness: engine sidecar present for $triple."
        # shellcheck disable=SC2046
        stale_engine=$(newer_than "$sidecar" "$ROOT/src-tauri/engine/src" "$ROOT/src-tauri/engine/Cargo.toml" "$ROOT/src-tauri/engine/Cargo.lock" | head -n 10)
        if [ -n "$stale_engine" ]; then
            warn "FRESHNESS WARN: engine sidecar older than engine sources — run: sh scripts/build-engine.sh"
        fi
    else
        warn "freshness ERROR: engine sidecar missing or not executable: src-tauri/binaries/maleficium-engine-$triple$exe — run: sh scripts/build-engine.sh"
        if [ "$STRICT" -eq 1 ]; then exit 1; fi
    fi
fi

if [ "$ALL" -eq 1 ]; then
    if [ ! -f "$ROOT/dist/index.html" ]; then
        warn "freshness: dist/index.html missing — npm run build creates it (tauri build runs it first)."
    else
        # shellcheck disable=SC2046
        stale_dist=$(newer_than "$ROOT/dist/index.html" "$ROOT/src" "$ROOT/index.html" | head -n 10)
        if [ -n "$stale_dist" ]; then
            warn "freshness WARN: sources newer than dist/ — packaging now would ship stale UI; run npm run build first. Newest:"
            warn "$stale_dist"
        else
            echo "freshness: dist newer than sources."
        fi
    fi
fi
exit 0
