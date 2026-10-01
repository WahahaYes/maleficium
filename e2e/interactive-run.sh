#!/bin/bash
# Interactive-widgets run over the stdio sidecar. Spawns `maleficium-mcp`,
# scripts grant -> interactive_install -> compile -> poll against a scratch
# copy of e2e/fixtures/interactive/, then asserts the sidecar, the named
# PDF annotations, the table row cap, porcelain discipline, and the four
# failing documents.
set -euo pipefail

DEVROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN="${CARGO_TARGET_DIR:-$DEVROOT/src-tauri/target}/debug/maleficium-mcp"
FIXTURE="$DEVROOT/e2e/fixtures/interactive"
SCRATCH="$(mktemp -d /tmp/maleficium-interactive-XXXXXX)"
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
  ROUNDS=40
else
  ROUNDS=120
fi
export POLL_ROUNDS="$ROUNDS"

python3 - "$SCRATCH" <<'EOF'
import json, os, re, subprocess, sys, time, zlib
scratch = sys.argv[1]
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

mcp = McpClient([BIN], "interactive")
def call(name, args):
    return flat(mcp.tool(name, args))

def compile(rel, rounds=ROUNDS):
    r = call("compile_run", {"root_id": "ip", "rel": rel})
    job = r.get("job_id") or ""
    if not (r["ok"] and job):
        return {"status": "not-started", "log": str(r)}
    rec = {"status": "running"}
    for _ in range(rounds):
        time.sleep(3)
        rec = call("compile_poll", {"job_id": job, "tail_lines": 5})
        if rec.get("status") != "running":
            break
    return rec

g = call("grant", {"root_id": "ip", "root": ROOT})
check("grant project root", g["ok"] and g["path"] == ROOT, str(g))

r = call("interactive_install", {"root_id": "ip"})
check("install writes the package", r["ok"] and r.get("file") == "maleficium-interactive.sty", str(r))
sty = os.path.join(ROOT, "maleficium-interactive.sty")
check("installed bytes match the embedded source",
      open(sty, "rb").read() == open(os.path.join(os.environ["DEVROOT"], "src-tauri/interactive/maleficium-interactive.sty"), "rb").read())
bad = call("interactive_install", {"root_id": "nope"})
check("install refuses unknown roots", not bad["ok"], str(bad))

rec = compile("main.tex")
check("fixture compiles", rec.get("status") == "success", str(rec)[:300])
pdf = (rec.get("pdf_url") or "")
check("compile reports the pdf", bool(pdf), str(rec)[:200])
outdir = os.path.dirname(pdf)
mfw = os.path.join(outdir, "main.mfw")
lines = open(mfw).read().splitlines() if os.path.isfile(mfw) else []
check("sidecar header", lines[:1] == ["mfw 1"], str(lines[:1]))
widgets = [l.split("|") for l in lines[1:] if l.startswith("widget|")]
check("sidecar lists five widgets", len(widgets) == 5, str(len(widgets)))
check("sidecar fields are complete", all(len(w) == 11 for w in widgets), str([len(w) for w in widgets]))
ids = {w[1] for w in widgets}
check("sidecar widget ids", ids == {"fig-mesh", "fig-clip", "tab-results", "fig-chart", "fig-demo"}, str(ids))
check("sidecar runtimes", {w[3] for w in widgets} == {"model@1", "video@1", "table@1", "chart@1", ""}, str({w[3] for w in widgets}))
check("sidecar alts are non-empty", all(w[10].strip() for w in widgets), str([w[10] for w in widgets]))
by_id = {w[1]: w for w in widgets}
check("post-caption float captures label and figure",
      by_id["fig-mesh"][4] == "fig:mesh" and by_id["fig-mesh"][5] == "1", str(by_id["fig-mesh"][4:6]))
check("pre-caption float stays honest",
      by_id["fig-clip"][4] == "" and by_id["fig-clip"][5] == "", str(by_id["fig-clip"][4:6]))

def streams(pdf_path):
    data = open(pdf_path, "rb").read()
    blobs = [data]
    for m in re.finditer(rb"<<(.*?)>>\s*stream\r?\n(.*?)endstream", data, re.S):
        body = m.group(2).rstrip(b"\r\n")
        try:
            blobs.append(zlib.decompress(body))
        except zlib.error:
            blobs.append(body)
    return b"\n".join(blobs)

blob = streams(pdf)
marks = re.findall(rb"/NM\s*\(mfw:([^)]+)\)", blob)
check("pdf carries five named annotations", sorted(marks) == sorted([b"fig-mesh", b"fig-clip", b"tab-results", b"fig-chart", b"fig-demo"]), str(marks))
rects = re.findall(rb"/Rect\s*\[([\d.]+) ([\d.]+) ([\d.]+) ([\d.]+)\]", blob)
check("annotation rects are well-formed", len(rects) >= 5 and all(float(a) < float(c) and float(b) < float(d) for a, b, c, d in rects), str(rects[:6]))
import shutil
if shutil.which("mutool"):
    txt = subprocess.run(["mutool", "draw", "-F", "txt", "-o", "-", pdf],
                         capture_output=True).stdout.decode("utf-8", "replace")
    check("table keeps the header and two rows",
          "alpha" in txt and "beta" in txt, txt[:200])
    check("table drops rows past pdfrows",
          "gamma" not in txt and "delta" not in txt, txt[:200])
else:
    print("skip: table row text needs mutool draw (absent)")

for bad_doc, want in [("bad-duplicate.tex", "Duplicate widget id"),
                      ("bad-id.tex", "must match [a-z0-9]"),
                      ("bad-missing.tex", "not found"),
                      ("bad-noalt.tex", "alt= is required")]:
    fr = compile(bad_doc, rounds=15)
    flog = (fr.get("log") or "") + "\n" + "\n".join(fr.get("lines") or [])
    check(f"{bad_doc} fails", fr.get("status") == "failed", str(fr)[:200])
    check(f"{bad_doc} names the cause", want in flog, flog[:300])
    check(f"{bad_doc} writes no pdf", not fr.get("pdf_url"), str(fr.get("pdf_url")))

pr = compile("plain.tex", rounds=20)
check("package-free document compiles", pr.get("status") == "success", str(pr)[:200])

mcp.p.kill()
sys.exit(1 if fails else 0)
EOF
driver_status=$?
[ "$driver_status" -eq 0 ] || fail "interactive run failed"

cd "$ROOT"
new="$(git status --porcelain)"
[ "$new" = "?? maleficium-interactive.sty" ] || fail "unexpected project writes: $new"
pass "porcelain shows only the installed package (explicit install)"

echo ""
echo "INTERACTIVE PROOFS COMPLETE: live MCP run green."
echo "  root: $ROOT"
