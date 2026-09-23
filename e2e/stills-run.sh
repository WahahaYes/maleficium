#!/bin/sh
# stills-run.sh — mechanical still capture for the shell states.
#
# Launches the app under Xvfb with a contained HOME, opens each fixture
# hands-free (Ctrl+O; the `?project=` preset skips the dialog), drives
# compile/failure with Ctrl+R, captures PNGs to OS tmp with `import`.
# Stills themselves are filed artifacts for on-demand review (no pixel
# assertions); the one exception is the app event log, asserted after
# every state because no other harness launches the real app (the
# driver only drives the headless sidecar, which never starts the
# frontend log). State 4 compiles cold against an empty engine cache.
# 3000pp is deferred (no cheap path: needs a generated
# 3000-page build; see SCOPE).
set -eu
unset CDPATH
ROOT=$(cd -- "$(dirname -- "$0")/.." && pwd -P)
OUT=${STILLS_OUT:-$(mktemp -d /tmp/maleficium-stills-XXXXXX)}
mkdir -p "$OUT"
: > "$OUT/dev.log"
REALHOME="$HOME"
# Fixable for log inspection: STILLS_HOME=/tmp/x ./e2e/stills-run.sh, then read
# $STILLS_HOME/.local/share/com.ethan.tauri-app/maleficium-log/events.jsonl.
FAKEHOME=${STILLS_HOME:-$(mktemp -d /tmp/maleficium-stills-home-XXXXXX)}
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
log "contained home $FAKEHOME"

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

# STILLS_STATES picks states to run (default all: "1 2 3 4 5").
want() {
  case " ${STILLS_STATES:-1 2 3 4 5} " in *" $1 "*) return 0 ;; esac
  return 1
}

encode() { python3 -c 'import sys,urllib.parse; print(urllib.parse.quote(sys.argv[1], safe=""))' "$1"; }

start_app() {
  # $1 = project dir, or empty for a launch with no preset (restore or
  # first run). Serves a merged devUrl carrying the preset, then launches
  # the app against it. Echoes nothing; sets APP_PID. A set APP_CACHE /
  # APP_DATA gives this launch its own app cache / data home.
  sweep_stale
  if [ -n "$1" ]; then
    DEVURL="http://localhost:1420/?project=$(encode "$1")"
    log "launching (preset $(basename "$1"))"
  else
    DEVURL="http://localhost:1420/"
    log "launching (no preset)"
  fi
  set --
  if [ -n "${APP_CACHE:-}" ]; then set -- "XDG_CACHE_HOME=$APP_CACHE"; fi
  if [ -n "${APP_DATA:-}" ]; then set -- "$@" "XDG_DATA_HOME=$APP_DATA"; fi
  # Toolchain homes stay real (rustup has no default under a fresh HOME);
  # everything the app writes stays contained via HOME. Compositing off:
  # no compositor runs under Xvfb and WebKit will not map otherwise.
  # shellcheck disable=SC2086
  HOME="$FAKEHOME" RUSTUP_HOME="$REALHOME/.rustup" CARGO_HOME="$REALHOME/.cargo" \
    DISPLAY="$DISP" GDK_BACKEND=x11 WEBKIT_DISABLE_COMPOSITING_MODE=1 \
    env "$@" setsid "$ROOT/node_modules/.bin/tauri" dev \
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
  # Per-window capture: root captures tear while webkit repaints. Bounded:
  # a dead window must fail loudly, never hang the harness (one hung
  # `import` cost a full run with zero output).
  timeout 60 import -display "$DISP" -window "$WIN" "$OUT/$1.png" 2>/dev/null && log "captured $1.png" || die "capture failed: $1"
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

check_log() {
  # $1 = space-separated event.action names this run's log must hold.
  # The app truncates the log at launch, so exactly one log.open proves
  # the file is this run's and nothing else's.
  APPLOG="$FAKEHOME/.local/share/com.ethan.tauri-app/maleficium-log/events.jsonl"
  [ -f "$APPLOG" ] || die "no app event log at $APPLOG"
  # shellcheck disable=SC2086
  REQ="$1" python3 - "$APPLOG" <<'EOF' || die "app event log check failed (see above)"
import json, os, sys
path, req = sys.argv[1], os.environ["REQ"].split()
lines = [l for l in open(path).read().splitlines() if l.strip()]
assert lines, "log is empty"
events = []
for i, l in enumerate(lines):
    try:
        e = json.loads(l)
    except Exception:
        sys.exit("line %d is not JSON: %s" % (i + 1, l[:120]))
    assert isinstance(e.get("at"), (int, float)) and isinstance(e.get("message"), str), "line %d lacks at/message" % (i + 1)
    events.append(e)
actions = [e["event"].get("action") if isinstance(e.get("event"), dict) else e.get("dropped") for e in events]
bad = [i + 1 for i, e in enumerate(events) if e.get("actor") not in ("user", "agent", "system")]
assert not bad, "lines without an actor: %s" % bad[:10]
assert actions.count("log.open") == 1, "expected exactly one log.open (truncation proof), got %d" % actions.count("log.open")
missing = [a for a in req if a not in actions]
assert not missing, "missing actions: %s (have %s)" % (missing, sorted(set(actions)))
print("stills: app log ok: %d lines, actions %s" % (len(lines), ",".join(sorted(set(actions)))))
EOF
  log "checked $APPLOG ($1 present)"
}

saved_external() {
  # Echo: the file the last Ctrl+S saved, and how many fs.external events
  # the log holds for it. Own-write suppression is content-matched, so the
  # save alone must leave that count at 0; an edit behind the app's back
  # must raise it.
  python3 - "$FAKEHOME/.local/share/com.ethan.tauri-app/maleficium-log/events.jsonl" <<'EOF'
import json, sys
evs = [json.loads(l) for l in open(sys.argv[1]).read().splitlines() if l.strip()]
acts = [e.get("event") or {} for e in evs]
saves = [a["path"] for a in acts if a.get("action") == "file.save"]
assert saves, "no file.save in the log"
p = saves[-1]
print(p, sum(1 for a in acts if a.get("action") == "fs.external" and a.get("path") == p))
EOF
}

if want 1; then
# State 1 — Default: open, no compile. Idle shell + quiet preview.
# Ctrl+S forces a save so the log carries file.save + revision.record live.
start_app "$FIX/simple"; wait_window 300
key ctrl+o; sleep 6
click_editor
key ctrl+s; sleep 3
shot 01-default
# Revision-echo: the save must not read as an external change; a real
# external edit right after it must.
# shellcheck disable=SC2046
set -- $(saved_external)
[ "$2" = 0 ] || die "own save reported as external ($2 fs.external for $1)"
printf '%% external edit\n' >>"$1"; sleep 4
# shellcheck disable=SC2046
set -- $(saved_external)
[ "$2" -ge 1 ] || die "external edit after a save was swallowed ($1)"
log "echo check: save silent, external edit reported ($2) for $1"
stop_app
check_log "log.open file.save revision.record fs.external compile.auto"

fi

if want 2; then
# State 2 — Compiling: pre-compile the fixture through the sidecar so
# OPEN warms into a live compile by itself (no keystroke race). Rapid
# stills through the warm run, then a settled done-still.
log "pre-warming engine cache via driver"
HOME="$FAKEHOME" RUSTUP_HOME="$REALHOME/.rustup" CARGO_HOME="$REALHOME/.cargo" \
  DRIVER_CACHE="$FAKEHOME/.cache" \
  MCP_ROOT_OVERRIDE="$FIX/simple" DRIVER_LOG="$OUT/driver-warm.jsonl" \
  WARM_ONLY=1 POLL_ROUNDS=150 \
  bash "$ROOT/e2e/driver-run.sh" >>"$OUT/dev.log" 2>&1 || die "warm driver failed"
start_app "$FIX/simple"; wait_window 300
key ctrl+o; sleep 6
i=0
while [ "$i" -lt 8 ]; do
  i=$((i + 1))
  timeout 60 import -display "$DISP" -window "$WIN" "$OUT/02-compiling-$i.png" 2>/dev/null || true
  sleep 5
done
log "captured 02-compiling-{1..8}.png"
# An agent recompiles the open project over MCP: the preview must notice
# the rewritten pdf and reload by itself.
HOME="$FAKEHOME" RUSTUP_HOME="$REALHOME/.rustup" CARGO_HOME="$REALHOME/.cargo" \
  DRIVER_CACHE="$FAKEHOME/.cache" \
  MCP_ROOT_OVERRIDE="$FIX/simple" DRIVER_LOG="$OUT/driver-external.jsonl" \
  WARM_ONLY=1 POLL_ROUNDS=60 \
  bash "$ROOT/e2e/driver-run.sh" >>"$OUT/dev.log" 2>&1 || die "external compile driver failed"
sleep 6
shot 02-compiling-done
stop_app
check_log "log.open compile.finish offline.readiness preview.external-update"
# The warm run compiled from the cache alone: the badge must read Ready
# offline (02-compiling-done shows it) and the bus must say so.
python3 - "$FAKEHOME/.local/share/com.ethan.tauri-app/maleficium-log/events.jsonl" <<'EOF' || die "no ready-offline readiness after the cached-only recompile"
import json, sys
evs = [json.loads(l).get("event") or {} for l in open(sys.argv[1]).read().splitlines() if l.strip()]
states = [e.get("state") for e in evs if e.get("action") == "offline.readiness"]
assert states and states[-1] == "ready", "offline.readiness states: %s" % states
print("stills: offline readiness after the warm recompile: %s" % states[-1])
EOF

fi

if want 3; then
# State 3 — Failure: bad project, focus, Ctrl+R (bundles warm by now).
start_app "$FIX/bad"; wait_window 300
key ctrl+o; sleep 6
click_editor
key ctrl+r; sleep 20
shot 03-failure
stop_app
check_log "log.open compile.finish"

fi

if want 4; then
# State 4 — Cold compile: an empty engine cache of its own, Ctrl+R, stills
# while the first compile downloads until the log says it finished. The
# status bar must read the phase and a live download count.
APP_CACHE="$FIX/cold-cache" start_app "$FIX/simple"; wait_window 300
key ctrl+o; sleep 6
click_editor
key ctrl+r
APPLOG="$FAKEHOME/.local/share/com.ethan.tauri-app/maleficium-log/events.jsonl"
i=0
while [ "$i" -lt 60 ]; do
  i=$((i + 1))
  sleep 8
  timeout 60 import -display "$DISP" -window "$WIN" "$OUT/04-cold-$i.png" 2>/dev/null || true
  grep -q '"action":"compile.finish"' "$APPLOG" 2>/dev/null && break
done
log "captured 04-cold-{1..$i}.png"
shot 04-cold-done
stop_app
check_log "log.open compile.phase compile.fetch compile.finish"
python3 - "$APPLOG" <<'EOF' || die "cold compile did not report its phases and downloads"
import json, sys
evs = [json.loads(l).get("event") or {} for l in open(sys.argv[1]).read().splitlines() if l.strip()]
acts = [e.get("action") for e in evs]
end = acts.index("compile.finish")
phases = [e.get("phase") for e in evs[:end] if e.get("action") == "compile.phase"]
fetched = sum(1 for e in evs[:end] if e.get("action") == "compile.fetch" and e.get("outcome") == "fetched")
assert phases[:1] == ["first-compile"], "phases: %s" % phases[:5]
assert "tex" in phases and "xdvipdfmx" in phases, "phases: %s" % phases
assert fetched > 0, "no fetched files before compile.finish"
assert evs[end].get("ok") is True, "cold compile failed: %s" % evs[end]
print("stills: cold compile: %d fetches, phases %s" % (fetched, ",".join(dict.fromkeys(phases))))
EOF

fi

if want 5; then
# State 5 — First run: a data home with no recent projects and no preset
# opens the welcome tour; the command palette then opens the template
# gallery.
FIRSTRUN="$FIX/first-run-data"
mkdir -p "$FIRSTRUN"
APP_CACHE="$FAKEHOME/.cache" APP_DATA="$FIRSTRUN" start_app ""; wait_window 300
sleep 12
shot 05-welcome
key ctrl+shift+p; sleep 2
# Synthetic keys sent to a window never reach the palette's input: click it
# to focus it, then type to whatever has focus.
# shellcheck disable=SC2086
$XDO mousemove --window "$WIN" 400 91 click 1 >/dev/null 2>&1 || true
sleep 1
$XDO type --delay 40 "from template" >/dev/null 2>&1 || true
sleep 1
$XDO key Return >/dev/null 2>&1 || true
sleep 3
shot 05-gallery
stop_app
python3 - "$FIRSTRUN/com.ethan.tauri-app/maleficium-log/events.jsonl" <<'EOF' || die "first run did not open the welcome tour"
import json, sys
evs = [json.loads(l).get("event") or {} for l in open(sys.argv[1]).read().splitlines() if l.strip()]
acts = [e.get("action") for e in evs]
assert "template.welcome" in acts, "no template.welcome in %s" % sorted(set(acts))
roots = [e.get("root", "") for e in evs if e.get("action") == "project.open"]
assert any(r.endswith("maleficium-welcome") for r in roots), "welcome project not opened: %s" % roots
print("stills: first run opened the welcome tour: %s" % roots[-1])
EOF

fi

log "stills in $OUT:"
ls "$OUT"/*.png
