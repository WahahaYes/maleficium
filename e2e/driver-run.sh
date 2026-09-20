#!/bin/bash
# Driver-driven run over the stdio sidecar. Spawns `maleficium-mcp`, scripts
# grant -> compile -> poll -> synctex -> delete -> undo over JSON-RPC against
# a scratch copy of playground/simple/, then asserts porcelain discipline +
# artifact homes.
set -euo pipefail

DEVROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN="$DEVROOT/src-tauri/target/debug/maleficium-mcp"
FIXTURE="$DEVROOT/playground/simple"
SCRATCH="$(mktemp -d /tmp/maleficium-driver-XXXXXX)"
trap 'rm -rf "$SCRATCH"' EXIT

fail() { echo "FAIL: $1"; exit 1; }
pass() { echo "ok: $1"; }

[ -x "$BIN" ] || fail "sidecar missing: build with cargo build --manifest-path src-tauri/Cargo.toml --bin maleficium-mcp"

if [ -n "${MCP_ROOT_OVERRIDE:-}" ]; then
  # Drive a caller-owned tree in place (e.g. pre-warming the engine
  # cache the app will open). Porcelain baseline is taken as-is.
  ROOT="$MCP_ROOT_OVERRIDE"
  [ -d "$ROOT" ] || fail "override root missing: $ROOT"
  cd "$ROOT"
  if [ ! -d .git ]; then git init -q; git add -A; git -c user.email=driver@local -c user.name=driver commit -qm "fixture"; fi
else
  cp -r "$FIXTURE" "$SCRATCH/proj"
  cd "$SCRATCH/proj"
  git init -q
  git add -A
  git -c user.email=driver@local -c user.name=driver commit -qm "fixture"
  ROOT="$SCRATCH/proj"
fi
porcelain() { git status --porcelain; }
[[ -z "$(porcelain)" ]] || fail "fixture repo not clean at start"

export MCP_BIN="$BIN" MCP_ROOT="$ROOT"
export DRIVER_LOG="${DRIVER_LOG:-/tmp/maleficium-driver-log.jsonl}"
python3 - "$SCRATCH" <<'EOF'
import json, os, subprocess, sys, time
scratch = sys.argv[1]
BIN = os.environ["MCP_BIN"]
ROOT = os.environ["MCP_ROOT"]
LOG = os.environ["DRIVER_LOG"]
WARM_ONLY = os.environ.get("WARM_ONLY") == "1"
ROUNDS = int(os.environ.get("POLL_ROUNDS", "30"))
fails = []
logf = open(LOG, "w")
def check(name, cond, detail="", ms=None):
    print(("ok: " if cond else "FAIL: ") + name + (f" ({detail})" if detail and not cond else ""))
    rec = {"check": name, "pass": bool(cond), "detail": str(detail)[:300]}
    if ms is not None:
        rec["ms"] = round(ms)
    logf.write(json.dumps(rec) + "\n"); logf.flush()
    if not cond:
        fails.append(name)

p = subprocess.Popen([BIN], stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True, bufsize=1)
mid = [0]
def send(method, params=None):
    mid[0] += 1
    msg = {"jsonrpc": "2.0", "id": mid[0], "method": method}
    if params is not None:
        msg["params"] = params
    p.stdin.write(json.dumps(msg) + "\n"); p.stdin.flush()
    return json.loads(p.stdout.readline())
def notify(m):
    p.stdin.write(json.dumps({"jsonrpc": "2.0", "method": m}) + "\n"); p.stdin.flush()
def call(name, args):
    r = send("tools/call", {"name": name, "arguments": args})
    return r["result"]["structuredContent"]

send("initialize", {"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "driver", "version": "0"}})
notify("notifications/initialized")

g = call("grant", {"root_id": "drv", "root": ROOT})
check("grant project root", g["ok"] and g["path"] == ROOT, str(g))

e = call("read", {"root_id": "drv", "rel": "../outside.tex"})
check("escape fails closed", not e["ok"], str(e))

r = call("compile_run", {"root_id": "drv", "rel": "main.tex"})
job = (r.get("job_id") or "")
check("compile starts", r["ok"] and bool(job), str(r))
t0 = time.time()
polls = 0
sc = {"status": "running"}
for _ in range(ROUNDS):
    time.sleep(4)
    polls += 1
    sc = call("compile_poll", {"job_id": job, "tail_lines": 3})
    if sc["status"] != "running":
        break
compile_ms = (time.time() - t0) * 1000
check("compile succeeds", sc["status"] == "success", str(sc)[:200], ms=compile_ms)
check("compile polls bounded", polls < ROUNDS, f"{polls} polls")
if WARM_ONLY:
    logf.close()
    p.kill()
    sys.exit(0)
pdf = sc.get("pdf_path") or ""
check("pdf outside project", pdf and not pdf.startswith(ROOT), pdf)
import os as _os
check("pdf exists", bool(pdf) and _os.path.exists(pdf), pdf)

fw = call("synctex_forward", {"root_id": "drv", "pdf_rel": pdf, "tex_rel": ROOT + "/main.tex", "line": 10})
check("forward query", fw["ok"] and "Page:" in fw["text"], fw["text"][:120])

outdir = pdf[: pdf.rfind("/")]
iv = call("synctex_inverse", {"root_id": "drv", "dir_rel": outdir, "pdf_name": "main.pdf", "page": 1, "x": 100, "y": 600})
check("inverse query", iv["ok"] and "Line:" in iv["text"], iv["text"][:120])

d0 = call("delete", {"root_id": "drv", "rel": "chapters/method.tex"})
check("delete needs confirm", not d0["ok"] and "confirm" in (d0.get("error") or ""), str(d0))
abs_target = ROOT + "/chapters/method.tex"
d1 = call("delete", {"root_id": "drv", "rel": "chapters/method.tex", "confirm": abs_target})
trash = d1.get("trash_path") or ""
check("delete trashes", d1["ok"] and bool(trash), str(d1))
check("trash outside project", trash and not trash.startswith(ROOT), trash)

u = call("undo", {"root_id": "drv", "trash_path": trash})
check("undo restores", u["ok"] and u.get("path", "").endswith("chapters/method.tex"), str(u))

with open(ROOT + "/fail.tex", "w") as f:
    f.write("\\documentclass{article}\n\\begin{document}\n\\badcommand\n\\end{document}\n")
fr = call("compile_run", {"root_id": "drv", "rel": "fail.tex"})
fjob = (fr.get("job_id") or "")
check("failure compile starts", fr["ok"] and bool(fjob), str(fr))
ft0 = time.time()
fsc = {"status": "running"}
for _ in range(30):
    time.sleep(4)
    fsc = call("compile_poll", {"job_id": fjob, "tail_lines": 5})
    if fsc["status"] != "running":
        break
fail_ms = (time.time() - ft0) * 1000
flog = (fsc.get("log") or "") + "\n" + "\n".join(fsc.get("lines") or [])
check("failure reports failed", fsc.get("status") == "failed", str(fsc)[:200], ms=fail_ms)
check("failure names the cause", "Undefined control sequence" in flog, flog[:200])
check("failure writes no pdf", not fsc.get("pdf_path"), str(fsc.get("pdf_path")))
_os.remove(ROOT + "/fail.tex")

p.kill()
sys.exit(1 if fails else 0)
EOF
driver_status=$?
[ "$driver_status" -eq 0 ] || fail "driver run failed"

[[ -z "$(porcelain)" ]] || fail "project dirty after driver round-trip: $(porcelain)"
pass "porcelain clean after grant/compile/delete/undo"
[ -f "$ROOT/chapters/method.tex" ] || fail "method.tex missing after undo"
pass "delete-undo round-trips through app-data"

echo ""
echo "DRIVER PROOFS COMPLETE: live MCP run green."
echo "  root: $ROOT"
