#!/bin/sh
# dumps-key.sh -- print the identity of the latexml format dumps: the latexml
# version pinned by src-tauri/engine/Cargo.lock plus a checksum of the bundle
# URL pinned in src-tauri/core/src/engine.rs. A dump is valid for exactly one
# such pair, so build-engine.sh stamps the dumps with it and CI keys its cache
# on it. Also: `dumps-key.sh url` prints the pinned bundle URL.
# POSIX sh.
set -eu
unset CDPATH
ROOT=$(cd -- "$(dirname -- "$0")/.." && pwd -P)
url=$(sed -n 's/^pub const BUNDLE_URL: &str = "\(.*\)";$/\1/p' "$ROOT/src-tauri/core/src/engine.rs")
[ -n "$url" ] || { echo "dumps-key: BUNDLE_URL not found in src-tauri/core/src/engine.rs" >&2; exit 1; }
if [ "${1:-}" = url ]; then
    printf '%s\n' "$url"
    exit 0
fi
ver=$(awk '$0 == "name = \"latexml\"" { getline; gsub(/[^0-9.]/, "", $3); print $3; exit }' "$ROOT/src-tauri/engine/Cargo.lock")
[ -n "$ver" ] || { echo "dumps-key: latexml not found in src-tauri/engine/Cargo.lock" >&2; exit 1; }
sum=$(printf '%s' "$url" | cksum | cut -d' ' -f1)
printf 'latexml-%s-%s\n' "$ver" "$sum"
