#!/bin/bash
# Corpus conversion gate: `maleficium-engine convert` over five papers and a
# per-paper structure count compared against pinned baselines. Any drift is a
# finding, never a silent re-baseline: update EXPECTED only by an explicit
# decision, recorded beside the change.
#
# Papers: bake-off (refs/TVCG_Paper_Ref, copied whole so ../figures and
# ../_my_packages resolve inside the scratch tree), eye-gaze and origin
# (vendored real papers), playground (every widget macro), simple.
#
# Counters over the converted HTML document:
#   headings  `<h[1-6][ >]` occurrences
#   cites     `ltx_cite` substring occurrences (each <cite> carries the class
#             twice: `ltx_cite` and `ltx_citemacro_cite`)
#   math      `<math` occurrences (presentation MathML)
#   widgets   `m-widget-<type>` typed placeholder spans (convert marks every
#             widget macro; the compile path may merge or drop some)
#   figures   `<figure` occurrences (bake-off only: 13 figures + 3 tables)
#   errors    `ltx_ERROR` occurrences (an error still yields the article)
# Needs: the engine (sh scripts/build-engine.sh), network on a cold bundle
# cache. ENGINE, CONVERT_CACHE override the engine and the bundle cache.
# Unlike convert-run.sh there is no bwrap sandbox: the native engine never
# shells out to a host TeX.
set -euo pipefail

DEVROOT="$(cd "$(dirname "$0")/.." && pwd)"
ENGINE="${ENGINE:-$DEVROOT/src-tauri/binaries/maleficium-engine-x86_64-unknown-linux-gnu}"
DUMPS="$DEVROOT/src-tauri/resources/dumps"
REFS="${CORPUS_REFS:-/home/ethan/repos/latex-project/refs}"
SCRATCH="$(mktemp -d /tmp/maleficium-corpus-XXXXXX)"
trap 'rm -rf "$SCRATCH"' EXIT
CACHE="${CONVERT_CACHE:-$SCRATCH/cache}"
OUT="$SCRATCH/out"
mkdir -p "$CACHE" "$OUT"

fail() { echo "FAIL: $1"; exit 1; }
pass() { echo "ok: $1"; }

[ -x "$ENGINE" ] || fail "engine missing: $ENGINE (run sh scripts/build-engine.sh)"
ls "$DUMPS"/latex.*.dump.txt >/dev/null 2>&1 || fail "no dumps in $DUMPS"
BUNDLE="$(sh "$DEVROOT/scripts/dumps-key.sh" url)"

convert() { # input out log
  "$ENGINE" convert "$1" --out "$2" --log "$3" --bundle "$BUNDLE" --cache "$CACHE" --dumps "$DUMPS" >/dev/null 2>&1 \
    || { tail -5 "$3" 2>/dev/null; fail "$1 did not convert"; }
  head -c 20 "$2" | grep -qi '<!doctype html' || fail "$1 output is not an HTML document"
}

count() { grep -o "$2" "$1" | wc -l | tr -d ' '; }

# name input-rel(expected scratch layout) headings cites math widgets errors [figures]
check() { # name html h c m w e [f]
  local name="$1" html="$2" h="$3" c="$4" m="$5" w="$6" e="$7" f="${8:-}" miss=0
  local ah ac am aw af ae
  ah="$(count "$html" '<h[1-6][ >]')"; ac="$(count "$html" 'ltx_cite')"
  am="$(count "$html" '<math')"; aw="$(count "$html" 'm-widget-[a-z]*')"
  af="$(count "$html" '<figure')"; ae="$(count "$html" 'ltx_ERROR')"
  [ "$ah" = "$h" ] || { echo "FAIL: $name headings: expected $h, got $ah"; miss=1; }
  [ "$ac" = "$c" ] || { echo "FAIL: $name cites: expected $c, got $ac"; miss=1; }
  [ "$am" = "$m" ] || { echo "FAIL: $name math: expected $m, got $am"; miss=1; }
  [ "$aw" = "$w" ] || { echo "FAIL: $name widgets: expected $w, got $aw"; miss=1; }
  [ "$ae" = "$e" ] || { echo "FAIL: $name errors: expected $e, got $ae"; miss=1; }
  if [ -n "$f" ]; then
    [ "$af" = "$f" ] || { echo "FAIL: $name figures: expected $f, got $af"; miss=1; }
  fi
  if [ "$miss" = 0 ]; then
    pass "$name headings=$ah cites=$ac math=$am widgets=$aw figures=$af errors=$ae"
  else
    echo "info: $name actual headings=$ah cites=$ac math=$am widgets=$aw figures=$af errors=$ae"
    return 1
  fi
}

FAILED=0

# Bake-off: the whole paper tree comes along, so the ../figures references
# and ../_my_packages stay inside the scratch copy (a wider root the caller
# passes deliberately; the figure paths keep their ../figures/ form and the
# export's figure-embed step is where escaping paths warn).
mkdir -p "$SCRATCH/bakeoff"
cp -r "$REFS/TVCG_Paper_Ref/paper" "$SCRATCH/bakeoff/"
cp -r "$REFS/TVCG_Paper_Ref/figures" "$SCRATCH/bakeoff/"
cp "$REFS/TVCG_Paper_Ref/bibliography.bib" "$REFS/TVCG_Paper_Ref/_my_packages.tex" "$SCRATCH/bakeoff/"
convert "$SCRATCH/bakeoff/paper/main.tex" "$OUT/bakeoff.html" "$OUT/bakeoff.log"
check "bake-off" "$OUT/bakeoff.html" 43 126 216 0 0 16 || FAILED=1

cp -r "$DEVROOT/e2e/fixtures/vendored/eye-gaze-attention" "$SCRATCH/eyegaze"
convert "$SCRATCH/eyegaze/main.tex" "$OUT/eyegaze.html" "$OUT/eyegaze.log"
check "eye-gaze" "$OUT/eyegaze.html" 26 104 26 0 0 || FAILED=1

cp -r "$DEVROOT/e2e/fixtures/vendored/on-the-origin-of-objects" "$SCRATCH/origin"
convert "$SCRATCH/origin/paper.tex" "$OUT/origin.html" "$OUT/origin.log"
check "origin" "$OUT/origin.html" 27 34 26 0 11 || FAILED=1

cp -r "$DEVROOT/e2e/fixtures/playground" "$SCRATCH/playground"
convert "$SCRATCH/playground/main.tex" "$OUT/playground.html" "$OUT/playground.log"
check "playground" "$OUT/playground.html" 21 12 19 9 0 || FAILED=1

cp -r "$DEVROOT/e2e/fixtures/simple" "$SCRATCH/simple"
convert "$SCRATCH/simple/main.tex" "$OUT/simple.html" "$OUT/simple.log"
check "simple" "$OUT/simple.html" 10 8 0 0 0 || FAILED=1

[ "$FAILED" = 0 ] || fail "corpus drift: see FAIL lines above (a finding, not a re-baseline)"
echo "all corpus checks passed"
