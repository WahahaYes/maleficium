#!/bin/bash
# Headless CLI export test: runs `maleficium export` without a display,
# compiling a small fixture and checking the bundle has index.html,
# manifest.json, paper.pdf (folder profile) or a single index.html
# (single-file profile). Exit codes: 0 success, 1 compile failure,
# 2 usage error, 3 warnings with --deny-warnings.
set -euo pipefail

DEVROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN="${CARGO_TARGET_DIR:-$DEVROOT/src-tauri/target}/debug/maleficium"
FIXTURE="$DEVROOT/e2e/fixtures/simple"
SCRATCH="$(mktemp -d /tmp/maleficium-cli-export-XXXXXX)"
trap 'rm -rf "$SCRATCH"' EXIT

fail() { echo "FAIL: $1"; exit 1; }
pass() { echo "ok: $1"; }

# Build the main binary (not maleficium-mcp)
cargo build -q --manifest-path "$DEVROOT/src-tauri/Cargo.toml" --bin maleficium \
  || fail "cannot build maleficium"
[ -x "$BIN" ] || fail "binary missing after build: $BIN"

# Prepare a clean project copy (the fixture has no maleficium-interactive.sty)
cp -r "$FIXTURE" "$SCRATCH/proj"
cd "$SCRATCH/proj"
git init -q
git add -A
git -c user.email=driver@local -c user.name=driver commit -qm "fixture"
ROOT="$SCRATCH/proj"

# Use the warm engine cache
export XDG_CACHE_HOME="${DRIVER_CACHE:-$HOME/.cache}"
export XDG_DATA_HOME="$SCRATCH/data"

# Unset DISPLAY and WAYLAND_DISPLAY to ensure headless operation
unset DISPLAY
unset WAYLAND_DISPLAY

# Test 1: folder profile export
OUT_FOLDER="$SCRATCH/out-folder"
env -u DISPLAY -u WAYLAND_DISPLAY "$BIN" export "$ROOT" --main main.tex --out "$OUT_FOLDER" \
  || fail "folder profile export failed"

# Check required files exist
[ -f "$OUT_FOLDER/index.html" ] || fail "folder: index.html missing"
[ -f "$OUT_FOLDER/manifest.json" ] || fail "folder: manifest.json missing"
[ -f "$OUT_FOLDER/paper.pdf" ] || fail "folder: paper.pdf missing"
pass "folder profile: index.html, manifest.json, paper.pdf present"

# Test 2: single-file profile export
OUT_SINGLE="$SCRATCH/out-single.html"
env -u DISPLAY -u WAYLAND_DISPLAY "$BIN" export "$ROOT" --main main.tex --out "$OUT_SINGLE" --single-file \
  || fail "single-file profile export failed"

[ -f "$OUT_SINGLE" ] || fail "single-file: output file missing"
# Check it's a single HTML file containing the manifest and pdf
grep -q 'id="mfw-manifest"' "$OUT_SINGLE" || fail "single-file: manifest not inlined"
grep -q 'id="pdf-link"' "$OUT_SINGLE" || fail "single-file: pdf link not inlined"
pass "single-file profile: self-contained index.html with manifest and pdf"

# Test 3: --deny-warnings on a clean fixture (should succeed, no warnings)
OUT_CLEAN="$SCRATCH/out-clean"
env -u DISPLAY -u WAYLAND_DISPLAY "$BIN" export "$ROOT" --main main.tex --out "$OUT_CLEAN" --deny-warnings \
  || fail "--deny-warnings failed on clean fixture"
pass "--deny-warnings succeeds when no warnings"

# Test 4: usage error (missing --main)
if env -u DISPLAY -u WAYLAND_DISPLAY "$BIN" export "$ROOT" --out "$SCRATCH/x" 2>/dev/null; then
    fail "missing --main should be usage error (exit 2)"
else
    code=$?
    [ "$code" -eq 2 ] || fail "missing --main exit code should be 2, got $code"
    pass "missing --main exits with code 2"
fi

# Test 5: usage error (missing --out)
if env -u DISPLAY -u WAYLAND_DISPLAY "$BIN" export "$ROOT" --main main.tex 2>/dev/null; then
    fail "missing --out should be usage error (exit 2)"
else
    code=$?
    [ "$code" -eq 2 ] || fail "missing --out exit code should be 2, got $code"
    pass "missing --out exits with code 2"
fi

# Test 6: usage error (unknown argument)
if env -u DISPLAY -u WAYLAND_DISPLAY "$BIN" export "$ROOT" --main main.tex --out "$SCRATCH/x" --unknown 2>/dev/null; then
    fail "unknown argument should be usage error (exit 2)"
else
    code=$?
    [ "$code" -eq 2 ] || fail "unknown argument exit code should be 2, got $code"
    pass "unknown argument exits with code 2"
fi

# Test 7: compile failure (nonexistent main file)
if env -u DISPLAY -u WAYLAND_DISPLAY "$BIN" export "$ROOT" --main nonexistent.tex --out "$SCRATCH/x" 2>/dev/null; then
    fail "nonexistent main should be compile failure (exit 1)"
else
    code=$?
    [ "$code" -eq 1 ] || fail "nonexistent main exit code should be 1, got $code"
    pass "nonexistent main exits with code 1"
fi

# Test 8: destination inside project is refused
if env -u DISPLAY -u WAYLAND_DISPLAY "$BIN" export "$ROOT" --main main.tex --out "$ROOT/inside" 2>/dev/null; then
    fail "destination inside project should be refused"
else
    code=$?
    [ "$code" -ne 0 ] || fail "destination inside project should fail"
    pass "destination inside project is refused"
fi

echo ""
echo "CLI EXPORT TESTS PASSED"
exit 0
