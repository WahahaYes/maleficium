#!/bin/sh
# Regenerate the README screenshots in docs/screenshots/ from the committed
# app. Builds HEAD in the harness worktree (e2e/worktree-run.sh), launches it
# under a private Xvfb at 1600x900, opens docs/screenshots/demo-project/,
# compiles it, and captures:
#   editor.png         file tree, editor, and the PDF at 100%
#   search.png         project search results (dark theme, editor-wide)
#   replace-light.png  a replace preview (GitHub Light theme, editor-wide)
# The PNGs replace the old ones only once all three are captured.
#
# Usage: sh scripts/generate-readme-screenshots.sh
# Env: SHOTS_DISPLAY (default :97), SHOTS_PORT (default 1431), SHOTS_HOME
#   (default /tmp/ada; its path shows in the status bar, and its engine
#   cache is kept between runs), SHOTS_TARGET_DIR (default
#   /var/tmp/maleficium-shots-target), SHOTS_DEBUG=1 keeps the step captures.
# The first run downloads the TeX support files (needs network).
# Clicks are positions in the 1600x900 window: a layout change can move them.
# POSIX sh. Needs: Xvfb, xdotool, ImageMagick (import), curl, cargo.
set -eu
unset CDPATH
ROOT=$(cd -- "$(dirname -- "$0")/.." && pwd -P)
SELF=scripts/generate-readme-screenshots.sh
DEMO=docs/screenshots/demo-project

if [ -z "${WORKTREE_ACTIVE:-}" ]; then
  # Carry this script and the demo project as they stand in this checkout;
  # everything else is HEAD.
  # Its own Cargo target: a shared one would bake this worktree's path into
  # binaries other harness runs use.
  exec env WORKTREE_CARRY="$SELF $DEMO" SHOTS_OUT="$ROOT/docs/screenshots" \
    HARNESS_TARGET_DIR="${SHOTS_TARGET_DIR:-/var/tmp/maleficium-shots-target}" \
    "$ROOT/e2e/worktree-run.sh" HEAD -- sh "$SELF"
fi

# shellcheck source=/dev/null
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"
for t in Xvfb xdotool import curl cargo; do
  command -v "$t" >/dev/null 2>&1 || { echo "shots: missing $t" >&2; exit 1; }
done

OUT=${SHOTS_OUT:?run through scripts/generate-readme-screenshots.sh}
DISP=${SHOTS_DISPLAY:-:97}
PORT=${SHOTS_PORT:-1431}
FAKEHOME=${SHOTS_HOME:-/tmp/ada}
WORK=$(mktemp -d /tmp/maleficium-shots-XXXXXX)
LOG="$FAKEHOME/.local/share/io.github.wahahayes.maleficium/maleficium-log/events.jsonl"
say() { echo "shots: $*" | tee -a "$WORK/run.log" >&2; }
die() { say "FATAL $*"; say "run log: $WORK/run.log"; exit 1; }

# The home is ours only if it carries our marker; never clear anyone else's.
if [ -e "$FAKEHOME" ] && [ ! -e "$FAKEHOME/.maleficium-shots" ]; then
  die "$FAKEHOME exists and is not a screenshot home; set SHOTS_HOME"
fi
mkdir -p "$FAKEHOME"
: >"$FAKEHOME/.maleficium-shots"
# Fresh app state every run; the engine cache under .cache survives.
rm -rf "$FAKEHOME/.local" "$FAKEHOME/demo"
cp -r "$ROOT/$DEMO" "$FAKEHOME/demo"

XVFB_PID="" VITE_PID="" APP_PID=""
cleanup() {
  for p in "$APP_PID" "$VITE_PID"; do
    [ -n "$p" ] && kill -s TERM -- "-$p" 2>/dev/null || true
  done
  sleep 1
  for p in "$APP_PID" "$VITE_PID"; do
    [ -n "$p" ] && kill -s KILL -- "-$p" 2>/dev/null || true
  done
  [ -n "$XVFB_PID" ] && kill "$XVFB_PID" 2>/dev/null || true
}
trap cleanup EXIT INT TERM

[ ! -e "/tmp/.X11-unix/X${DISP#:}" ] || die "display $DISP is taken; set SHOTS_DISPLAY"
Xvfb "$DISP" -screen 0 1600x900x24 >/dev/null 2>&1 &
XVFB_PID=$!
sleep 2
export DISPLAY="$DISP"

# The dev build loads its frontend from this run's vite; the preset opens the
# demo project on Ctrl+O without a dialog (vite.config.ts).
export DEV_PORT="$PORT"
export TAURI_CONFIG="{\"build\":{\"devUrl\":\"http://localhost:$PORT/\"}}"
say "building the app"
(cd "$ROOT/src-tauri" && cargo build --bin maleficium) >>"$WORK/run.log" 2>&1 || die "build failed"
APPBIN="${CARGO_TARGET_DIR:-$ROOT/src-tauri/target}/debug/maleficium"
printf '%s' "$FAKEHOME/demo" >"$WORK/preset"
(cd "$ROOT" && STILLS_PRESET_FILE="$WORK/preset" exec setsid "$ROOT/node_modules/.bin/vite") \
  >>"$WORK/run.log" 2>&1 &
VITE_PID=$!
n=0
until curl -s -o /dev/null "http://localhost:$PORT/"; do
  n=$((n + 1)); [ "$n" -lt 120 ] || die "vite never answered on :$PORT"; sleep 0.5
done

HOME_REAL=$HOME
HOME="$FAKEHOME" RUSTUP_HOME="$HOME_REAL/.rustup" CARGO_HOME="$HOME_REAL/.cargo" \
  GDK_BACKEND=x11 WEBKIT_DISABLE_COMPOSITING_MODE=1 \
  env -C "$ROOT/src-tauri" setsid "$APPBIN" >>"$WORK/run.log" 2>&1 &
APP_PID=$!

# Bare Xvfb has no window manager: find the main window (the largest of the
# app's), map it, and pin it to the full screen.
WIN="" n=0
while [ -z "$WIN" ]; do
  n=$((n + 1)); [ "$n" -lt 60 ] || die "no app window"
  best=0
  for c in $(xdotool search --name "^Maleficium$" 2>/dev/null || true); do
    # shellcheck disable=SC2046
    set -- $(xdotool getwindowgeometry "$c" 2>/dev/null | sed -n 's/^ *Geometry: \([0-9]*\)x\([0-9]*\)/\1 \2/p')
    if [ "$((${1:-0} * ${2:-0}))" -gt "$best" ]; then best=$((${1:-0} * ${2:-0})); WIN=$c; fi
  done
  [ -n "$WIN" ] || sleep 1
done
xdotool windowmap "$WIN"
sleep 1
xdotool windowmove "$WIN" 0 0 windowsize "$WIN" 1600 900
sleep 6

focus() { xdotool windowraise "$WIN"; xdotool windowfocus --sync "$WIN" 2>/dev/null || true; }
key() { focus; xdotool key --window "$WIN" "$@"; sleep 1; }
typ() { focus; xdotool type --delay 40 "$1"; sleep 1; }
click() { focus; xdotool mousemove --window "$WIN" "$1" "$2" click 1; sleep 1; }
drag() {
  focus
  xdotool mousemove --window "$WIN" "$1" "$2" mousedown 1
  sleep 0.5
  xdotool mousemove --window "$WIN" $((($1 + $3) / 2)) "$2"
  sleep 0.3
  xdotool mousemove --window "$WIN" "$3" "$2"
  sleep 0.5
  xdotool mouseup 1
  sleep 1
}
shot() {
  timeout 60 import -display "$DISP" -window "$WIN" "$WORK/$1.png" || die "capture failed: $1"
  say "captured $1"
}
debug() { [ -z "${SHOTS_DEBUG:-}" ] || shot "debug-$1"; }
wait_for() {
  # $1 = pattern in the app's event log, $2 = timeout seconds.
  n=0
  until grep -q "$1" "$LOG" 2>/dev/null; do
    n=$((n + 1)); [ "$n" -lt "$2" ] || die "no $1 within $2 s"; sleep 1
  done
}

# Open the demo project and compile it.
n=0
until grep -q '"action":"file.open"' "$LOG" 2>/dev/null; do
  n=$((n + 1)); [ "$n" -lt 6 ] || die "the demo project never opened"
  key ctrl+o
  sleep 4
done
sleep 3
click 700 300
key ctrl+r
wait_for '"action":"compile.finish"' 1200
grep -q '"action":"compile.finish","ok":true' "$LOG" || die "the demo project did not compile"
sleep 6
debug compiled

# editor.png: log hidden, preview widened, PDF at 100%.
click 1559 729
sleep 2
drag 1063 450 720
click 130 600
debug widened
click 1510 84
sleep 1
debug zoom-menu
click 1494 261
sleep 4
xdotool mousemove --window "$WIN" 130 600
sleep 1
shot editor

# The search shots favor the editor: splitter right, preview fit to width.
drag 777 450 1150
click 130 600
key ctrl+0
sleep 3
debug editor-wide

# search.png: project search results.
click 500 300
key ctrl+shift+f
typ "curse"
sleep 3
shot search

# replace-light.png: GitHub Light, then a replace preview.
click 143 15
click 174 361
sleep 1
click 800 257
typ "GitHub Light"
sleep 1
key Down
key Return
sleep 1
key Escape
key Escape
sleep 1
click 171 54
click 80 114
typ "hex"
click 211 116
sleep 3
shot replace-light

mkdir -p "$OUT"
for f in editor search replace-light; do cp "$WORK/$f.png" "$OUT/$f.png"; done
say "wrote editor.png, search.png, replace-light.png to $OUT"
if [ -n "${SHOTS_DEBUG:-}" ]; then say "step captures kept in $WORK"; else rm -rf "$WORK"; fi
