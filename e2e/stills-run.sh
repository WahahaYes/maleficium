#!/bin/sh
# stills-run.sh — mechanical still capture for the shell states.
#
# Launches the app under Xvfb with a contained HOME, opens each fixture
# hands-free (Ctrl+O; the `?project=` preset skips the dialog), drives
# compile/failure with Ctrl+R, captures PNGs to OS tmp with `import`.
# No assertions here: stills are filed artifacts for on-demand review;
# the WebDriver slice owns assertions. 3000pp is deferred (no cheap
# path: needs a generated 3000-page build; see SCOPE).
set -eu
unset CDPATH
ROOT=$(cd -- "$(dirname -- "$0")/.." && pwd -P)
OUT=${STILLS_OUT:-$(mktemp -d /tmp/maleficium-stills-XXXXXX)}
mkdir -p "$OUT"
: > "$OUT/dev.log"
REALHOME="$HOME"
FAKEHOME=$(mktemp -d /tmp/maleficium-stills-home-XXXXXX)
DISP=${STILLS_DISPLAY:-:99}
XVFB_PID=""
# Pin every X tool at the harness display via the environment (xdotool
# has no --display flag; it reads DISPLAY). Never touch the session.
export DISPLAY="$DISP"
XDO="xdotool"

cleanup() {
  # Group-kill our own tree (tauri CLI + vite + app + webkit). Never
  # pkill by name: the human's own dev:desktop shares the binary name.
  if [ -n "${APP_PID:-}" ] && kill -0 "$APP_PID" 2>/dev/null; then
    kill -TERM -- "-$APP_PID" 2>/dev/null || true
    sleep 2
    kill -KILL -- "-$APP_PID" 2>/dev/null || true
  fi
  if [ -n "$XVFB_PID" ] && kill -0 "$XVFB_PID" 2>/dev/null; then kill "$XVFB_PID" 2>/dev/null || true; fi
}
trap cleanup EXIT INT TERM

sweep_stale() {
  # Orphans from hard-killed runs: same binary name as a human's
  # dev:desktop, so match DISPLAY=:99 in their environ, nothing else.
  for p in $(pgrep -x tauri-app 2>/dev/null); do
    if tr '\0' '\n' <"/proc/$p/environ" 2>/dev/null | grep -qx 'DISPLAY=:99'; then
      log "sweeping stale harness app $p"
      kill -KILL "$p" 2>/dev/null || true
    fi
  done
}

log() { printf 'stills: %s\n' "$*"; }
die() { printf 'stills: FATAL %s\n' "$*" >&2; exit 1; }

need() { command -v "$1" >/dev/null 2>&1 || die "missing tool: $1"; }
need Xvfb; need xdotool; need import; need python3

if ! xdotool search --onlyvisible --name ".*" >/dev/null 2>&1; then
  log "starting Xvfb on $DISP"
  Xvfb "$DISP" -screen 0 1600x900x24 >/dev/null 2>&1 &
  XVFB_PID=$!
  sleep 2
fi

# Scratch fixtures (never the repo originals): simple compiles clean,
# bad fails fast with `\badcommand` on l.3.
FIX=$(mktemp -d /tmp/maleficium-stills-fix-XXXXXX)
cp -r "$ROOT/playground/simple/." "$FIX/simple/"
mkdir -p "$FIX/bad"
printf '\\documentclass{article}\n\\begin{document}\n\\badcommand\n\\end{document}\n' >"$FIX/bad/bad.tex"

sh "$ROOT/scripts/reclaim.sh" >/dev/null 2>&1 || true
sweep_stale

encode() { python3 -c 'import sys,urllib.parse; print(urllib.parse.quote(sys.argv[1], safe=""))' "$1"; }

start_app() {
  # $1 = project dir. Serves a merged devUrl carrying the preset, then
  # launches the app against it. Echoes nothing; sets APP_PID.
  sweep_stale
  DEVURL="http://localhost:1420/?project=$(encode "$1")"
  log "launching (preset $(basename "$1"))"
  # Toolchain homes stay real (rustup has no default under a fresh HOME);
  # everything the app writes stays contained via HOME. Compositing off:
  # no compositor runs under Xvfb and WebKit will not map otherwise.
  # shellcheck disable=SC2086
  HOME="$FAKEHOME" RUSTUP_HOME="$REALHOME/.rustup" CARGO_HOME="$REALHOME/.cargo" \
    DISPLAY="$DISP" GDK_BACKEND=x11 WEBKIT_DISABLE_COMPOSITING_MODE=1 \
    setsid "$ROOT/node_modules/.bin/tauri" dev \
    --config "{\"build\": {\"devUrl\": \"$DEVURL\"}}" \
    >>"$OUT/dev.log" 2>&1 &
  APP_PID=$!
}

wait_window() {
  # $1 = timeout seconds. Sets WIN. Bare Xvfb runs no window manager, so
  # the maximized shell never maps itself: find it unmapped, force-map,
  # force-size, then proceed. Activation is skipped (no WM to honor it).
  end=$(( $(date +%s) + $1 ))
  while [ "$(date +%s)" -lt "$end" ]; do
    # shellcheck disable=SC2086
    WIN=$($XDO search --name "^Maleficium$" 2>/dev/null | head -n 1 || true)
    if [ -n "${WIN:-}" ]; then
      # shellcheck disable=SC2086
      $XDO windowmap "$WIN" >/dev/null 2>&1 || true
      sleep 2
      # Refuse a black still: the map must actually take effect.
      vend=$(( $(date +%s) + 30 ))
      while [ "$(date +%s)" -lt "$vend" ]; do
        # shellcheck disable=SC2086
        if $XDO search --onlyvisible --name "^Maleficium$" 2>/dev/null | grep -qx "$WIN"; then break; fi
        sleep 2
      done
      log "window $WIN"
      return 0
    fi
    sleep 2
  done
  # shellcheck disable=SC2086
  log "visible on $DISP at timeout: $($XDO search --onlyvisible --name ".*" 2>/dev/null | tr '\n' ' ')"
  die "no app window appeared (see $OUT/dev.log)"
}

key() {
  # Focus first: unfocused synthetic keys never reach the menu handler
  # without a window manager. Failures tolerated throughout.
  # shellcheck disable=SC2086
  $XDO windowraise "$WIN" >/dev/null 2>&1 || true
  # shellcheck disable=SC2086
  $XDO windowfocus --sync "$WIN" >/dev/null 2>&1 || true
  # shellcheck disable=SC2086
  $XDO key --window "$WIN" "$@" >/dev/null 2>&1 || true
  sleep 1
}
click_editor() {
  # Menu accelerators (Ctrl+O) work at GTK level; webview-bound chords
  # (Ctrl+R) need the editor widget focused: click into it first.
  # shellcheck disable=SC2086
  $XDO mousemove --window "$WIN" 450 200 click 1 >/dev/null 2>&1 || true
  sleep 1
}
shot() {
  # Per-window capture: root captures tear while webkit repaints.
  import -display "$DISP" -window "$WIN" "$OUT/$1.png" 2>/dev/null && log "captured $1.png" || die "capture failed: $1"
}

stop_app() {
  # Belt and suspenders: group-kill our tree, then sweep anything on
  # :99 (a human dev:desktop never lives there, so this is precise).
  if [ -n "${APP_PID:-}" ] && kill -0 "$APP_PID" 2>/dev/null; then
    kill -TERM -- "-$APP_PID" 2>/dev/null || true
    sleep 3
    kill -KILL -- "-$APP_PID" 2>/dev/null || true
    sleep 2
  fi
  APP_PID=""
  sweep_stale
  sleep 2
  sh "$ROOT/scripts/reclaim.sh" >/dev/null 2>&1 || true
}

# State 1 — Default: open, no compile. Idle shell + quiet preview.
start_app "$FIX/simple"; wait_window 300
key ctrl+o; sleep 6
shot 01-default
stop_app

# State 2 — Compiling: pre-compile the fixture through the sidecar so
# OPEN warms into a live compile by itself (no keystroke race). Rapid
# stills through the warm run, then a settled done-still.
log "pre-warming engine cache via driver"
HOME="$FAKEHOME" RUSTUP_HOME="$REALHOME/.rustup" CARGO_HOME="$REALHOME/.cargo" \
  MCP_ROOT_OVERRIDE="$FIX/simple" DRIVER_LOG="$OUT/driver-warm.jsonl" \
  WARM_ONLY=1 POLL_ROUNDS=150 \
  bash "$ROOT/e2e/driver-run.sh" >>"$OUT/dev.log" 2>&1 || die "warm driver failed"
start_app "$FIX/simple"; wait_window 300
key ctrl+o; sleep 6
i=0
while [ "$i" -lt 8 ]; do
  i=$((i + 1))
  import -display "$DISP" -window "$WIN" "$OUT/02-compiling-$i.png" 2>/dev/null || true
  sleep 5
done
log "captured 02-compiling-{1..8}.png"
shot 02-compiling-done
stop_app

# State 3 — Failure: bad project, focus, Ctrl+R (bundles warm by now).
start_app "$FIX/bad"; wait_window 300
key ctrl+o; sleep 6
click_editor
key ctrl+r; sleep 20
shot 03-failure
stop_app

log "stills in $OUT:"
ls "$OUT"/*.png
