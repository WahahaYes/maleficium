#!/bin/bash
# Live widgets end to end, in three parts; every check is a gate.
#  1. The shared widget-host conformance suite against the Linux helper,
#     with and without a network namespace, and the X11 embedding test,
#     under a private Xvfb.
#  2. The playground over the stdio sidecar: it compiles, and the automation
#     surface offers no way to approve widgets.
#  3. The real app under Xvfb on the playground: a fresh project runs no
#     widget; the run approval starts them, each in its own helper, with
#     network still denied; native views sit exactly on their slots and the
#     slots on their PDF rects, at two zooms; the network approval restarts
#     only the widget that declares an origin; approvals survive a restart;
#     revoking ends every helper without one.
# Env: WIDGETS_DISPLAY (:98), WIDGETS_PORT (vite, 1420), DRIVER_CACHE (the
# warm engine cache home, ~/.cache), POLL_ROUNDS, WIDGETS_PARTS ("1 2 3").
set -euo pipefail

DEVROOT="$(cd "$(dirname "$0")/.." && pwd)"
TARGET="${CARGO_TARGET_DIR:-$DEVROOT/src-tauri/target}"
DISP="${WIDGETS_DISPLAY:-:98}"
PORT="${WIDGETS_PORT:-1420}"
PARTS="${WIDGETS_PARTS:-1 2 3}"
SCRATCH="$(mktemp -d /tmp/maleficium-widgets-XXXXXX)"
XVFB=""
cleanup() {
  status=$?
  [ -n "$XVFB" ] && kill "$XVFB" 2>/dev/null && wait "$XVFB" 2>/dev/null
  if [ "$status" -eq 0 ]; then rm -rf "$SCRATCH"; else echo "kept for inspection: $SCRATCH"; fi
}
trap cleanup EXIT

fail() { echo "FAIL: $1"; exit 1; }
pass() { echo "ok: $1"; }
want() { case " $PARTS " in *" $1 "*) return 0 ;; esac; return 1; }

for tool in Xvfb xdotool xwininfo import curl; do
  command -v "$tool" >/dev/null || fail "missing tool: $tool"
done
[ -e "/tmp/.X${DISP#:}-lock" ] && fail "an X server already holds $DISP"

if want 1; then
  # the widget layer stays Tauri-free so it can deploy to the web
  for crate in maleficium-widget-host maleficium-widget-helper; do
    if cargo tree --manifest-path "$DEVROOT/src-tauri/Cargo.toml" -p "$crate" -e normal,build --prefix none \
        | grep -E '^(tauri|tauri-[a-z-]+|wry|tao) '; then
      fail "$crate depends on a tauri crate"
    fi
  done
  pass "widget crates pull in no tauri crates"
  cargo test -q --manifest-path "$DEVROOT/src-tauri/Cargo.toml" -p maleficium-widget-helper --tests --no-run \
    || fail "cannot build the widget helper tests"
  Xvfb "$DISP" -screen 0 1400x1000x24 -nolisten tcp >/dev/null 2>&1 &
  XVFB=$!
  for _ in $(seq 100); do DISPLAY="$DISP" xdotool getdisplaygeometry >/dev/null 2>&1 && break; sleep 0.1; done
  run_suite() {
    env -u WAYLAND_DISPLAY DISPLAY="$DISP" CONFORMANCE_NETNS="$1" timeout 600 \
      cargo test -q --manifest-path "$DEVROOT/src-tauri/Cargo.toml" -p maleficium-widget-helper \
      --test "$2" -- --ignored --nocapture 2>&1 | grep -E "^(conformance |capabilities|watchdog:|live-cap:|test |thread)" || true
  }
  out="$(run_suite 1 conformance)"
  echo "$out"
  echo "$out" | grep -q '"egress":"network-namespace"' || echo "note: no unprivileged user namespaces here; the namespace run used the dead-proxy fallback"
  echo "$out" | grep -q "test result: ok. 1 passed" \
    || fail "conformance suite (network namespace where available)"
  pass "conformance suite, network namespace where available"
  out="$(run_suite 0 conformance)"
  echo "$out"
  echo "$out" | grep -q '"egress":"dead-proxy"' || fail "the fallback run did not use the dead proxy"
  echo "$out" | grep -q "test result: ok. 1 passed" \
    || fail "conformance suite (dead-proxy fallback)"
  pass "conformance suite, dead-proxy fallback"
  out="$(run_suite 1 x11)"
  echo "$out"
  echo "$out" | grep -q "test result: ok. 1 passed" \
    || fail "X11 embedding: slot, clip, popup hide and input routing"
  pass "X11 embedding: slot, clip, popup hide and input routing"
  kill "$XVFB" 2>/dev/null; wait "$XVFB" 2>/dev/null || true
  XVFB=""
fi

want 2 || want 3 || { echo "WIDGET PROOFS COMPLETE (parts: $PARTS)"; exit 0; }

# The contained home the sidecar and the app share; the engine cache is the
# warm one, linked in, so compiles stay offline.
IDENT="io.github.wahahayes.maleficium"
FAKEHOME="$SCRATCH/home"
REALCACHE="${DRIVER_CACHE:-$HOME/.cache}/$IDENT/maleficium-tectonic"
[ -d "$REALCACHE" ] || fail "no warm engine cache at $REALCACHE: run e2e/playground-run.sh once"
mkdir -p "$FAKEHOME/.cache/$IDENT"
ln -s "$REALCACHE" "$FAKEHOME/.cache/$IDENT/maleficium-tectonic"
cp -r "$DEVROOT/e2e/fixtures/playground" "$SCRATCH/proj"
cp "$DEVROOT/src-tauri/interactive/maleficium-interactive.sty" "$SCRATCH/proj/"

export DEVROOT SCRATCH FAKEHOME TARGET DISP PORT PARTS IDENT
export POLL_ROUNDS="${POLL_ROUNDS:-60}"
python3 - <<'PYEOF'
import atexit, json, os, re, shutil, subprocess, sys, time
ROOT = os.environ["DEVROOT"]
sys.path.insert(0, os.path.join(ROOT, "e2e"))
import harness
from harness import now_ms, xdo
from mcp_client import McpClient

SCRATCH, FAKEHOME, TARGET = os.environ["SCRATCH"], os.environ["FAKEHOME"], os.environ["TARGET"]
DISP, PORT, IDENT = os.environ["DISP"], int(os.environ["PORT"]), os.environ["IDENT"]
PARTS = os.environ["PARTS"].split()
PROJ = os.path.join(SCRATCH, "proj")
OUT = os.path.join(SCRATCH, "out")
os.makedirs(OUT)
fails = []

def check(name, cond, detail=""):
    print(("ok: " if cond else "FAIL: ") + name + (f" ({detail})" if detail and not cond else ""), flush=True)
    if not cond:
        fails.append(name)
    return cond

def finish():
    print("")
    if fails:
        print("FAILED: " + ", ".join(fails))
        print("app log and screenshots: " + OUT)
        sys.exit(1)

# The sidecar and the app see the contained home only.
env = {k: v for k, v in os.environ.items() if not k.startswith("XDG_")}
env["HOME"] = FAKEHOME
mcp_bin = os.path.join(TARGET, "debug", "maleficium-mcp")
r = subprocess.run(["cargo", "build", "-q", "--manifest-path", os.path.join(ROOT, "src-tauri", "Cargo.toml"),
                    "--bin", "maleficium-mcp"])
if r.returncode != 0:
    sys.exit("cannot build maleficium-mcp")

# ---- part 2: the sidecar ---------------------------------------------------
mcp = McpClient([mcp_bin], "widgets", env=env)
def call(name, args):
    ok, r = mcp.tool(name, args)
    return {"ok": True, **r} if ok else {"ok": False, "error": r}
g = call("grant", {"root_id": "pg", "root": PROJ})
check("grant the playground", g["ok"], str(g))
r = call("compile_run", {"root_id": "pg", "rel": "main.tex"})
rec = {"status": "not-started", **r}
for _ in range(int(os.environ["POLL_ROUNDS"])):
    time.sleep(2)
    rec = call("compile_poll", {"job_id": r.get("job_id", ""), "tail_lines": 5})
    if rec.get("status") != "running":
        break
check("the playground compiles offline", rec.get("status") == "success", str(rec)[:300])
w = call("widgets", {"root_id": "pg", "main_rel": "main.tex"})
WIDGETS = {x["id"]: x for x in (w.get("widgets") or [])}
check("seven widgets with rects", len(WIDGETS) == 7, str(list(WIDGETS)))
RUNNABLE = {"tab-results", "fig-chart", "fig-html", "chart-bare", "chart-inline", "fig-model", "fig-video"}
tools = [t["name"] for t in mcp.request("tools/list")["result"]["tools"]]
schema = json.dumps(mcp.request("tools/list")["result"])
check("no automation tool approves widgets or their network",
      not [t for t in tools if re.search(r"approv|widget_(run|net)|allow", t)]
      and "approval" not in schema.lower(), str(tools))
mcp.p.kill()
if "3" not in PARTS:
    finish()
    sys.exit(0)

# ---- part 3: the app -------------------------------------------------------
procs = harness.Procs()
atexit.register(procs.stop_all)
PRESET = os.path.join(OUT, ".preset")
open(PRESET, "w").close()
harness.start_xvfb(procs, DISP, "1600x1000")
os.environ["TAURI_CONFIG"] = json.dumps({"build": {"devUrl": "http://localhost:%d/" % PORT}}, separators=(",", ":"))
harness.build_app(OUT, ROOT, TARGET, PORT)
harness.start_vite(procs, OUT, ROOT, PRESET, PORT, os.path.join(ROOT, "node_modules", ".vite"))
tl = harness.Timeline(os.path.join(OUT, "timeline.jsonl"))

class App(harness.App):
    def launch(self, project):
        with open(self.preset_file, "w") as f:
            f.write(project)
        e = {k: v for k, v in os.environ.items() if not k.startswith("XDG_")}
        e.update(HOME=self.home, DISPLAY=self.display, GDK_BACKEND="x11", WEBKIT_DISABLE_COMPOSITING_MODE="1")
        e.pop("WAYLAND_DISPLAY", None)
        e.pop("TAURI_CONFIG", None)
        self.launched = self.tl.mark("app.launch", project=project)
        self.p = self.procs.start([self.appbin], cwd=os.path.join(self.root, "src-tauri"), env=e,
                                  stdout=open(os.path.join(self.out, "app.log"), "a"), stderr=subprocess.STDOUT)

app = App(procs, OUT, FAKEHOME, PRESET, "", tl, os.path.join(TARGET, "debug", "maleficium"), ROOT, DISP,
          "1600x1000", IDENT)
log = app.log

def helpers():
    out = subprocess.run(["pgrep", "-f", "--", "--widget-helper --fd"], capture_output=True, text=True).stdout
    return [int(p) for p in out.split()]

def events(action, since):
    return [e["event"] for e in log.events() if e.get("at", 0) > since and e.get("event", {}).get("action") == action]

def palette(text):
    app.key("ctrl+shift+p")
    time.sleep(2)
    # the palette input only takes keystrokes once clicked (as stills-run.py does)
    xdo(DISP, "mousemove", "--window", app.win, "800", "91", "click", "1")
    time.sleep(1)
    xdo(DISP, "type", "--delay", "40", text)
    time.sleep(0.8)
    app.shot("palette-" + re.sub(r"\W+", "-", text.lower()))
    xdo(DISP, "key", "Return")
    time.sleep(0.8)

def start(first):
    app.launch(PROJ)
    app.place()
    app.open_project()
    m = now_ms()
    app.key("ctrl+r")
    check("compile in the app", bool(log.wait("compile.finish", m, 120)))
    return log.wait("widgets.layout", m, 30)

def layout_after(m, timeout=10, pred=lambda ev: True):
    end = time.time() + timeout
    while time.time() < end:
        evs = [ev for ev in events("widgets.layout", m) if pred(ev)]
        if evs:
            return evs[-1]
        time.sleep(0.3)
    return None

def goto_page(n):
    xdo(DISP, "mousemove", "--window", app.win, "1232", "84", "click", "--repeat", "3", "1")
    time.sleep(0.3)
    xdo(DISP, "type", "--delay", "40", str(n))
    xdo(DISP, "key", "Return")

def scroll_to(ids, m0):
    """Reach a page where one of `ids` has a visible slot: wheel first (the
    wheel reaches the paper because inactive widgets let it through), then
    page by page through the page box."""
    def seen():
        hit = [ev for ev in events("widgets.layout", m0) if any(s["id"] in ids for s in ev["slots"])]
        return hit[-1] if hit else None
    for _ in range(12):
        if seen():
            return seen()
        xdo(DISP, "mousemove", "--window", app.win, "1110", "500", "click", "5")
        time.sleep(1.0)
    for pg in range(1, 7):
        goto_page(pg)
        time.sleep(2.5)
        if seen():
            return seen()
    return None

def children():
    """The X child windows of the app window: the live widgets' views."""
    out = subprocess.run(["xwininfo", "-children", "-id", app.win], capture_output=True, text=True,
                         env=dict(os.environ, DISPLAY=DISP)).stdout
    kids = []
    for m in re.finditer(r"0x[0-9a-f]+ .*?: \(.*?\)\s+(\d+)x(\d+)\+(-?\d+)\+(-?\d+)", out):
        w, h, x, y = (int(v) for v in m.groups())
        kids.append((x, y, w, h))
    return kids

def aligned(lay, live_ids, label):
    """Every in-view slot sits on its PDF rect (mapped from the page box), and
    every live in-view widget's native view sits exactly on its slot."""
    ok = True
    kids = children()
    for s in lay["slots"]:
        rect = WIDGETS[s["id"]]["rect"]
        pb, W, H = s["pageBox"], s["pageWidthPt"], s["pageHeightPt"]
        want = (pb["x"] + rect["x0"] / W * pb["w"], pb["y"] + (H - rect["y1"]) / H * pb["h"],
                (rect["x1"] - rect["x0"]) / W * pb["w"], (rect["y1"] - rect["y0"]) / H * pb["h"])
        got = (s["slot"]["x"], s["slot"]["y"], s["slot"]["w"], s["slot"]["h"])
        ok &= check(f"{label}: {s['id']} slot on its pdf rect", all(abs(a - b) <= 1.5 for a, b in zip(got, want)),
                    f"slot {got} rect maps to {tuple(round(v, 1) for v in want)}")
        if s["id"] in live_ids:
            hit = [k for k in kids if all(abs(a - b) <= 1 for a, b in zip(k, got))]
            ok &= check(f"{label}: {s['id']} native view on its slot", len(hit) == 1, f"slot {got} children {kids}")
    return ok

def loaded_ids(since):
    return {ev["id"] for ev in events("widget.loaded", since)}

# 3a. a fresh project: posters only.
t0 = now_ms()
lay = start(True)
check("the preview reports widget slots", lay is not None)
time.sleep(2)
check("a fresh project starts no widget", not events("widget.launched", t0) and not helpers(),
      f"launched {events('widget.launched', t0)} helpers {helpers()}")
lay0 = scroll_to({"tab-results"}, t0)
check("scrolled to the table", lay0 is not None)
time.sleep(1.5)
check("still nothing runs in view of a widget", not events("widget.launched", t0) and not helpers())
app.shot("01-posters")

# 3b. run approval: live widgets in view start, each in its own process.
m = now_ms()
palette("Run Widgets")
ap = log.wait("widgets.approval", m, 10, lambda ev: ev.get("scope") == "run" and ev.get("granted"))
check("the run approval is a typed event", ap is not None and ap["event"]["run"] and not ap["event"]["network"],
      str(ap))
ok = log.wait("widget.loaded", m, 20)
check("an in-view widget starts and loads", ok is not None)
time.sleep(1.5)
hs = helpers()
launched = events("widget.launched", m)
check("each live widget is its own process, within the cap",
      0 < len(hs) == len({ev["id"] for ev in launched}) <= 6, f"helpers {hs} launched {launched}")
check("only runnable widgets start", {ev["id"] for ev in launched} <= RUNNABLE, str(launched))
check("network stays denied after the run approval",
      all(not ev["network"] and ev["contained"] for ev in launched), str(launched))
app.shot("02-running")

# 3c. alignment at two zooms.
live = loaded_ids(m)
lay = layout_after(t0, 5)
check("a settled layout after start", lay is not None)
if lay:
    aligned(lay, live, f"zoom {lay['percent']}%")
z = now_ms()
for _ in range(3):
    app.key("ctrl+minus")
lay2 = layout_after(z, 10, lambda ev: lay is None or ev["percent"] != lay["percent"])
check("the zoom changed the layout", lay2 is not None)
time.sleep(1.5)
lay2 = layout_after(z, 5, lambda ev: lay is None or ev["percent"] != lay["percent"]) or lay2
if lay2:
    live = loaded_ids(m)
    check("live widgets in view after the zoom", any(s["id"] in live for s in lay2["slots"]), str(lay2)[:300])
    aligned(lay2, live, f"zoom {lay2['percent']}%")
app.shot("03-zoomed")

# 3d. network approval: only the widget that declares an origin restarts.
lay3 = scroll_to({"fig-html"}, now_ms() - 1)
check("scrolled to the html widget", lay3 is not None)
html_up = log.wait("widget.launched", m, 15, lambda ev: ev["id"] == "fig-html")
check("the html widget starts without network", html_up is not None and not html_up["event"]["network"],
      str(html_up))
n = now_ms()
launched_before = {ev["id"] for ev in events("widget.launched", t0)}
palette("Allow Widget Network")
ap = log.wait("widgets.approval", n, 10, lambda ev: ev.get("scope") == "network" and ev.get("granted"))
check("the network approval is its own typed event", ap is not None and ap["event"]["run"] and ap["event"]["network"])
# a restarted widget starts again when it is next in view: bring it back
scroll_to({"fig-html"}, n)
relaunch = log.wait("widget.launched", n, 15, lambda ev: ev["id"] == "fig-html")
check("the html widget restarts with network", relaunch is not None and relaunch["event"]["network"]
      and not relaunch["event"]["contained"], str(relaunch))
time.sleep(1)
# a widget first seen only now (scrolled into view) is a start, not a restart
# one the live cap suspended and the scroll brought back is a resume, not a restart
suspended = {ev["id"] for ev in events("widget.suspended", n)}
others = [ev for ev in events("widget.launched", n)
          if ev["id"] != "fig-html" and ev["id"] in launched_before and ev["id"] not in suspended]
check("widgets that declare nothing are not restarted", not others, str(others))

# 3e. approvals persist: a restart runs widgets with no new approval.
app.stop()
time.sleep(1)
check("helpers end with the app", not helpers(), str(helpers()))
r0 = now_ms()
start(False)
lay = scroll_to({"fig-html"}, r0)
up = log.wait("widget.launched", r0, 20, lambda ev: ev["id"] == "fig-html")
check("after a restart the approvals still hold", up is not None and up["event"]["network"]
      and not events("widgets.approval", r0), str(up))

# 3f. revoking takes effect at once, without a restart.
v = now_ms()
palette("Run Widgets")
ap = log.wait("widgets.approval", v, 10, lambda ev: ev.get("scope") == "run" and not ev.get("granted"))
check("the revocation is a typed event", ap is not None and not ap["event"]["run"])
end = time.time() + 5
while helpers() and time.time() < end:
    time.sleep(0.2)
check("revoking ends every helper without a restart", not helpers(), str(helpers()))
app.shot("04-revoked")
app.stop()
finish()
PYEOF
echo ""
echo "WIDGET PROOFS COMPLETE (parts: $PARTS)"
