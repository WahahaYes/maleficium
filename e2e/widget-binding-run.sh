#!/bin/bash
# Widget-binding proof: the playground paper compiled to a PDF sidecar and
# converted to HTML through the \interactive* binding joins widget for widget.
#
# Steps, all in scratch dirs under OS tmp (nothing is written beside the
# fixture; the engine drops CSS next to its input):
#   1. engine `compile` of the fixture, cached files only -> main.mfw
#   2. engine `convert` of a second copy of the fixture, inside bwrap with a
#      writable tmpfs /tmp (without one the HTML silently comes out as raw
#      XML) and the ip.58.1 kpsewhich stand-in first on PATH
#   3. count placeholders and sidecar lines, then run the Rust join test.
#
# TEMPORARY PROTOTYPE: step 2 uses the spike's kpsewhich stand-in and its flat
# file directory because the prebuilt engine has no native resolver yet. Drop
# the shim when the engine resolves embedded/maleficium-interactive.sty.rhai
# itself. Needs bwrap, python3, a warm Tectonic cache and the spike tree.
set -euo pipefail

DEVROOT="$(cd "$(dirname "$0")/.." && pwd)"
ENGINE="${ENGINE:-$DEVROOT/src-tauri/binaries/maleficium-engine-x86_64-unknown-linux-gnu}"
FIXTURE="$DEVROOT/e2e/fixtures/playground"
SPIKE="${SPIKE:-/var/tmp/ip-native-spike/ip581}"
CACHE="${TECTONIC_CACHE_DIR:-$HOME/.cache/io.github.wahahayes.maleficium/maleficium-tectonic/$(sed -n 's/^pub const BUNDLE_DIGEST: &str = "\([0-9a-f]*\)";/\1/p' "$DEVROOT/src-tauri/core/src/engine.rs")}"
BUNDLE_URL="https://data1b.fullyjustified.net/tlextras-2022.0r0.tar"
SCRATCH="$(mktemp -d)"
trap 'rm -rf "$SCRATCH"' EXIT

fail() { echo "FAIL: $1"; exit 1; }
pass() { echo "ok: $1"; }

[ -x "$ENGINE" ] || fail "engine missing: $ENGINE"
[ -d "$SPIKE/shim" ] || fail "kpsewhich stand-in tree missing: $SPIKE"

# 1. the sidecar, from the real package and a real compile
mkdir -p "$SCRATCH/compile/pdf"
cp -r "$FIXTURE/." "$SCRATCH/compile/"
cp "$DEVROOT/embed-runtime/tex/maleficium-interactive.sty" "$SCRATCH/compile/"
(cd "$SCRATCH/compile" && TECTONIC_CACHE_DIR="$CACHE" "$ENGINE" compile main.tex \
  --outdir "$SCRATCH/compile/pdf" -C -b "$BUNDLE_URL" >"$SCRATCH/compile.log" 2>&1) \
  || { tail -20 "$SCRATCH/compile.log"; fail "playground does not compile from the cache"; }
MFW="$SCRATCH/compile/pdf/main.mfw"
[ -s "$MFW" ] || fail "compile left no main.mfw"
RECORDS="$(grep -c '^widget|' "$MFW")"
pass "compile wrote main.mfw with $RECORDS widget records"

# 2. the article, through the binding (served by the stand-in's flat dir)
cp "$DEVROOT/src-tauri/engine/embedded/maleficium-interactive.sty.rhai" "$SPIKE/files/"
mkdir -p "$SCRATCH/convert/out"
cp -r "$FIXTURE/." "$SCRATCH/convert/"
: >"$SPIKE/lookups.log"
(cd "$SPIKE" && bwrap --ro-bind / / --dev /dev --proc /proc --bind /var/tmp /var/tmp \
  --tmpfs /tmp --bind "$SCRATCH" "$SCRATCH" --tmpfs /usr/share/texlive \
  --setenv PATH "$SPIKE/shim:/usr/bin:/bin" \
  --setenv LATEXML_DUMP_DIR "$SPIKE/gen/resources/dumps" \
  --setenv MALEFICIUM_DUMP_DIR "$DEVROOT/src-tauri/resources/dumps" \
  "$ENGINE" convert "$SCRATCH/convert/main.tex" --out "$SCRATCH/convert/out/main.html" \
  --bundle "$BUNDLE_URL" --cache "$SCRATCH/bundle-cache" \
  >"$SCRATCH/convert.log" 2>&1) || { cat "$SCRATCH/convert.log"; fail "convert failed"; }
HTML="$SCRATCH/convert/out/main.html"
head -c 15 "$HTML" | grep -q '<!DOCTYPE html' || fail "output is not HTML (post-processing skipped)"
grep -q 'Conversion complete: No obvious problems' "$SCRATCH/convert.log" \
  || { cat "$SCRATCH/convert.log"; fail "conversion reported problems"; }
pass "convert is clean (no undefined macros, \\includegraphics resolves)"
PLACEHOLDERS="$(grep -o 'class="ltx_text m-widget m-widget-[a-z]*"' "$HTML" | wc -l)"
[ "$PLACEHOLDERS" = "$RECORDS" ] || fail "$PLACEHOLDERS placeholders, $RECORDS records"
pass "$PLACEHOLDERS placeholders = $RECORDS sidecar records"

# 2b. the custom runtime macro: one placeholder of kind custom, and no
# undefined macro (the binding defines \interactiveruntime).
mkdir -p "$SCRATCH/runtime/out"
cp -r "$DEVROOT/e2e/fixtures/interactive/." "$SCRATCH/runtime/"
(cd "$SPIKE" && bwrap --ro-bind / / --dev /dev --proc /proc --bind /var/tmp /var/tmp \
  --tmpfs /tmp --bind "$SCRATCH" "$SCRATCH" --tmpfs /usr/share/texlive \
  --setenv PATH "$SPIKE/shim:/usr/bin:/bin" \
  --setenv LATEXML_DUMP_DIR "$SPIKE/gen/resources/dumps" \
  --setenv MALEFICIUM_DUMP_DIR "$DEVROOT/src-tauri/resources/dumps" \
  "$ENGINE" convert "$SCRATCH/runtime/runtime.tex" --out "$SCRATCH/runtime/out/runtime.html" \
  --bundle "$BUNDLE_URL" --cache "$SCRATCH/bundle-cache" \
  >"$SCRATCH/runtime.log" 2>&1) || { cat "$SCRATCH/runtime.log"; fail "convert of runtime.tex failed"; }
grep -q 'Conversion complete: No obvious problems' "$SCRATCH/runtime.log" \
  || { cat "$SCRATCH/runtime.log"; fail "runtime.tex conversion reported problems"; }
[ "$(grep -o 'class="ltx_text m-widget m-widget-custom"' "$SCRATCH/runtime/out/runtime.html" | wc -l)" = 1 ] \
  || fail "runtime.tex does not convert to one m-widget-custom placeholder"
pass "\\interactiveruntime converts to one m-widget-custom placeholder"

# 3. the real join
cd "$DEVROOT/src-tauri"
MFW_E2E_HTML="$HTML" MFW_E2E_SIDECAR="$MFW" cargo test -q -p maleficium-core --lib \
  the_converted_playground_joins_its_compiled_sidecar -- --ignored --nocapture \
  | grep -v '^$' || fail "join failed"
pass "joined"
