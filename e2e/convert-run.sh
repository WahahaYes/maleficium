#!/bin/bash
# `maleficium-engine convert` with no TeX installed. Runs the built engine in a
# bubblewrap sandbox that hides the host TeX tree and its kpsewhich/pdflatex
# (with a writable tmpfs /tmp), converts a scratch copy of e2e/fixtures/simple/
# and a paper with an undefined macro, and asserts: an HTML document comes
# out, the structure counts hold, every figure carries its `data-graphic` path, an error in the paper still yields the
# article and is counted in the log, and nothing but --out (plus latexml's
# stylesheets beside it) is written, never beside the source.
# Needs: bwrap (Linux), the engine (sh scripts/build-engine.sh), network on a
# cold cache. ENGINE, CONVERT_CACHE override the engine and the bundle cache.
set -euo pipefail

DEVROOT="$(cd "$(dirname "$0")/.." && pwd)"
ENGINE="${ENGINE:-$DEVROOT/src-tauri/binaries/maleficium-engine-x86_64-unknown-linux-gnu}"
DUMPS="$DEVROOT/src-tauri/resources/dumps"
SCRATCH="$(mktemp -d /tmp/maleficium-convert-XXXXXX)"
trap 'rm -rf "$SCRATCH"' EXIT
CACHE="${CONVERT_CACHE:-$SCRATCH/cache}"

fail() { echo "FAIL: $1"; exit 1; }
pass() { echo "ok: $1"; }

command -v bwrap >/dev/null || fail "bwrap is required"
[ -x "$ENGINE" ] || fail "engine missing: $ENGINE (run sh scripts/build-engine.sh)"
ls "$DUMPS"/latex.*.dump.txt >/dev/null 2>&1 || fail "no dumps in $DUMPS"
BUNDLE="$(sh "$DEVROOT/scripts/dumps-key.sh" url)"

mkdir -p "$CACHE" "$SCRATCH/out"
cp -r "$DEVROOT/e2e/fixtures/simple" "$SCRATCH/simple"
cat > "$SCRATCH/simple/broken.tex" <<'TEX'
\documentclass{article}
\begin{document}
\section{Before}
Text $x^2$.
\section{After}
\undefinedmacro{here} and more text.
\end{document}
TEX

# Hides the host TeX; nothing but ours can answer kpsewhich.
sandbox() {
  bwrap --ro-bind / / --dev /dev --proc /proc --tmpfs /tmp \
    --bind "$SCRATCH" "$SCRATCH" --bind "$CACHE" "$CACHE" \
    --tmpfs /usr/share/texlive --tmpfs /usr/share/texmf \
    --ro-bind /dev/null /usr/bin/kpsewhich --ro-bind /dev/null /usr/bin/pdflatex \
    --ro-bind /dev/null /usr/bin/latex "$@"
}
convert() { # input out log
  sandbox "$ENGINE" convert "$1" --out "$2" --log "$3" --bundle "$BUNDLE" --cache "$CACHE" --dumps "$DUMPS"
}
count() { grep -o "$2" "$1" | wc -l | tr -d ' '; }

before="$(cd "$SCRATCH/simple" && find . | sort)"
convert "$SCRATCH/simple/main.tex" "$SCRATCH/out/simple.html" "$SCRATCH/out/simple.log" >/dev/null 2>&1 \
  || { cat "$SCRATCH/out/simple.log" 2>/dev/null; fail "simple.tex did not convert"; }
head -c 20 "$SCRATCH/out/simple.html" | grep -qi '<!doctype html' || fail "output is not an HTML document"
pass "simple converts to an HTML document with no host TeX"
h="$(count "$SCRATCH/out/simple.html" '<h[1-6][ >]')"
[ "$h" = 10 ] || fail "expected 10 headings, got $h"
pass "10 headings"
g="$(count "$SCRATCH/out/simple.html" 'data-graphic="figs/[a-z]*"')"
[ "$g" = 2 ] || fail "expected 2 data-graphic paths, got $g"
pass "each figure carries its data-graphic path"
[ "$(cd "$SCRATCH/simple" && find . | sort)" = "$before" ] || fail "convert wrote beside the source"
pass "nothing written beside the source"
[ -f "$SCRATCH/out/LaTeXML.css" ] || fail "stylesheets did not land next to --out"
pass "stylesheets land next to --out"
[ ! -e "$CACHE/scratch" ] || [ -z "$(ls -A "$CACHE/scratch")" ] || fail "scratch directory left behind"
pass "scratch directory removed"

convert "$SCRATCH/simple/broken.tex" "$SCRATCH/out/broken.html" "$SCRATCH/out/broken.log" >/dev/null 2>&1 \
  || fail "an undefined macro must not fail the conversion"
[ "$(count "$SCRATCH/out/broken.html" '<h2[ >]')" = 2 ] || fail "the article is incomplete"
grep -q 'undefinedmacro' "$SCRATCH/out/broken.log" || fail "the undefined macro is not in the log"
pass "an undefined macro still yields the article and is logged"

# An author's own package, beside the paper, loads raw: a plain one and an
# expl3 one render their macros; one that does not exist is a warning only.
mkdir -p "$SCRATCH/pkgs"
cat > "$SCRATCH/pkgs/mypkg.sty" <<'STY'
\ProvidesPackage{mypkg}
\newcommand\hello[1]{\textbf{Hello #1}}
STY
cat > "$SCRATCH/pkgs/explpkg.sty" <<'STY'
\ProvidesPackage{explpkg}
\RequirePackage{expl3}
\ExplSyntaxOn
\NewDocumentCommand\greet{m}{\textit{Greetings~#1}}
\ExplSyntaxOff
STY
cat > "$SCRATCH/pkgs/main.tex" <<'TEX'
\documentclass{article}
\usepackage{mypkg}
\usepackage{explpkg}
\usepackage{notapackage}
\begin{document}
A: \hello{world}. B: \greet{there}.
\end{document}
TEX
convert "$SCRATCH/pkgs/main.tex" "$SCRATCH/out/pkgs.html" "$SCRATCH/out/pkgs.log" >/dev/null 2>&1 \
  || fail "a paper with local packages did not convert"
grep -q 'Hello world' "$SCRATCH/out/pkgs.html" || fail "a plain local .sty was not loaded"
grep -q 'Greetings' "$SCRATCH/out/pkgs.html" || fail "an expl3 local .sty was not loaded"
[ "$(count "$SCRATCH/out/pkgs.html" 'ltx_ERROR')" = 0 ] || fail "local packages left undefined macros"
pass "local .sty packages (plain and expl3) are loaded raw"

if sandbox "$ENGINE" convert "$SCRATCH/simple/missing.tex" --out "$SCRATCH/out/x.html" --bundle "$BUNDLE" --cache "$CACHE" --dumps "$DUMPS" >/dev/null 2>&1; then
  fail "a missing input must fail"
fi
pass "a missing input exits non-zero"
echo "all convert checks passed"
