#!/bin/bash
# Auto-poster run. Compiles e2e/fixtures/interactive/posters.tex over the
# stdio sidecar, then drives the app's headless renderer
# (`maleficium --render-posters <root>`, one JSON request per stdin line)
# under its own Xvfb: one fixture model from two cameras (pos/target/up and
# a matrix) gives two distinct non-blank PNGs of the requested size; a chart
# renders at its scale; a deliberately hanging debug-only runtime is killed
# at its time limit, its web process is gone, and the next render still
# works; html and table widgets are refused; the project, explicit posters
# included, is never written.
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
mkdir "$SCRATCH/out"

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
check("install the package", call("interactive_install", {"root_id": "pp"})["ok"])
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

for widget, want in [("html-demo", "html widgets are not rendered"), ("tab", "typeset rows")]:
    r, _ = render(widget, widget + ".png")
    check(f"{widget} is refused", not r["ok"] and want in r.get("error", ""), str(r)[:300])

app.stdin.close()
try:
    code = app.wait(timeout=15)
except subprocess.TimeoutExpired:
    app.kill()
    code = None
check("the renderer exits at end of input, non-zero after failures", code == 1, str(code))
after = {k: v for k, v in tree(ROOT).items() if k != "maleficium-interactive.sty"}
check("the project is untouched, explicit posters included",
      after == {k: v for k, v in before.items() if k != "maleficium-interactive.sty"},
      str(set(after.items()) ^ set(before.items()))[:300])
sys.exit(1 if fails else 0)
EOF
echo ""
echo "POSTER PROOFS COMPLETE: headless renders green."
