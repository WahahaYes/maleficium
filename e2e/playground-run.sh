#!/bin/bash
# Playground-paper run over the stdio sidecar. Spawns `maleficium-mcp`,
# installs the interactive package into a scratch copy of
# e2e/fixtures/playground/, compiles main.tex, and asserts both that the PDF
# builds cleanly (bibliography resolved, no undefined references) and that
# the MCP `widgets` tool lists every widget the paper declares.
set -euo pipefail

DEVROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN="${CARGO_TARGET_DIR:-$DEVROOT/src-tauri/target}/debug/maleficium-mcp"
FIXTURE="$DEVROOT/e2e/fixtures/playground"
SCRATCH="$(mktemp -d /tmp/maleficium-playground-XXXXXX)"
trap 'rm -rf "$SCRATCH"' EXIT

fail() { echo "FAIL: $1"; exit 1; }
pass() { echo "ok: $1"; }

cargo build -q --manifest-path "$DEVROOT/src-tauri/Cargo.toml" --bin maleficium-mcp \
  || fail "cannot build maleficium-mcp"
[ -x "$BIN" ] || fail "sidecar missing after build: $BIN"

cp -r "$FIXTURE" "$SCRATCH/proj"
cd "$SCRATCH/proj"
git init -q
git add -A
git -c user.email=driver@local -c user.name=driver commit -qm "fixture"
ROOT="$SCRATCH/proj"

export MCP_BIN="$BIN" MCP_ROOT="$ROOT"
export DEVROOT
# Reuse the warm engine cache: the first cold download is minutes.
export XDG_CACHE_HOME="${DRIVER_CACHE:-$HOME/.cache}"
export XDG_DATA_HOME="$SCRATCH/data"
ENGINE_RS="$DEVROOT/src-tauri/core/src/engine.rs"
BUNDLE_DIGEST="$(sed -n 's/^pub const BUNDLE_DIGEST: &str = "\([0-9a-f]*\)";/\1/p' "$ENGINE_RS")"
[ -n "$BUNDLE_DIGEST" ] || fail "pinned bundle digest not found in $ENGINE_RS"
export BUNDLE_DIGEST
if [ -f "$XDG_CACHE_HOME/io.github.wahahayes.maleficium/maleficium-tectonic/$BUNDLE_DIGEST/bundles/data/$BUNDLE_DIGEST.index" ]; then
  export POLL_ROUNDS="${POLL_ROUNDS:-40}"
else
  export POLL_ROUNDS="${POLL_ROUNDS:-120}"
fi

python3 - <<'PYEOF'
import os, re, sys, time, zlib
BIN = os.environ["MCP_BIN"]
ROOT = os.environ["MCP_ROOT"]
ROUNDS = int(os.environ["POLL_ROUNDS"])
fails = []
def check(name, cond, detail=""):
    print(("ok: " if cond else "FAIL: ") + name + (f" ({detail})" if detail and not cond else ""))
    if not cond:
        fails.append(name)

sys.path.insert(0, os.path.join(os.environ["DEVROOT"], "e2e"))
from mcp_client import McpClient

def flat(result):
    ok, r = result
    return {"ok": True, **r} if ok else {"ok": False, "error": r}

mcp = McpClient([BIN], "playground")
def call(name, args):
    return flat(mcp.tool(name, args))

g = call("grant", {"root_id": "pg", "root": ROOT})
check("grant project root", g["ok"] and g["path"] == ROOT, str(g))
r = call("interactive_install", {"root_id": "pg"})
check("install writes the package", r["ok"] and r.get("file") == "maleficium-interactive.sty", str(r))

# subcaption is not in the warm cache on a fresh machine; networked lets the
# first run fetch it, later runs are offline.
r = call("compile_run", {"root_id": "pg", "rel": "main.tex", "networked": True})
job = r.get("job_id") or ""
check("compile starts", r["ok"] and bool(job), str(r))
rec = {"status": "running"}
for _ in range(ROUNDS):
    time.sleep(3)
    rec = call("compile_poll", {"job_id": job, "tail_lines": 40})
    if rec.get("status") != "running":
        break
check("playground compiles", rec.get("status") == "success", str(rec)[:400])
pdf = rec.get("pdf_url") or ""
check("compile reports the pdf", bool(pdf) and os.path.isfile(pdf), str(rec)[:200])
log = (rec.get("log") or "") + "\n" + "\n".join(rec.get("lines") or [])
for bad in ("undefined", "Citation", "Overfull", "! "):
    if bad == "undefined":
        check("no undefined references", not re.search(r"(Reference|Citation).*undefined", log), log[-400:])
    elif bad == "! ":
        check("no TeX errors in the log", not re.search(r"^! ", log, re.M), log[-400:])

outdir = os.path.dirname(pdf)
mfw = os.path.join(outdir, "main.mfw")
lines = open(mfw).read().splitlines() if os.path.isfile(mfw) else []
check("sidecar header", lines[:1] == ["mfw 1"], str(lines[:1]))
rows = [l.split("|") for l in lines[1:] if l.startswith("widget|")]

EXPECT = {  # id: (type, runtime, label)
    "fig-model": ("model", "model@1", "fig:model"),
    "fig-video": ("video", "video@1", "fig:video"),
    "tab-results": ("table", "table@1", "tab:results"),
    "fig-chart": ("chart", "chart@1", "fig:chart"),
    "fig-html": ("html", "", "fig:html"),
    "chart-bare": ("chart", "chart@1", ""),
    "chart-inline": ("chart", "chart@1", ""),
}
check("sidecar lists every widget", {w[1] for w in rows} == set(EXPECT), str([w[1] for w in rows]))
by_id = {w[1]: w for w in rows}
check("every widget type is covered",
      {w[2] for w in rows} == {"model", "video", "table", "chart", "html"}, str({w[2] for w in rows}))
check("every widget has alt text", all(len(w) == 11 and w[10].strip() for w in rows), str([w[-1] for w in rows]))
for wid, (typ, rt, label) in EXPECT.items():
    w = by_id.get(wid)
    check(f"{wid} type, runtime and label",
          bool(w) and w[2] == typ and w[3] == rt and w[4] == label, str(w))
check("uncaptioned and floatless widgets have no figure number",
      all(by_id[i][5] == "" for i in ("chart-bare", "chart-inline")), str([by_id[i] for i in ("chart-bare", "chart-inline")]))
check("captioned widgets carry a figure number",
      all(by_id[i][5] != "" for i in ("fig-model", "fig-video", "fig-chart", "fig-html")), str([by_id[i][5] for i in by_id]))

data = open(pdf, "rb").read()
blobs = [data]
for m in re.finditer(rb"<<(.*?)>>\s*stream\r?\n(.*?)endstream", data, re.S):
    body = m.group(2).rstrip(b"\r\n")
    try:
        blobs.append(zlib.decompress(body))
    except zlib.error:
        blobs.append(body)
blob = b"\n".join(blobs)
marks = re.findall(rb"/NM\s*\(mfw:([^)]+)\)", blob)
check("pdf carries one annotation per widget", sorted(marks) == sorted(i.encode() for i in EXPECT), str(marks))

uris = re.findall(rb"/URI\s*\(https://example\.org/papers/playground\)>>/Rect\[[\d.]+ ([\d.]+) ", blob)
# Footer link rects sit under 40pt (a wrapped url makes one per line); widget links sit above.
check("one text link per widget", len([y for y in uris if float(y) >= 40]) == len(EXPECT), str(uris))
check("the footer link sits at the page foot", any(float(y) < 40 for y in uris), str(uris))

w = call("widgets", {"root_id": "pg", "main_rel": "main.tex"})
check("widgets lists the compiled paper", w["ok"], str(w)[:300])
wl = w.get("widgets") or []
check("widgets returns every widget",
      sorted(x["id"] for x in wl) == sorted(EXPECT), str([x["id"] for x in wl]))
check("widgets covers all five types",
      {x["type"] for x in wl} == {"model", "video", "table", "chart", "html"}, str({x["type"] for x in wl}))
check("widgets follow the pdf, page by page",
      [x["page"] for x in wl] == sorted(x["page"] for x in wl), str([(x["id"], x["page"]) for x in wl]))
check("widget pages and rects are well-formed",
      all(x["page"] >= 1 and x["rect"]["x0"] < x["rect"]["x1"] and x["rect"]["y0"] < x["rect"]["y1"] for x in wl))
wby = {x["id"]: x for x in wl}
check("widget sources and options are typed",
      wby["fig-model"]["sources"] == [{"role": "model", "path": "models/mesh.glb"}]
      and {"key": "pdfrows", "value": "3"} in wby["tab-results"]["options"]
      and wby["fig-html"]["sources"][0]["role"] == "bundle", str(wl)[:300])
check("the html widget declares the origin it wants, nothing else does",
      wby["fig-html"].get("csp", {}).get("connectDomains") == ["https://example.org"]
      and all("csp" not in wby[i] for i in wby if i != "fig-html"), str(wby["fig-html"].get("csp")))
check("uncaptioned and floatless widgets list no label",
      all("label" not in wby[i] for i in ("chart-bare", "chart-inline")), str([wby[i] for i in ("chart-bare", "chart-inline")]))

import shutil, subprocess
if shutil.which("mutool"):
    txt = subprocess.run(["mutool", "draw", "-F", "txt", "-o", "-", pdf], capture_output=True).stdout.decode("utf-8", "replace")
    check("footer shows the bundle url exactly once", txt.count("example.org/papers/playground") == 1, str(txt.count("example.org/papers/playground")))
    check("every widget keeps a one-line mark", txt.count("Interactive version") == len(EXPECT), str(txt.count("Interactive version")))
    check("bibliography resolved in the pdf", "theory of communication" in txt.lower() and "[1]" in txt, txt[-300:])
    check("table keeps pdfrows rows", "ours-base" in txt and "ours-large" not in txt, txt[:200])
    check("subfigure captions print", "First panel" in txt and "Second panel" in txt)
else:
    print("skip: pdf text checks need mutool (absent)")

mcp.p.kill()
sys.exit(1 if fails else 0)
PYEOF
driver_status=$?
[ "$driver_status" -eq 0 ] || fail "playground run failed"

cd "$ROOT"
new="$(git status --porcelain | grep -v -e '^?? main\.\(aux\|bbl\|blg\|log\|out\|pdf\|xdv\|mfw\)$' || true)"
[ "$new" = "?? maleficium-interactive.sty" ] || fail "unexpected project writes: $new"
pass "porcelain shows only the installed package (explicit install)"

echo ""
echo "PLAYGROUND PROOFS COMPLETE: live MCP run green."
echo "  root: $ROOT"
