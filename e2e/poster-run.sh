#!/bin/bash
# Auto-poster run. Compiles e2e/fixtures/interactive/posters.tex over the
# stdio sidecar, then drives the app's headless renderer
# (`maleficium --render-posters <root>`, one JSON request per stdin line)
# under its own Xvfb: one fixture model from two cameras (pos/target/up and
# a matrix) gives two distinct non-blank PNGs of the requested size; a chart
# renders at its scale; a deliberately hanging debug-only runtime is killed
# at its time limit, its web process is gone, and the next render still
# works; a table widget is refused; the project, explicit posters
# included, is never written.
#
# The html gate, with a probe widget (no poster=) whose script, if it ever
# runs, paints the poster magenta (its marker): unapproved it returns
# approval_required and writes nothing; approved in the app data store (as
# the desktop's Approve writes it) it renders the marker; an edit stops it
# until re-approved; a symlink in its folder or a request for a digest the
# folder no longer has refuses. Then the two-pass compile: an approved probe
# is rendered into .maleficium/posters and mapped, an edited one gets an
# approval_required line and no map entry.
#
# Speculative loading: an approved widget with static and scripted
# preconnect links to a loopback listener renders, and the listener sees no
# TCP connection (the renderer turns LinkPreconnect off in its webview).
set -euo pipefail

DEVROOT="$(cd "$(dirname "$0")/.." && pwd)"
TARGET="${CARGO_TARGET_DIR:-$DEVROOT/src-tauri/target}/debug"
FIXTURE="$DEVROOT/e2e/fixtures/interactive"
SCRATCH="$(mktemp -d /tmp/maleficium-posters-XXXXXX)"
XVFB_PID=""
cleanup() {
  [ -n "$XVFB_PID" ] && kill "$XVFB_PID" 2>/dev/null || true
  # POSTER_SHOTS=<dir> keeps the rendered posters for a look.
  [ -n "${POSTER_SHOTS:-}" ] && mkdir -p "$POSTER_SHOTS" && cp "$SCRATCH"/out/* "$POSTER_SHOTS"/ 2>/dev/null
  rm -rf "$SCRATCH"
}
trap cleanup EXIT

fail() { echo "FAIL: $1"; exit 1; }

command -v Xvfb >/dev/null || fail "Xvfb is required"
cargo build -q --manifest-path "$DEVROOT/src-tauri/Cargo.toml" --bin maleficium-mcp --bin maleficium \
  || fail "cannot build the app and sidecar"

cp -r "$FIXTURE" "$SCRATCH/proj"
cp "$DEVROOT/src-tauri/interactive/maleficium-interactive.sty" "$SCRATCH/proj/"
mkdir "$SCRATCH/out"
# The probe: an html widget with no poster= whose script, when run, answers
# the bridge with a magenta snapshot.
mkdir -p "$SCRATCH/proj/widgets/probe"
cat > "$SCRATCH/proj/widgets/probe/index.html" <<'HTML'
<!doctype html><html><head><meta charset="utf-8"></head>
<body><canvas id="c" width="64" height="48"></canvas><script src="probe.js"></script></body></html>
HTML
cat > "$SCRATCH/proj/widgets/probe/probe.js" <<'JS'
var c = document.getElementById("c"), g = c.getContext("2d");
g.fillStyle = "#ff00ff"; g.fillRect(0, 0, 64, 48);
addEventListener("message", function (e) {
  if (e.source !== parent || !e.data || e.data.mfw !== 1) return;
  if (e.data.type === "init") parent.postMessage({ mfw: 1, type: "status", state: "loaded" }, "*");
  else if (e.data.type === "snapshot-request")
    parent.postMessage({ mfw: 1, type: "snapshot", requestId: e.data.requestId, png: c.toDataURL("image/png") }, "*");
});
parent.postMessage({ mfw: 1, type: "ready" }, "*");
JS
sed -i 's|^\\interactivetable|\\interactive[height=3cm, id=html-probe, alt={Probe widget}]{widgets/probe/}\n\n\\interactive[height=3cm, id=html-pc, alt={Preconnect probe}]{widgets/pc/}\n\n\\interactivetable|' "$SCRATCH/proj/posters.tex"
grep -q "id=html-probe" "$SCRATCH/proj/posters.tex" || fail "cannot add the probe widget"
grep -q "id=html-pc" "$SCRATCH/proj/posters.tex" || fail "cannot add the preconnect probe widget"

# A private display: the renderer's window is never shown, but GTK needs one.
exec 3< <(Xvfb -displayfd 1 -screen 0 1280x800x24 -nolisten tcp 2>/dev/null & echo "pid $!")
read -r -t 20 _ XVFB_PID <&3 || fail "Xvfb did not start"
read -r -t 20 DISPNUM <&3 || fail "Xvfb gave no display"
export DISPLAY=":$DISPNUM"

export MCP_BIN="$TARGET/maleficium-mcp" APP_BIN="$TARGET/maleficium"
export ROOT="$SCRATCH/proj" OUT="$SCRATCH/out" DEVROOT
# Reuse the warm engine cache: the first cold download is minutes.
export XDG_CACHE_HOME="${DRIVER_CACHE:-$HOME/.cache}"
export XDG_DATA_HOME="$SCRATCH/data"

python3 - <<'EOF'
import hashlib, json, os, re, subprocess, sys, time, zlib
ROOT, OUT = os.environ["ROOT"], os.environ["OUT"]
fails = []
def check(name, cond, detail=""):
    print(("ok: " if cond else "FAIL: ") + name + (f" ({detail})" if detail and not cond else ""))
    if not cond:
        fails.append(name)

sys.path.insert(0, os.path.join(os.environ["DEVROOT"], "e2e"))
from mcp_client import McpClient

# The preconnect probe: an html widget whose markup asks the engine to
# preconnect to a loopback listener (static links, as a hostile widget would
# write them, plus one made by script). Its strict policy does not stop that
# in WebKit; only the renderer turning LinkPreconnect off does. Any TCP accept
# on the listener is the leak.
import socket, threading
pc_sock = socket.socket()
pc_sock.bind(("127.0.0.1", 0))
pc_sock.listen(16)
PC_PORT, pc_hits = pc_sock.getsockname()[1], []
def pc_accept():
    while True:
        try:
            c, _ = pc_sock.accept()
        except OSError:
            return
        pc_hits.append(time.time())
        c.close()
threading.Thread(target=pc_accept, daemon=True).start()
os.makedirs(os.path.join(ROOT, "widgets/pc"), exist_ok=True)
open(os.path.join(ROOT, "widgets/pc/index.html"), "w").write(f"""<!doctype html><html><head><meta charset="utf-8">
<link rel="preconnect" href="http://127.0.0.1:{PC_PORT}">
<link rel="preconnect" href="http://127.0.0.1:{PC_PORT}" crossorigin>
</head><body><link rel="preconnect" href="http://localhost:{PC_PORT}">
<canvas id="c" width="32" height="24"></canvas><script src="pc.js"></script></body></html>
""")
open(os.path.join(ROOT, "widgets/pc/pc.js"), "w").write(f"""var l = document.createElement("link");
l.rel = "preconnect"; l.href = "http://127.0.0.1:{PC_PORT}/js"; document.head.appendChild(l);
var c = document.getElementById("c"), g = c.getContext("2d");
g.fillStyle = "#00ffff"; g.fillRect(0, 0, 32, 24);
addEventListener("message", function (e) {{
  if (e.source !== parent || !e.data || e.data.mfw !== 1) return;
  if (e.data.type === "init") setTimeout(function () {{ parent.postMessage({{ mfw: 1, type: "status", state: "loaded" }}, "*"); }}, 500);
  else if (e.data.type === "snapshot-request")
    parent.postMessage({{ mfw: 1, type: "snapshot", requestId: e.data.requestId, png: c.toDataURL("image/png") }}, "*");
}});
parent.postMessage({{ mfw: 1, type: "ready" }}, "*");
""")

def tree(root):
    out = {}
    for d, _, files in os.walk(root):
        for f in files:
            p = os.path.join(d, f)
            out[os.path.relpath(p, root)] = hashlib.sha256(open(p, "rb").read()).hexdigest()
    return out

mcp = McpClient([os.environ["MCP_BIN"]], "posters")
def call(name, args):
    ok, r = mcp.tool(name, args)
    return {"ok": True, **r} if ok else {"ok": False, "error": r}

check("grant project root", call("grant", {"root_id": "pp", "root": ROOT})["ok"])
before = tree(ROOT)
r = call("compile_run", {"root_id": "pp", "rel": "posters.tex"})
rec = {"status": "not-started", "log": str(r)}
for _ in range(120 if r.get("job_id") else 0):
    time.sleep(3)
    rec = call("compile_poll", {"job_id": r["job_id"], "tail_lines": 5})
    if rec.get("status") != "running":
        break
check("posters.tex compiles", rec.get("status") == "success", str(rec)[:400])
wl = call("widgets", {"root_id": "pp", "main_rel": "posters.tex"})
ws = {w["id"]: w for w in wl.get("widgets") or []}
opt = lambda w, k: next((o["value"] for o in ws.get(w, {}).get("options", []) if o["key"] == k), None)
check("the pos/target/up camera is recorded as a canonical matrix",
      opt("cam-front", "camera") == "1 0 0 0 0 1 0 0 0 0 1 0 0 0 3 1", str(ws.get("cam-front"))[:300])
cam = [float(v) for v in (opt("cam-oblique", "camera") or "").split()]
check("the matrix camera is recorded canonical and rigid",
      len(cam) == 16 and abs(cam[12] - 2) < 1e-6 and abs(cam[13] - 1.5) < 1e-6
      and abs(sum(v * v for v in cam[0:3]) - 1) < 1e-4, str(cam))
check("size, background and scale are recorded",
      opt("cam-front", "size") == "320x240" and opt("cam-front", "background") == "#ffffff"
      and opt("chart", "scale") == "2", str(ws)[:300])
mcp.p.kill()

def png_info(path):
    """(width, height, share of pixels unlike the top-left one, sha256, the
    top-left pixel) of an 8-bit RGB or RGBA png."""
    b = open(path, "rb").read()
    if b[:8] != b"\x89PNG\r\n\x1a\n":
        return None
    w, h = int.from_bytes(b[16:20], "big"), int.from_bytes(b[20:24], "big")
    bpp = {2: 3, 6: 4}.get(b[25])
    if b[24] != 8 or bpp is None or b[28] != 0:
        return w, h, None, hashlib.sha256(b).hexdigest(), None
    idat, i = b"", 8
    while i < len(b):
        n = int.from_bytes(b[i:i + 4], "big")
        if b[i + 4:i + 8] == b"IDAT":
            idat += b[i + 8:i + 8 + n]
        i += 12 + n
    raw, stride, prev, rows = zlib.decompress(idat), w * bpp, bytearray(w * bpp), []
    for y in range(h):
        f, line = raw[y * (stride + 1)], bytearray(raw[y * (stride + 1) + 1:(y + 1) * (stride + 1)])
        for x in range(stride):
            a = line[x - bpp] if x >= bpp else 0
            c = prev[x - bpp] if x >= bpp else 0
            up = prev[x]
            if f == 1: line[x] = (line[x] + a) & 255
            elif f == 2: line[x] = (line[x] + up) & 255
            elif f == 3: line[x] = (line[x] + (a + up) // 2) & 255
            elif f == 4:
                p = a + up - c
                pa, pb, pc = abs(p - a), abs(p - up), abs(p - c)
                line[x] = (line[x] + (a if pa <= pb and pa <= pc else up if pb <= pc else c)) & 255
        rows.append(bytes(line))
        prev = line
    corner = rows[0][:bpp]
    other = sum(1 for r in rows for x in range(0, stride, bpp) if r[x:x + bpp] != corner)
    return w, h, other / (w * h), hashlib.sha256(b).hexdigest(), tuple(corner)

def web_processes(pid):
    """WebKitWebProcess descendants of pid."""
    kids, found, todo = {}, [], [pid]
    for d in os.listdir("/proc"):
        if d.isdigit():
            try:
                stat = open(f"/proc/{d}/stat").read()
                ppid = int(stat.rsplit(")", 1)[1].split()[1])
                kids.setdefault(ppid, []).append(int(d))
            except (OSError, ValueError, IndexError):
                pass
    while todo:
        p = todo.pop()
        for c in kids.get(p, []):
            todo.append(c)
            try:
                if "WebKitWebProcess" in open(f"/proc/{c}/cmdline", "rb").read().decode("utf-8", "replace"):
                    found.append(c)
            except OSError:
                pass
    return found

app = subprocess.Popen([os.environ["APP_BIN"], "--render-posters", ROOT], stdin=subprocess.PIPE,
                       stdout=subprocess.PIPE, stderr=open(os.path.join(OUT, "app.log"), "w"), text=True)
def render(widget, name, timeout_ms=None):
    req = {"mainRel": "posters.tex", "widgetId": widget, "outPath": os.path.join(OUT, name)}
    if timeout_ms:
        req["timeoutMs"] = timeout_ms
    t0 = time.time()
    app.stdin.write(json.dumps(req) + "\n")
    app.stdin.flush()
    line = app.stdout.readline()
    return (json.loads(line) if line else {"ok": False, "error": "the app exited"}), time.time() - t0

front, _ = render("cam-front", "front.png")
check("the front camera renders", front["ok"], str(front)[:300])
fi = png_info(os.path.join(OUT, "front.png")) if front["ok"] else None
check("the front poster is a 320x240 png", fi is not None and fi[:2] == (320, 240), str(fi))
check("the front poster shows the model over a white background",
      fi is not None and fi[2] is not None and 0.1 < fi[2] < 0.9 and front["ok"], str(fi))
obl, _ = render("cam-oblique", "oblique.png")
check("the matrix camera renders", obl["ok"], str(obl)[:300])
oi = png_info(os.path.join(OUT, "oblique.png")) if obl["ok"] else None
check("the oblique poster is a 320x240 png", oi is not None and oi[:2] == (320, 240), str(oi))
check("the two cameras give two different posters", fi and oi and fi[3] != oi[3], str((fi, oi)))
check("the oblique poster shows the model", oi is not None and oi[2] is not None and 0.1 < oi[2] < 0.9, str(oi))
check("background=ffffff paints white, background=transparent leaves alpha 0",
      fi and oi and fi[4][:3] == (255, 255, 255) and (len(fi[4]) == 3 or fi[4][3] == 255)
      and len(oi[4]) == 4 and oi[4][3] == 0, str((fi and fi[4], oi and oi[4])))
check("the reported size and hash match the file",
      obl.get("ok") and obl["result"]["width"] == 320 and obl["result"]["sha256"] == (oi or [0, 0, 0, ""])[3],
      str(obl)[:300])

chart, _ = render("chart", "chart.png")
check("the chart renders", chart["ok"], str(chart)[:300])
ci = png_info(os.path.join(OUT, "chart.png")) if chart["ok"] else None
frame_w = round((ws["chart"]["rect"]["x1"] - ws["chart"]["rect"]["x0"]) * 96 / 72) if "chart" in ws else 0
check("the chart poster fills its frame at scale 2",
      ci is not None and abs(ci[0] - 2 * frame_w) <= 2 and ci[2] is not None and ci[2] > 0.1,
      str((ci, frame_w)))

hang, took = render("hang", "hang.png", timeout_ms=3000)
check("a hanging runtime is stopped at its time limit",
      not hang["ok"] and "within 3000 ms" in hang.get("error", "") and 2.9 <= took < 10,
      str((hang, round(took, 2))))
check("a hanging runtime writes no poster", not os.path.exists(os.path.join(OUT, "hang.png")))
end, left = time.time() + 5, None
while time.time() < end:
    left = web_processes(app.pid)
    if not left:
        break
    time.sleep(0.2)
check("the hanging runtime's web process is gone", not left, str(left))
again, _ = render("cam-front", "again.png")
check("the renderer still works after a kill", again["ok"], str(again)[:300])

r, _ = render("tab", "tab.png")
check("tab is refused", not r["ok"] and "typeset rows" in r.get("error", ""), str(r)[:300])

# --- the html gate -----------------------------------------------------------
REAL = os.path.realpath(ROOT)
DATA = os.environ["XDG_DATA_HOME"]
store_dir = os.path.join(DATA, "io.github.wahahayes.maleficium", "maleficium-widgets", "approvals",
                         hashlib.sha256(REAL.encode()).hexdigest()[:32])
approved = {}
def approve(digest, folder="widgets/probe", widget="html-probe"):
    """The approval the desktop's Approve writes (MCP and the renderer cannot)."""
    os.makedirs(store_dir, exist_ok=True)
    approved[folder] = {"widget": widget, "digest": digest, "origins": {},
                        "files": {}, "approvedAt": 1, "revoked": False}
    json.dump({"format": 1, "root": REAL, "autoApprove": False, "widgets": approved},
              open(os.path.join(store_dir, "store.json"), "w"))
def outcome(r):
    return (r.get("result") or {}).get("status") if r.get("ok") else None
MAGENTA = (255, 0, 255)
def marker(name):
    """The probe ran: its magenta 64x48 snapshot is the poster."""
    i = png_info(os.path.join(OUT, name)) if os.path.exists(os.path.join(OUT, name)) else None
    return i is not None and i[:2] == (64, 48) and i[4][:3] == MAGENTA

r, _ = render("html-demo", "html-demo.png")
check("an unapproved html widget (poster= or not) returns approval_required, not an error",
      outcome(r) == "approval_required" and r["result"].get("cause") == "never_approved"
      and r["result"].get("panel") == "View > Widgets", str(r)[:300])
check("...and writes nothing", not os.path.exists(os.path.join(OUT, "html-demo.png")))

r, _ = render("html-probe", "probe-unapproved.png")
check("the unapproved probe returns approval_required", outcome(r) == "approval_required"
      and r["result"].get("widget") == "html-probe" and r["result"].get("path") == "widgets/probe", str(r)[:300])
check("...and its script never ran (no marker)", not os.path.exists(os.path.join(OUT, "probe-unapproved.png")))
DIGEST = (r.get("result") or {}).get("digest", "")
check("...naming the digest the user would approve", len(DIGEST) == 64, DIGEST)

approve(DIGEST)
r, _ = render("html-probe", "probe-approved.png")
check("the approved probe renders", outcome(r) == "rendered", str(r)[:300])
check("...its script ran: the poster is its magenta 64x48 snapshot", marker("probe-approved.png"),
      str(png_info(os.path.join(OUT, "probe-approved.png")) if os.path.exists(os.path.join(OUT, "probe-approved.png")) else None))

r, _ = render("html-pc", "pc-unapproved.png")
check("the unapproved preconnect probe returns approval_required", outcome(r) == "approval_required", str(r)[:300])
approve((r.get("result") or {}).get("digest", ""), "widgets/pc", "html-pc")
r, _ = render("html-pc", "pc.png")
check("the approved preconnect probe renders (its script ran)", outcome(r) == "rendered"
      and (png_info(os.path.join(OUT, "pc.png")) or [0] * 5)[4][:3] == (0, 255, 255), str(r)[:300])
time.sleep(2)
check("the poster renderer opens no preconnect: LinkPreconnect is off in its webview",
      not pc_hits, f"{len(pc_hits)} TCP accepts on 127.0.0.1:{PC_PORT}")
applied = [l for l in open(os.path.join(OUT, "app.log")) if l.startswith("speculative loading off in poster-")]
check("...and every render window reports LinkPreconnect off",
      applied and all("LinkPreconnect," in l for l in applied), str(applied[:2]))
pc_sock.close()
approved.pop("widgets/pc")  # the next approve() drops it: the compile below counts one poster

js = os.path.join(ROOT, "widgets/probe/probe.js")
orig = open(js).read()
open(js, "a").write("// edited after approval\n")
r, _ = render("html-probe", "probe-edited.png")
check("an edit after approval stops it: approval_required (changed_since_approval)",
      outcome(r) == "approval_required" and r["result"].get("cause") == "changed_since_approval"
      and r["result"].get("approvedDigest") == DIGEST, str(r)[:300])
check("...and nothing ran", not os.path.exists(os.path.join(OUT, "probe-edited.png")))
EDITED = (r.get("result") or {}).get("digest", "")
approve(EDITED)
r, _ = render("html-probe", "probe-reapproved.png")
check("re-approved at its new digest, it renders again", outcome(r) == "rendered" and marker("probe-reapproved.png"),
      str(r)[:300])

req = {"mainRel": "posters.tex", "widgetId": "html-probe", "outPath": os.path.join(OUT, "probe-stale.png"),
       "digest": DIGEST}
app.stdin.write(json.dumps(req) + "\n")
app.stdin.flush()
r = json.loads(app.stdout.readline() or '{"ok": false, "error": "the app exited"}')
check("a request for a digest the folder no longer has is refused",
      not r["ok"] and "changed since" in r.get("error", ""), str(r)[:300])
check("...and nothing ran", not os.path.exists(os.path.join(OUT, "probe-stale.png")))

link = os.path.join(ROOT, "widgets/probe/leak.txt")
os.symlink("/etc/hostname", link)
r, _ = render("html-probe", "probe-symlink.png")
check("a symlink in an approved widget's folder refuses the render",
      not r["ok"] and "symlink" in r.get("error", ""), str(r)[:300])
check("...and nothing ran", not os.path.exists(os.path.join(OUT, "probe-symlink.png")))
os.remove(link)

app.stdin.close()
try:
    code = app.wait(timeout=15)
except subprocess.TimeoutExpired:
    app.kill()
    code = None
check("the renderer exits at end of input, non-zero after failures", code == 1, str(code))
after = {k: v for k, v in tree(ROOT).items() if k != "widgets/probe/probe.js"}
check("the project is untouched, explicit posters included",
      after == {k: v for k, v in before.items() if k != "widgets/probe/probe.js"},
      str(set(after.items()) ^ set(before.items()))[:300])

# --- the two-pass compile ------------------------------------------------------
# The sidecar renders missing auto-posters through the app binary beside it;
# the probe is approved at its current (edited) digest.
mcp = McpClient([os.environ["MCP_BIN"]], "posters-compile")
check("grant project root again", call("grant", {"root_id": "pp", "root": ROOT})["ok"])
def compile_lines():
    r = call("compile_run", {"root_id": "pp", "rel": "posters.tex"})
    rec = {"status": "not-started", "log": str(r)}
    for _ in range(120 if r.get("job_id") else 0):
        time.sleep(2)
        rec = call("compile_poll", {"job_id": r["job_id"], "tail_lines": 400})
        if rec.get("status") != "running":
            break
    return rec
MAP = os.path.join(ROOT, ".maleficium", "posters", "posters.map")
def mapped():
    return os.path.isfile(MAP) and "\\mfw@postermap{html-probe}" in open(MAP).read()
rec = compile_lines()
lines = rec.get("lines") or []
check("compile: the approved probe is rendered before the engine runs",
      rec.get("status") == "success" and any(l.startswith("poster html-probe: rendered") for l in lines),
      str([l for l in lines if "poster" in l])[:400])
check("...and mapped", mapped())
pngs = [f for f in os.listdir(os.path.dirname(MAP)) if f.endswith(".png")] if os.path.isfile(MAP) else []
check("...its cached poster is the probe's magenta snapshot",
      len(pngs) == 1 and (png_info(os.path.join(os.path.dirname(MAP), pngs[0])) or [0] * 5)[4][:3] == MAGENTA, str(pngs))
open(js, "w").write(orig + "// edited again\n")
rec = compile_lines()
lines = rec.get("lines") or []
check("compile: an edited probe gets an approval_required line, not a render",
      rec.get("status") == "success"
      and any(l.startswith("poster html-probe: approval_required (changed_since_approval)") for l in lines)
      and not any(l.startswith("poster html-probe: rendered") for l in lines),
      str([l for l in lines if "poster" in l])[:400])
check("...and no map entry: its cached poster does not stand in for the approval", not mapped())
# The finished compile also asked the app: a typed event in the log, which the
# window turns into its Approve | Skip prompt.
log = os.path.join(DATA, "io.github.wahahayes.maleficium", "maleficium-log", "events.jsonl")
asked = [l for l in (json.loads(x) for x in open(log)) if (l.get("event") or {}).get("action") == "widget.approval-required"] if os.path.isfile(log) else []
check("compile: the unapproved html widget is logged as widget.approval-required for the app",
      any(l["event"].get("widget") == "html-probe" and l["event"].get("cause") == "changed_since_approval"
          and l["event"].get("digest") and l["actor"] == "agent" for l in asked),
      str(asked)[:400])
mcp.p.kill()
sys.exit(1 if fails else 0)
EOF
echo ""
echo "POSTER PROOFS COMPLETE: headless renders green."
