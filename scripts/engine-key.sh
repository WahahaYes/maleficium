#!/bin/sh
# engine-key.sh -- print the identity of the built engine: a checksum of every
# file under src-tauri/engine (sources, Cargo.lock, the nightly pin; the
# engine is its own workspace with no path dependencies, build output
# excluded) plus scripts/build-engine.sh, which sets the build's flags.
# build-engine.sh stamps the installed binary with it so an unchanged engine
# is not rebuilt, and CI keys its engine cache on it. POSIX sh.
set -eu
unset CDPATH
ROOT=$(cd -- "$(dirname -- "$0")/.." && pwd -P)
cd "$ROOT"
sum=$({
    find src-tauri/engine -name target -prune -o -type f -print | LC_ALL=C sort | while IFS= read -r f; do
        printf '%s\n' "$f"
        tr -d '\r' <"$f"
    done
    printf 'scripts/build-engine.sh\n'
    tr -d '\r' <scripts/build-engine.sh
} | cksum | cut -d' ' -f1)
printf 'engine-%s\n' "$sum"
