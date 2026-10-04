#!/bin/bash
# Poster cache run. A fresh project (the interactive fixture plus auto.tex,
# whose model and chart give no poster=) compiles over the stdio sidecar,
# which renders auto-posters through the app's headless renderer (the
# `maleficium` binary beside it) under a private Xvfb:
#   1. the first compile has no widget list before the engine runs, so it
#      renders both posters after it, into .maleficium/posters/, and
#      compiles again: the pdf it returns embeds them (pdfimages), never
#      placeholders; README and map are written;
#   2. an unchanged compile renders nothing (same files, same mtimes);
#   3. deleting .maleficium regenerates it on the next compile;
#   4. an edited chart never shows its stale poster: the compile renders
#      the new one, embeds it, and collects the old;
#   5. porcelain shows only the edited paper and .maleficium/, search and
#      find_files never list the cache, the watcher never queues it (core
#      test), and a hand-edited README survives;
#   6. export_zip carries posters, map and README; the unzipped paper
#      compiles with its posters through a sidecar with no app beside it
#      and no display, leaving .maleficium as it was; the bundle export
#      takes the cached posters.
set -euo pipefail

DEVROOT="$(cd "$(dirname "$0")/.." && pwd)"
TARGET="${CARGO_TARGET_DIR:-$DEVROOT/src-tauri/target}/debug"
FIXTURE="$DEVROOT/e2e/fixtures/interactive"
SCRATCH="$(mktemp -d /tmp/maleficium-poster-cache-XXXXXX)"
XVFB_PID=""
cleanup() {
  [ -n "$XVFB_PID" ] && kill "$XVFB_PID" 2>/dev/null || true
  rm -rf "$SCRATCH"
}
trap cleanup EXIT

fail() { echo "FAIL: $1"; exit 1; }

command -v Xvfb >/dev/null || fail "Xvfb is required"
command -v pdfimages >/dev/null || fail "pdfimages (poppler-utils) is required"
cargo build -q --manifest-path "$DEVROOT/src-tauri/Cargo.toml" --bin maleficium-mcp --bin maleficium \
  || fail "cannot build the app and sidecar"

# The watcher half runs as a core test: no MCP tool drives the watcher.
timeout 600 cargo test -q --manifest-path "$DEVROOT/src-tauri/Cargo.toml" -p maleficium-core --lib \
  watch::tests::watcher_never_queues_poster_cache_writes 2>&1 | grep -q "1 passed" \
  || fail "the watcher queues poster cache writes"
echo "ok: the watcher never queues poster cache writes (core test)"

cp -r "$FIXTURE" "$SCRATCH/proj"
cp "$DEVROOT/src-tauri/interactive/maleficium-interactive.sty" "$SCRATCH/proj/"
cat > "$SCRATCH/proj/auto.tex" <<'TEX'
\documentclass{article}
\usepackage{maleficium-interactive}
\begin{document}
\interactivemodel[height=4cm, id=auto-model, size=320x240, background=ffffff,
  camera={0.707107, 0, -0.707107, 0, 0, 1, 0, 0, 0.707107, 0, 0.707107, 0, 2, 0, 2, 1},
  alt={The mesh from the side}]{models/mesh.glb}

\interactivechart[height=4cm, id=auto-chart, alt={Ablation chart}]{charts/ablation.vl.json}
\end{document}
TEX
(cd "$SCRATCH/proj" && git init -q && git add -A && git -c user.email=e2e@local -c user.name=e2e commit -qm fixture)
mkdir -p "$SCRATCH/lonely" "$SCRATCH/unzipped"
# A sidecar with no app beside it: the "no Maleficium" compile.
cp "$TARGET/maleficium-mcp" "$SCRATCH/lonely/"

exec 3< <(Xvfb -displayfd 1 -screen 0 1280x800x24 -nolisten tcp 2>/dev/null & echo "pid $!")
read -r -t 20 _ XVFB_PID <&3 || fail "Xvfb did not start"
read -r -t 20 DISPNUM <&3 || fail "Xvfb gave no display"
export DISPLAY=":$DISPNUM"

export MCP_BIN="$TARGET/maleficium-mcp" LONELY_BIN="$SCRATCH/lonely/maleficium-mcp"
export ROOT="$SCRATCH/proj" UNZIPPED="$SCRATCH/unzipped" SCRATCH DEVROOT
export XDG_CACHE_HOME="${DRIVER_CACHE:-$HOME/.cache}"
export XDG_DATA_HOME="$SCRATCH/data"

python3 - <<'EOF'
import hashlib, os, subprocess, sys, time, zipfile
ROOT, SCRATCH = os.environ["ROOT"], os.environ["SCRATCH"]
CACHE = os.path.join(ROOT, ".maleficium")
POSTERS = os.path.join(CACHE, "posters")
fails = []
def check(name, cond, detail=""):
    print(("ok: " if cond else "FAIL: ") + name + (f" ({detail})" if detail and not cond else ""))
    if not cond:
        fails.append(name)

sys.path.insert(0, os.path.join(os.environ["DEVROOT"], "e2e"))
from mcp_client import McpClient

def client(binary, name, env=None):
    mcp = McpClient([binary], name, env=env)
    def call(tool, args):
        ok, r = mcp.tool(tool, args)
        return {"ok": True, **r} if ok else {"ok": False, "error": r}
    return mcp, call

mcp, call = client(os.environ["MCP_BIN"], "poster-cache")

def compile(call, root_id, rel="auto.tex"):
    r = call("compile_run", {"root_id": root_id, "rel": rel})
    rec = {"status": "not-started", "log": str(r)}
    for _ in range(160 if r.get("job_id") else 0):
        time.sleep(2)
        rec = call("compile_poll", {"job_id": r["job_id"], "tail_lines": 400})
        if rec.get("status") != "running":
            break
    return rec

def posters_lines(rec):
    """Every summary line of the compile: one per pass that looked at the cache."""
    return [l for l in rec.get("lines") or [] if l.startswith("posters: ") and "cached" in l]

def images(pdf):
    """(width, height) of every image the pdf embeds (soft masks aside)."""
    out = subprocess.run(["pdfimages", "-list", pdf], capture_output=True, text=True).stdout.splitlines()[2:]
    rows = [l.split() for l in out if l.split()]
    return sorted((int(r[3]), int(r[4])) for r in rows if r[2] == "image")

def pngs():
    return sorted(f for f in os.listdir(POSTERS) if f.endswith(".png")) if os.path.isdir(POSTERS) else []

def stamps():
    return {f: os.stat(os.path.join(POSTERS, f)).st_mtime_ns for f in os.listdir(POSTERS)}

def tree(root):
    out = {}
    for d, _, files in os.walk(root):
        for f in files:
            p = os.path.join(d, f)
            out[os.path.relpath(p, root)] = hashlib.sha256(open(p, "rb").read()).hexdigest()
    return out

check("grant project root", call("grant", {"root_id": "pc", "root": ROOT})["ok"])

# 1. First compile: no widget list before the engine, so the posters render
# after it and a second engine run embeds them.
r1 = compile(call, "pc")
check("the first compile succeeds", r1.get("status") == "success", str(r1)[:400])
pdf = r1.get("pdf_url") or ""
check("the first compile renders both posters",
      "posters: 0 cached, 2 rendered, 0 placeholder" in posters_lines(r1), str(posters_lines(r1)))
imgs = images(pdf)
wl = call("widgets", {"root_id": "pc", "main_rel": "auto.tex"})
ws = {w["id"]: w for w in wl.get("widgets") or []}
check("both widgets are listed without a poster=",
      set(ws) == {"auto-model", "auto-chart"} and all("poster" not in w for w in ws.values()), str(wl)[:300])
rect1 = {k: w["rect"] for k, w in ws.items()}
chart_frame = round((rect1["auto-chart"]["x1"] - rect1["auto-chart"]["x0"]) * 96 / 72) if rect1 else 0
check("the first compile's pdf embeds the model poster at its size", (320, 240) in imgs, str(imgs))
check("...and the chart poster at twice its frame",
      len(imgs) == 2 and any(abs(w - 2 * chart_frame) <= 2 for w, _ in imgs), str((imgs, chart_frame)))
check("two posters in the cache", len(pngs()) == 2, str(pngs()))
check("the README is written", os.path.isfile(os.path.join(CACHE, "README.md")))
mp = os.path.join(POSTERS, "auto.map")
mtext = open(mp).read() if os.path.isfile(mp) else ""
check("the map lists both widgets by poster hash",
      all(f"\\mfw@postermap{{{w}}}" in mtext for w in ("auto-model", "auto-chart"))
      and all(p[:-4] in mtext for p in pngs()), mtext[:400])

# 2. Unchanged: nothing renders.
before = stamps()
r3 = compile(call, "pc")
check("an unchanged compile renders nothing",
      posters_lines(r3) and all(l == "posters: 2 cached, 0 rendered, 0 placeholder" for l in posters_lines(r3)),
      str(posters_lines(r3)))
check("...and leaves every cache file as it was", stamps() == before, str((before, stamps())))
check("...and still embeds both posters", len(images(pdf)) == 2, str(images(pdf)))

# 5a. A hand-edited README survives compiles.
open(os.path.join(CACHE, "README.md"), "a").write("my note\n")

# 3. Deleted: regenerated.
import shutil
shutil.rmtree(CACHE)
r4 = compile(call, "pc")
check("after deleting .maleficium the next compile renders again",
      "posters: 0 cached, 2 rendered, 0 placeholder" in posters_lines(r4), str(posters_lines(r4)))
check("...and embeds both posters", len(images(pdf)) == 2, str(images(pdf)))
check("...and rewrites the README", os.path.isfile(os.path.join(CACHE, "README.md")))
open(os.path.join(CACHE, "README.md"), "a").write("my note\n")

# 4. An edited chart: never the stale poster; the new one renders in the same
# compile, and the old one is collected.
old = set(pngs())
tex = os.path.join(ROOT, "auto.tex")
src = open(tex).read()
open(tex, "w").write(src.replace("height=4cm, id=auto-chart", "height=3cm, id=auto-chart"))
r5 = compile(call, "pc")
check("the edited compile succeeds", r5.get("status") == "success", str(r5)[:300])
check("the edited compile renders the new chart poster",
      "posters: 1 cached, 1 rendered, 0 placeholder" in posters_lines(r5), str(posters_lines(r5)))
check("...and its pdf embeds both posters", len(images(pdf)) == 2, str(images(pdf)))
check("the stale chart poster is collected",
      len(pngs()) == 2 and len(set(pngs()) - old) == 1 and any("removed 1 unused" in l for l in r5.get("lines") or []),
      str((pngs(), [l for l in r5.get("lines") or [] if "poster" in l])))
check("the hand-edited README is kept", open(os.path.join(CACHE, "README.md")).read().endswith("my note\n"))

# 5. Porcelain, search, find_files.
st = subprocess.run(["git", "status", "--porcelain", "--untracked-files=normal"], cwd=ROOT,
                    capture_output=True, text=True).stdout.split("\n")
st = sorted(l for l in st if l)
check("porcelain shows only the edited paper and .maleficium/",
      st == [" M auto.tex", "?? .maleficium/"], str(st))
s = call("search", {"root_id": "pc", "pattern": "safe to delete"})
check("search never reads the cache (README and map say it)", s["ok"] and s.get("hits") == 0, str(s)[:300])
f = call("find_files", {"root_id": "pc", "query": "map"})
check("find_files never lists the cache", f["ok"] and ".maleficium" not in str(f), str(f)[:300])
lst = call("list", {"root_id": "pc", "rel": "."})
check("the file listing hides the cache", lst["ok"] and ".maleficium" not in str(lst), str(lst)[:300])

# 6. Export, then compile without the app.
b = call("export_bundle", {"root_id": "pc", "main_rel": "auto.tex",
                           "dest": os.path.join(SCRATCH, "bundle.html"), "profile": "single-file"})
check("the bundle export takes the cached posters", b["ok"], str(b)[:300])
zp = os.path.join(SCRATCH, "paper.zip")
z = call("export_zip", {"root_id": "pc", "dest": zp})
files = z.get("files") or []
check("export_zip carries the posters, the map and the README",
      z["ok"] and ".maleficium/README.md" in files and ".maleficium/posters/auto.map" in files
      and all(f".maleficium/posters/{p}" in files for p in pngs()), str(files)[:400])
mcp.p.kill()

UNZ = os.environ["UNZIPPED"]
zipfile.ZipFile(zp).extractall(UNZ)
cache_before = tree(os.path.join(UNZ, ".maleficium"))
env = {k: v for k, v in os.environ.items() if k not in ("DISPLAY", "WAYLAND_DISPLAY")}
lonely, lcall = client(os.environ["LONELY_BIN"], "no-app", env=env)
check("grant the unzipped paper", lcall("grant", {"root_id": "uz", "root": UNZ})["ok"])
r7 = compile(lcall, "uz")
check("the exported paper compiles without the app", r7.get("status") == "success", str(r7)[:300])
upd = r7.get("pdf_url") or ""
check("...with both posters from the exported cache", upd and len(images(upd)) == 2, str(upd and images(upd)))
check("...rendering nothing", all(l == "posters: 2 cached, 0 rendered, 0 placeholder" for l in posters_lines(r7)) and not any(l.startswith("poster ") for l in r7.get("lines") or []),
      str([l for l in r7.get("lines") or [] if "poster" in l]))
check("...and leaving .maleficium as it was", tree(os.path.join(UNZ, ".maleficium")) == cache_before)
shutil.rmtree(os.path.join(UNZ, ".maleficium"))
r8 = compile(lcall, "uz")
check("without .maleficium and without the app the paper still compiles, with placeholders",
      r8.get("status") == "success" and images(r8.get("pdf_url") or "") == [], str(r8)[:300])
check("...and reports that no renderer is available",
      any("no poster renderer" in l for l in r8.get("lines") or []), str([l for l in r8.get("lines") or [] if "poster" in l]))
lonely.p.kill()
sys.exit(1 if fails else 0)
EOF
echo ""
echo "POSTER CACHE PROOFS COMPLETE: two-pass compile, GC and export green."
