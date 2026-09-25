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
# $STILLS_HOME/.local/share/io.github.wahahayes.maleficium/maleficium-log/events.jsonl.
FAKEHOME=${STILLS_HOME:-$(mktemp -d /tmp/maleficium-stills-home-XXXXXX)}
DISP=${STILLS_DISPLAY:-:99}
# STILLS_PORT moves the harness vite off 1420 while a dev app holds it.
PORT=${STILLS_PORT:-1420}
export DEV_PORT="$PORT"
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
  for p in $(pgrep -x maleficium 2>/dev/null); do
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

# STILLS_STATES picks states to run (default all: "1 2 3 4 5 6 7").
want() {
  case " ${STILLS_STATES:-1 2 3 4 5 6 7} " in *" $1 "*) return 0 ;; esac
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
    DEVURL="http://localhost:$PORT/?project=$(encode "$1")"
    log "launching (preset $(basename "$1"))"
  else
    DEVURL="http://localhost:$PORT/"
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
    # The app owns a 10x10 helper window in the same class: take the
    # largest match, never the first.
    best=0; WIN=""
    # shellcheck disable=SC2086
    for cand in $($XDO search --name "^Maleficium$" 2>/dev/null || true); do
      # shellcheck disable=SC2086
      set -- $($XDO getwindowgeometry "$cand" 2>/dev/null | sed -n 's/^ *Geometry: \([0-9]*\)x\([0-9]*\)/\1 \2/p')
      if [ "$((${1:-0} * ${2:-0}))" -gt "$best" ]; then
        best=$((${1:-0} * ${2:-0})); WIN=$cand
      fi
    done
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
  APPLOG="$FAKEHOME/.local/share/io.github.wahahayes.maleficium/maleficium-log/events.jsonl"
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
  python3 - "$FAKEHOME/.local/share/io.github.wahahayes.maleficium/maleficium-log/events.jsonl" <<'EOF'
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
# The compiled pdf must load in pdf.js and paint a page: a preview stuck on
# 'loading...' (e.g. its worker never started) fails here, not in a still.
check_log "log.open file.save revision.record fs.external compile.auto preview.pdf-load preview.page-render"

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
check_log "log.open compile.finish offline.readiness preview.external-update preview.pdf-load preview.page-render"
# The warm run compiled from the cache alone: the badge must read Ready
# offline (02-compiling-done shows it) and the bus must say so.
python3 - "$FAKEHOME/.local/share/io.github.wahahayes.maleficium/maleficium-log/events.jsonl" <<'EOF' || die "no ready-offline readiness after the cached-only recompile"
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
APPLOG="$FAKEHOME/.local/share/io.github.wahahayes.maleficium/maleficium-log/events.jsonl"
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
# opens the welcome tour without building it; Help > Welcome then builds
# it, and a template double-clicked in the gallery lands in the home
# folder, opens with its main file, and builds.
FIRSTRUN="$FIX/first-run-data"
FIRSTLOG="$FIRSTRUN/io.github.wahahayes.maleficium/maleficium-log/events.jsonl"
mkdir -p "$FIRSTRUN"
rm -rf "$FAKEHOME/article"
palette() {
  # $1 = text typed into the command palette, then Return.
  key ctrl+shift+p; sleep 2
  # shellcheck disable=SC2086
  $XDO mousemove --window "$WIN" 400 91 click 1 >/dev/null 2>&1 || true
  sleep 1
  $XDO type --delay 40 "$1" >/dev/null 2>&1 || true
  sleep 1
  $XDO key Return >/dev/null 2>&1 || true
}
finishes() {
  # $1 = compile.finish count to wait for, $2 = timeout seconds.
  end=$(( $(date +%s) + $2 ))
  while [ "$(date +%s)" -lt "$end" ]; do
    n=$(grep -c '"action":"compile.finish"' "$FIRSTLOG" 2>/dev/null || true)
    [ "${n:-0}" -ge "$1" ] && return 0
    sleep 3
  done
  log "only ${n:-0} compile.finish events after $2s"
}
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
key Escape; sleep 2
palette "welcome"
finishes 1 240
sleep 3
shot 05-welcome-built
palette "from template"
sleep 3
# A double-click on a card creates it at the shown location (home).
# shellcheck disable=SC2086
$XDO mousemove --window "$WIN" 167 200 click --repeat 2 --delay 200 1 >/dev/null 2>&1 || true
finishes 2 300
sleep 3
shot 05-created
stop_app
python3 - "$FIRSTRUN/io.github.wahahayes.maleficium/maleficium-log/events.jsonl" <<'EOF' || die "first run did not open the welcome tour"
import json, sys
evs = [json.loads(l).get("event") or {} for l in open(sys.argv[1]).read().splitlines() if l.strip()]
acts = [e.get("action") for e in evs]
assert "template.welcome" in acts, "no template.welcome in %s" % sorted(set(acts))
roots = [e.get("root", "") for e in evs if e.get("action") == "project.open"]
assert any(r.endswith("maleficium-welcome") for r in roots), "welcome project not opened: %s" % roots
print("stills: first run opened the welcome tour: %s" % roots[-1])
EOF
FAKEHOME="$FAKEHOME" python3 - "$FIRSTLOG" <<'EOF' || die "welcome or template project did not open with a main file and build"
import json, os, sys
evs = [json.loads(l).get("event") or {} for l in open(sys.argv[1]).read().splitlines() if l.strip()]
acts = [e.get("action") for e in evs]
w = [i for i, a in enumerate(acts) if a == "template.welcome"]
assert len(w) >= 2, "Help > Welcome did not reopen the tour: %s" % sorted(set(acts))
first = acts[w[0]:w[1]]
assert "compile.warm-skipped" in first and "compile.finish" not in first, "first run built the tour: %s" % first
fin = [e for e in evs if e.get("action") == "compile.finish"]
assert fin and fin[0].get("ok") is True and fin[0].get("target", "").endswith("maleficium-welcome/welcome.tex"), "welcome build: %s" % fin[:1]
want = os.path.join(os.path.realpath(os.environ["FAKEHOME"]), "article")
made = [e for e in evs if e.get("action") == "template.create"]
assert made and made[-1].get("root") == want, "template root %s, want %s" % (made[-1:], want)
mains = [e.get("mainFile") for e in evs if e.get("action") == "main.resolved" and e.get("root") == want]
assert mains == [want + "/main.tex"], "main for the new project: %s" % mains
assert len(fin) >= 2 and fin[1].get("ok") is True and fin[1].get("target") == want + "/main.tex", "template build: %s" % fin[1:2]
assert os.path.isfile(want + "/main.tex")
print("stills: Help > Welcome built the tour; %s opened with main.tex and built" % want)
EOF

fi

if want 6; then
# State 6 — Zoom: step the preview to 50%, 150% and 300% with the zoom
# chords and capture each. Every settled zoom must be followed by a fresh
# render of the visible page (a stretched stale bitmap fails here).
APPLOG="$FAKEHOME/.local/share/io.github.wahahayes.maleficium/maleficium-log/events.jsonl"
last_zoom() {
  python3 - "$APPLOG" <<'EOF'
import json, sys
evs = [json.loads(l).get("event") or {} for l in open(sys.argv[1]).read().splitlines() if l.strip()]
z = [e["percent"] for e in evs if e.get("action") == "preview.zoom"]
print(z[-1] if z else -1)
EOF
}
zoom_to() {
  # $1 = target percent. One chord per step, bounded; the log says where it is.
  n=0
  while [ "$n" -lt 45 ]; do
    n=$((n + 1))
    cur=$(last_zoom)
    [ "$cur" = "$1" ] && break
    if [ "$cur" -lt "$1" ]; then k=ctrl+equal; else k=ctrl+minus; fi
    # shellcheck disable=SC2086
    $XDO key --window "$WIN" "$k" >/dev/null 2>&1 || true
    sleep 0.5
  done
  [ "$(last_zoom)" = "$1" ] || die "zoom did not reach $1% (at $(last_zoom)%)"
  sleep 4
}
scroll_preview() {
  # $1/$2 = wheel notches down/right over the preview, onto body text.
  # shellcheck disable=SC2086
  $XDO mousemove --window "$WIN" 320 400 >/dev/null 2>&1 || true
  # shellcheck disable=SC2086
  $XDO click --repeat "$1" --delay 80 5 >/dev/null 2>&1 || true
  # shellcheck disable=SC2086
  $XDO click --repeat "$2" --delay 80 7 >/dev/null 2>&1 || true
  sleep 2
}
start_app "$FIX/simple"; wait_window 300
# A wide preview: full-size window, Preview Only layout (through the palette).
# shellcheck disable=SC2086
$XDO windowsize "$WIN" 1600 900 >/dev/null 2>&1 || true
sleep 2
key ctrl+o; sleep 6
click_editor
key ctrl+s
end=$(( $(date +%s) + 180 ))
until grep -q '"preview.page-render"' "$APPLOG" 2>/dev/null; do
  [ "$(date +%s)" -lt "$end" ] || die "the preview never painted a page"
  sleep 2
done
key ctrl+shift+p; sleep 2
# shellcheck disable=SC2086
$XDO mousemove --window "$WIN" 800 91 click 1 >/dev/null 2>&1 || true
sleep 1
$XDO type --delay 40 "preview only" >/dev/null 2>&1 || true
sleep 1
$XDO key Return >/dev/null 2>&1 || true
sleep 3
zoom_to 50; shot 06-zoom-50
zoom_to 150; scroll_preview 4 4; shot 06-zoom-150
zoom_to 300; scroll_preview 10 10; shot 06-zoom-300
key ctrl+0; sleep 3
stop_app
check_log "log.open preview.zoom preview.page-render"
python3 - "$APPLOG" <<'EOF' || die "a settled zoom left the visible page unrendered"
import json, sys
evs = [json.loads(l) for l in open(sys.argv[1]).read().splitlines() if l.strip()]
evs = [(e["at"], e.get("event") or {}) for e in evs]
zooms = [(at, e["percent"]) for at, e in evs if e.get("action") == "preview.zoom"]
settled = [(at, p, (zooms[i + 1][0] if i + 1 < len(zooms) else float("inf")))
           for i, (at, p) in enumerate(zooms)
           if i + 1 == len(zooms) or zooms[i + 1][0] - at >= 2000]
assert settled, "no settled zoom in the log"
for at, p, until in settled:
    pages = [e["page"] for t, e in evs if e.get("action") == "preview.page-render" and at < t < until]
    assert 1 in pages, "zoom to %s%% at %s: no page 1 render before the next zoom (renders %s)" % (p, at, pages)
print("stills: every settled zoom re-rendered page 1: %s" % ",".join("%s%%" % p for _, p, _ in settled))
EOF

fi

if want 7; then
# State 7 — Render churn, counted in the app log. A portrait paper: fit page
# renders each page once and then holds still, one zoom step renders each
# page once, and scrolling at a fixed zoom renders nothing. A landscape
# beamer deck: its first open renders page 1 once, at the page's own aspect.
APPLOG="$FAKEHOME/.local/share/io.github.wahahayes.maleficium/maleficium-log/events.jsonl"
MARKS="$OUT/07-marks"
: >"$MARKS"
mark() { python3 -c 'import sys, time; print(sys.argv[1], int(time.time() * 1000))' "$1" >>"$MARKS"; }
palette() {
  key ctrl+shift+p; sleep 2
  # shellcheck disable=SC2086
  $XDO mousemove --window "$WIN" 800 91 click 1 >/dev/null 2>&1 || true
  sleep 1
  $XDO type --delay 40 "$1" >/dev/null 2>&1 || true
  sleep 1
  $XDO key Return >/dev/null 2>&1 || true
}
wheel() {
  # $1 = button (4 up, 5 down), $2 = notches, over the preview pane.
  # shellcheck disable=SC2086
  $XDO mousemove --window "$WIN" 1330 400 >/dev/null 2>&1 || true
  # shellcheck disable=SC2086
  $XDO click --repeat "$2" --delay 150 "$1" >/dev/null 2>&1 || true
}
open_compiled() {
  # $1 = project dir: open it at fit width, save to compile, wait for paint.
  start_app "$1"; wait_window 300
  # shellcheck disable=SC2086
  $XDO windowsize "$WIN" 1600 900 >/dev/null 2>&1 || true
  sleep 2
  key ctrl+o; sleep 6
  click_editor
  key ctrl+0
  mark "$2-open"
  key ctrl+s
  end=$(( $(date +%s) + 600 ))
  until grep -q '"preview.page-render"' "$APPLOG" 2>/dev/null; do
    [ "$(date +%s)" -lt "$end" ] || die "the preview never painted a page ($1)"
    sleep 2
  done
  sleep 5
}
open_compiled "$FIX/simple" portrait
mark portrait-fitpage
palette "fit page"
sleep 4
mark portrait-hold
sleep 6
mark portrait-step
click_editor
key ctrl+equal
sleep 4
mark portrait-scroll
wheel 5 15; wheel 4 15
sleep 3
mark portrait-done
shot 07-portrait
stop_app
cp "$APPLOG" "$OUT/07-portrait.jsonl"
mkdir -p "$FIX/beamer"
cp "$ROOT/src-tauri/templates/beamer/main.tex" "$FIX/beamer/main.tex"
open_compiled "$FIX/beamer" beamer
shot 07-beamer-open
mark beamer-scroll
wheel 5 30
sleep 3
mark beamer-done
stop_app
cp "$APPLOG" "$OUT/07-beamer.jsonl"
python3 - "$OUT" <<'EOF' || die "preview render counts are off (table above)"
import json, sys
out = sys.argv[1]
marks = dict((k, int(v)) for k, v in (l.split() for l in open(out + "/07-marks")))
def renders(log, a, b):
    evs = [json.loads(l) for l in open(out + "/" + log) if l.strip()]
    return [(e.get("event") or {}) for e in evs
            if (e.get("event") or {}).get("action") == "preview.page-render" and marks[a] <= e["at"] < marks[b]]
def pages(rs):
    c = {}
    for r in rs:
        c[r["page"]] = c.get(r["page"], 0) + 1
    return c
rows = [
    ("portrait fit page", pages(renders("07-portrait.jsonl", "portrait-fitpage", "portrait-hold"))),
    ("portrait fit page held 6s", pages(renders("07-portrait.jsonl", "portrait-hold", "portrait-step"))),
    ("portrait zoom step", pages(renders("07-portrait.jsonl", "portrait-step", "portrait-scroll"))),
    ("portrait scroll", pages(renders("07-portrait.jsonl", "portrait-scroll", "portrait-done"))),
    ("beamer first open", pages(renders("07-beamer.jsonl", "beamer-open", "beamer-scroll"))),
    ("beamer scroll", pages(renders("07-beamer.jsonl", "beamer-scroll", "beamer-done"))),
]
for name, c in rows:
    print("stills: renders per page, %-26s %s" % (name + ":", dict(sorted(c.items())) or "none"))
r = dict(rows)
bad = []
if r["portrait fit page"].get(1) != 1 or max(r["portrait fit page"].values(), default=0) > 1:
    bad.append("fit page must render each page once")
if r["portrait fit page held 6s"]:
    bad.append("fit page held still must render nothing")
if r["portrait zoom step"].get(1) != 1 or max(r["portrait zoom step"].values(), default=0) > 1:
    bad.append("a zoom step must render each page once")
if r["portrait scroll"]:
    bad.append("scrolling at a fixed zoom must render nothing already painted")
if r["beamer first open"].get(1) != 1:
    bad.append("the beamer deck must open with one page-1 render")
if max(r["beamer scroll"].values(), default=0) > 1:
    bad.append("scrolling the deck must render no page twice")
first = renders("07-beamer.jsonl", "beamer-open", "beamer-scroll")
p1 = [x for x in first if x.get("page") == 1]
if p1 and "width" in p1[0]:
    aspect = p1[0]["width"] / p1[0]["height"]
    print("stills: beamer page 1 backing %dx%d (aspect %.3f)" % (p1[0]["width"], p1[0]["height"], aspect))
    if not 1.30 < aspect < 1.37:
        bad.append("beamer page 1 must render at its 4:3 aspect")
assert not bad, "; ".join(bad)
print("stills: render counts ok")
EOF
fi

log "stills in $OUT:"
ls "$OUT"/*.png
