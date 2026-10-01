#!/bin/sh
# Convert one .tex with a relocated LaTeXML add-on tree and classify the
# outcome, because latexmlc's own summary is not trustworthy: after a fatal
# timeout it still prints "Post-processing complete: No obvious problems".
#
#   convert.sh TREE MAIN.tex OUTDIR [SECONDS] [-- extra latexmlc args]
#
# Guards:
#   1. latexmlc --timeout=SECONDS (LaTeXML's own; it then logs a Fatal)
#   2. timeout(1) at SECONDS+30, then SIGKILL 10 s later (a real wedge)
# Verdict on the last line: OK | WARNINGS | ERRORS | FATAL | HANG
# Exit: 0 for OK/WARNINGS, 2 ERRORS, 3 FATAL, 4 HANG.
# The add-on's stubs/ dir (if present) goes first on --path so bindings such
# as lipsum.sty.ltxml shadow packages known to hang (LATEXML_ADDON_STUBS=0
# turns that off, to reproduce the hang).
# POSIX sh.
set -u
unset CDPATH
TREE=${1:?usage: convert.sh TREE MAIN.tex OUTDIR [SECONDS] [-- args]}
MAIN=${2:?main .tex}
OUT=${3:?output dir}
SECS=${4:-300}
shift 3; [ $# -gt 0 ] && shift
[ "${1:-}" = "--" ] && shift
HERE=$(cd -- "$(dirname -- "$0")" && pwd -P)
mkdir -p "$OUT"
OUT=$(cd -- "$OUT" && pwd -P)
SRCDIR=$(cd -- "$(dirname -- "$MAIN")" && pwd -P)
# Prepend to "$@" rather than keep in a variable: the tree may live under a
# path with spaces.
[ "${LATEXML_ADDON_STUBS:-1}" = 0 ] || for d in "$TREE/stubs" "$HERE/stubs"; do
  [ -d "$d" ] && { set -- "--path=$d" "$@"; break; }
done
# LaTeXML calls kpsewhich when one is on PATH, so a user's own TeX Live or
# MiKTeX would silently change what gets loaded. Off by default; pass
# LATEXML_KPSEWHICH=kpsewhich to opt back in.
LATEXML_KPSEWHICH=${LATEXML_KPSEWHICH:-maleficium-no-kpsewhich}
export LATEXML_KPSEWHICH
LOG="$OUT/latexml.log"
ERR="$OUT/latexmlc.stderr"
start=$(date +%s)
(cd "$SRCDIR" && timeout -k 10 $((SECS + 30)) "$TREE/latexmlc" \
  --format=html5 --dest="$OUT/index.html" --log="$LOG" \
  --timeout="$SECS" "$@" "$(basename -- "$MAIN")") >"$OUT/latexmlc.stdout" 2>"$ERR"
rc=$?
end=$(date +%s)
status=$(sed -n 's/^Status:conversion:\([0-9]*\).*/\1/p' "$ERR" | tail -1)
fatals=$(cat "$LOG" "$ERR" 2>/dev/null | grep -c '^Fatal:')
claims_ok=$(grep -c 'No obvious problems' "$ERR")
echo "exit=$rc status=${status:-none} fatal_lines=$fatals says_no_obvious_problems=$claims_ok wall=$((end - start))s"
if [ "$rc" -eq 124 ] || [ "$rc" -eq 137 ]; then
  echo "verdict: HANG (killed by outer timeout)"; exit 4
fi
if [ "$fatals" -gt 0 ] || [ "${status:-3}" -ge 3 ] || [ "$rc" -ne 0 ]; then
  grep -h '^Fatal:' "$LOG" "$ERR" 2>/dev/null | sort -u | head -5
  echo "verdict: FATAL"; exit 3
fi
case "$status" in
  0) echo "verdict: OK" ;;
  1) echo "verdict: WARNINGS" ;;
  *) echo "verdict: ERRORS"; exit 2 ;;
esac
