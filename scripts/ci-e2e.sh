#!/bin/sh
# The interactive-papers e2e suites, one after another, as CI runs them (the
# e2e job in .github/workflows/build.yml): export (with the reader and the
# wasm cells in Chromium, Firefox and WebKit), interactive, poster and
# approval. Stops at the first failing suite and exits non-zero.
# Needs what the suites need (e2e/README.md): the engine and its format dumps
# from scripts/build-engine.sh, python3 with jsonschema, npm ci and
# playwright-core's three browsers, Xvfb, and openssl. Linux only.
#   sh scripts/ci-e2e.sh [suite...]     default: export interactive poster approval
# CI_E2E_LOGS=<dir> keeps each suite's output as <suite>.log (default: a temp
# dir, removed on exit); CI_E2E_TIMEOUT caps each suite in seconds (1800).
# POSIX sh.
set -eu
unset CDPATH
ROOT=$(cd -- "$(dirname -- "$0")/.." && pwd -P)
cd "$ROOT"

die() { echo "ci-e2e: $1" >&2; exit 1; }
say() { echo "ci-e2e: $1"; }

[ "$(uname -s)" = Linux ] || die "the suites run on Linux only"
[ $# -gt 0 ] || set -- export interactive poster approval
for s in "$@"; do
    [ -f "e2e/$s-run.sh" ] || die "no suite e2e/$s-run.sh"
done

if [ -n "${CI_E2E_LOGS:-}" ]; then
    LOGS=$CI_E2E_LOGS
    mkdir -p "$LOGS"
else
    LOGS=$(mktemp -d "${TMPDIR:-/tmp}/ci-e2e-XXXXXX")
    trap 'rm -rf "$LOGS"' EXIT
fi
TIMEOUT=${CI_E2E_TIMEOUT:-1800}

# The engine the suites compile with is target/debug/maleficium-engine, which
# cargo's build script (tauri-build) copies from src-tauri/binaries/ when that
# file changes; the copy wins over binaries/ afterwards. Build first, then
# check the copy is the engine build-engine.sh installed, not a stale one.
TRIPLE=$(rustc -vV | sed -n 's/^host: //p')
ENGINE="src-tauri/binaries/maleficium-engine-$TRIPLE"
[ -x "$ENGINE" ] || die "no engine at $ENGINE: run scripts/build-engine.sh"
ls src-tauri/resources/dumps/latex.*.dump.txt >/dev/null 2>&1 \
    || die "no format dumps in src-tauri/resources/dumps: run scripts/build-engine.sh"
say "building the app and sidecar"
cargo build -q --manifest-path src-tauri/Cargo.toml --bin maleficium-mcp --bin maleficium \
    || die "cannot build the app and sidecar"
COPY="${CARGO_TARGET_DIR:-$ROOT/src-tauri/target}/debug/maleficium-engine"
if [ -e "$COPY" ]; then
    cmp -s "$COPY" "$ENGINE" || die "$COPY differs from $ENGINE: touch $ENGINE and rebuild"
fi
say "engine: $(sha256sum "$ENGINE" | cut -c1-16)"

for s in "$@"; do
    log="$LOGS/$s.log"
    say "$s: running (log $log)"
    start=$(date +%s)
    # The suite's status, past the tee that streams its output.
    # (|| keeps set -e from ending the group before the status is written.)
    {
        st=0
        timeout "$TIMEOUT" bash "e2e/$s-run.sh" 2>&1 || st=$?
        echo "$st" >"$LOGS/$s.status"
    } | tee "$log"
    status=$(cat "$LOGS/$s.status")
    oks=$(grep -c '^ok: ' "$log" || true)
    fails=$(grep -c '^FAIL: ' "$log" || true)
    say "$s: $oks ok, $fails FAIL, exit $status, $(($(date +%s) - start))s"
    [ "$status" -eq 0 ] || die "$s failed"
done
say "all suites passed: $*"
