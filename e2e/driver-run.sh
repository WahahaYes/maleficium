#!/bin/bash
# Driver-driven run over the stdio sidecar. Spawns `maleficium-mcp`, scripts
# grant -> compile -> poll -> synctex -> delete -> undo over JSON-RPC against
# a scratch copy of playground/simple/, then asserts porcelain discipline +
# artifact homes.
set -euo pipefail

DEVROOT="$(cd "$(dirname "$0")/.." && pwd)"
# Honor a shared target dir (e.g. worktree runs reuse the main checkout's
# build cache via CARGO_TARGET_DIR): the binary lives where cargo put it.
BIN="${CARGO_TARGET_DIR:-$DEVROOT/src-tauri/target}/debug/maleficium-mcp"
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
export DEVROOT

# The app cache home holds the engine cache (TECTONIC_CACHE_DIR) and the
# output shards: keep it in OS tmp, reused across runs so only the first run
# pays the cold bundle download.
export XDG_CACHE_HOME="${DRIVER_CACHE:-${TMPDIR:-/tmp}/maleficium-driver-cache-$(id -u)}"
ENGINE_RS="$DEVROOT/src-tauri/src/core/engine.rs"
BUNDLE_DIGEST="$(sed -n 's/^pub const BUNDLE_DIGEST: &str = "\([0-9a-f]*\)";/\1/p' "$ENGINE_RS")"
[ -n "$BUNDLE_DIGEST" ] || fail "pinned bundle digest not found in $ENGINE_RS"
export ENGINE_CACHE="$XDG_CACHE_HOME/com.ethan.tauri-app/maleficium-tectonic/$BUNDLE_DIGEST"
export BUNDLE_DIGEST

# Heavy fixtures are generated megabytes: they live only in OS tmp (RULES §4)
# and are rebuilt from src/test/fixtures.ts whenever they are absent.
FIXTURES="${DRIVER_FIXTURES:-${TMPDIR:-/tmp}/maleficium-driver-fixtures-$(id -u)}"
PAGES_ROOT="$FIXTURES/pages"
TREE_ROOT="$FIXTURES/tree"
export PAGES_ROOT TREE_ROOT
if [ ! -f "$PAGES_ROOT/doc.tex" ] || [ ! -f "$TREE_ROOT/main.tex" ]; then
  echo "generating heavy fixtures under $FIXTURES (3000pp + 1000 files)"
  GEN="$SCRATCH/gen"
  mkdir -p "$GEN"
  (cd "$DEVROOT" && ./node_modules/.bin/tsc --ignoreConfig src/test/make-fixture.ts \
    --outDir "$GEN" --module commonjs --target es2022 --skipLibCheck \
    --esModuleInterop --types node)
  node "$GEN/make-fixture.js" pages 3000 "$PAGES_ROOT" >/dev/null
  node "$GEN/make-fixture.js" flat 1000 "$TREE_ROOT" >/dev/null
fi

# D.5 budget probe: replicates Preview.tsx's load + render timings headlessly
# (same pdf.js build, same viewport formula) and reports one JSON line.
PDF_PROBE="$SCRATCH/pdf-probe.mjs"
export PDF_PROBE
cat > "$PDF_PROBE" <<'PROBE'
const [devroot, pdfPath] = process.argv.slice(2);
const { readFileSync } = await import('node:fs');
// @napi-rs/canvas is an OPTIONAL dependency of pdfjs-dist: present on a full
// npm install, absent with --no-optional or on platforms without a prebuilt
// binary. Without it pdfjs cannot even polyfill DOMMatrix headlessly, so both
// imports live inside the skip guard: skip with a stated reason instead of
// failing the whole driver run.
let pdfjs, createCanvas;
try {
  pdfjs = await import(`file://${devroot}/node_modules/pdfjs-dist/legacy/build/pdf.mjs`);
  ({ createCanvas } = await import(`file://${devroot}/node_modules/@napi-rs/canvas/index.js`));
} catch (e) {
  process.stdout.write(
    JSON.stringify({
      skipped:
        'D.5 render budgets need pdfjs-dist with its @napi-rs/canvas optional dependency (' +
        String((e && e.message) || e).slice(0, 160) +
        ')',
    }) + '\n',
  );
  process.exit(0);
}
pdfjs.GlobalWorkerOptions.workerSrc = `${devroot}/node_modules/pdfjs-dist/legacy/build/pdf.worker.mjs`;

// Load window matches the `pdf loaded N pages in Xms` stream line: bytes in,
// document open, page count known.
const t0 = Date.now();
const data = new Uint8Array(readFileSync(pdfPath));
const doc = await pdfjs.getDocument({
  data,
  useSystemFonts: true,
  standardFontDataUrl: `${devroot}/node_modules/pdfjs-dist/standard_fonts/`,
}).promise;
const loadMs = Date.now() - t0;

// Render window matches `page N rendered in Xms`: raster plus text layer, at
// the shell-width scale the preview uses (dpr folded in, 1 backing px per CSS px).
const CSS_WIDTH = 820;
async function renderPage(n) {
  const page = await doc.getPage(n);
  const probe = page.getViewport({ scale: 1 });
  const viewport = page.getViewport({ scale: CSS_WIDTH / Math.max(1, probe.width) });
  const canvas = createCanvas(Math.floor(viewport.width), Math.floor(viewport.height));
  const ctx = canvas.getContext('2d');
  const t1 = Date.now();
  await page.render({ canvasContext: ctx, canvas, viewport }).promise;
  await page.getTextContent();
  return Date.now() - t1;
}
const firstMs = await renderPage(1);
const lastMs = await renderPage(doc.numPages);

process.stdout.write(
  JSON.stringify({
    pages: doc.numPages,
    load_ms: loadMs,
    render_first_ms: firstMs,
    render_last_ms: lastMs,
    rss_mb: Math.round(process.resourceUsage().maxRSS / 1024),
  }) + '\n',
);
PROBE

export DRIVER_LOG="${DRIVER_LOG:-/tmp/maleficium-driver-log.jsonl}"
python3 - "$SCRATCH" <<'EOF'
import json, os, subprocess, sys, tempfile, time
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
    # Tool failures are isError results with the reason as text; successes
    # carry the record as structuredContent. Flatten both into one dict.
    r = send("tools/call", {"name": name, "arguments": args})["result"]
    if r.get("isError"):
        return {"ok": False, "error": "".join(c.get("text", "") for c in r.get("content") or [])}
    return {"ok": True, **r["structuredContent"]}

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
    if sc.get("status") != "running":
        break
compile_ms = (time.time() - t0) * 1000
check("compile succeeds", sc.get("status") == "success", str(sc)[:200], ms=compile_ms)
check("compile polls bounded", polls < ROUNDS, f"{polls} polls")
if WARM_ONLY:
    logf.close()
    p.kill()
    sys.exit(0)
EC = os.environ["ENGINE_CACHE"]
pinned = os.path.join(EC, "bundles", "hashes", "https,58,,47,,47,data1b.fullyjustified.net,47,tlextras-2022.0r0.tar")
check("engine cache is app-owned under OS tmp", EC.startswith(tempfile.gettempdir()) and os.path.isdir(EC), EC)
check("compile resolved the pinned bundle", os.path.isfile(pinned) and open(pinned).read().strip() == os.environ["BUNDLE_DIGEST"], pinned)
pdf = sc.get("pdf_url") or ""
check("pdf outside project", pdf and not pdf.startswith(ROOT), pdf)
import os as _os
check("pdf exists", bool(pdf) and _os.path.exists(pdf), pdf)

fw = call("synctex_forward", {"root_id": "drv", "main_rel": "main.tex", "tex_rel": "main.tex", "line": 10})
check("forward query", fw["ok"] and (fw.get("page") or 0) >= 1, str(fw)[:120])

iv = call("synctex_inverse", {"root_id": "drv", "main_rel": "main.tex", "page": 1, "x": 100, "y": 600})
ivh = iv
check("inverse query", iv["ok"] and (ivh.get("line") or 0) >= 1, str(iv)[:120])
check("inverse hit is root-relative", bool(ivh.get("relPath")) and not ivh["relPath"].startswith("/"), str(ivh))

st = call("file_graph", {"root_id": "drv", "main_rel": "main.tex"})
st_files = {f["rel"]: f["exists"] for f in (st.get("files") or [])}
check("file graph walks inputs", st["ok"] and st_files.get("chapters/method.tex") is True and st_files.get("chapters/background.tex") is True, str(st)[:200])
check("structure is disk-sourced + revisioned", st.get("source") == "disk" and len(st.get("revision") or "") == 16, str(st)[:120])
check("structure paths are root-relative", ROOT not in json.dumps(st), json.dumps(st)[:200])
ol = call("outline", {"root_id": "drv", "rel": "main.tex"})
check("outline lists sections", ol["ok"] and any(e["kind"] == "section" and e["title"] == "Introduction" for e in ol.get("entries") or []), str(ol)[:200])
lrf = call("labels_refs", {"root_id": "drv", "main_rel": "main.tex"})
check("refs resolve", lrf["ok"] and any(r["key"] == "fig:diagram" and r["resolved"] for r in lrf.get("refs") or []), str(lrf)[:200])
ci = call("citations", {"root_id": "drv", "main_rel": "main.tex"})
check("cites resolve against refs.bib", ci["ok"] and any(c["key"] == "knuth1984texbook" and c["resolved"] for c in ci.get("cites") or []), str(ci)[:200])
dg = call("diagnostics", {"root_id": "drv", "main_rel": "main.tex"})
check("diagnostics after a clean compile", dg["ok"] and ROOT not in json.dumps(dg), str(dg)[:200])
esc = call("outline", {"root_id": "drv", "rel": "../outside.tex"})
check("structure escape is a tool error", not esc["ok"] and "forbidden" in esc.get("error", ""), str(esc))

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
    if fsc.get("status") != "running":
        break
fail_ms = (time.time() - ft0) * 1000
flog = (fsc.get("log") or "") + "\n" + "\n".join(fsc.get("lines") or [])
check("failure reports failed", fsc.get("status") == "failed", str(fsc)[:200], ms=fail_ms)
check("failure names the cause", "Undefined control sequence" in flog, flog[:200])
check("failure writes no pdf", not fsc.get("pdf_url"), str(fsc.get("pdf_url")))
fd = call("diagnostics", {"root_id": "drv", "main_rel": "fail.tex"})
fdd = fd.get("diagnostics") or []
check("failure diagnostics are structured", fd["ok"] and any(d.get("path") == "fail.tex" and d["line"] == 3 and d["severity"] == "error" for d in fdd), str(fd)[:300])
_os.remove(ROOT + "/fail.tex")

# ---- heavy-document probes: 1000-file open, cancel mid-compile, D.5 budgets ----
stream = []
def record(line):
    stream.append(line)
    logf.write(json.dumps({"stream": line}) + "\n"); logf.flush()

PAGES_ROOT = os.environ["PAGES_ROOT"]
TREE_ROOT = os.environ["TREE_ROOT"]

gt = call("grant", {"root_id": "tree", "root": TREE_ROOT})
check("grant 1000-file root", gt["ok"], str(gt))
t = time.time()
lr = call("list", {"root_id": "tree", "rel": "."})
open_ms = (time.time() - t) * 1000
names = [e["name"] for e in (lr.get("entries") or [])]
check("1000-file open lists the whole level", lr["ok"] and len(names) == 1003, f"{len(names)} entries", ms=open_ms)
check("1000-file open stays depth 1", "deep" in names and "d1.tex" not in names, ",".join(names[:5]))
check("1000-file open under 750ms", open_ms < 750, f"{open_ms:.0f}ms", ms=open_ms)
t = time.time()
ls = call("list", {"root_id": "tree", "rel": "small"})
small_ms = (time.time() - t) * 1000
check(
    "open cost tracks the listed level, not the tree",
    ls["ok"] and len(ls.get("entries") or []) == 10 and small_ms <= open_ms + 5,
    f"10-entry {small_ms:.1f}ms vs 1003-entry {open_ms:.1f}ms",
    ms=small_ms,
)

gp = call("grant", {"root_id": "pages", "root": PAGES_ROOT})
check("grant 3000pp root", gp["ok"], str(gp))
cr = call("compile_run", {"root_id": "pages", "rel": "doc.tex"})
cjob = cr.get("job_id") or ""
check("cancel probe compile starts", cr["ok"] and bool(cjob), str(cr))
cancel_t0 = time.time()
time.sleep(0.3)
cs = call("compile_poll", {"job_id": cjob, "tail_lines": 3})
check("cancel probe finds the engine still running", cs.get("status") == "running", str(cs)[:200])
# cancel only succeeds while the job still owns a live child, so an accepted
# cancel is itself proof the engine was mid-run.
cc = call("compile_cancel", {"job_id": cjob})
check("cancel accepted mid-compile", cc["ok"], str(cc))
for _ in range(100):
    time.sleep(0.1)
    cs = call("compile_poll", {"job_id": cjob, "tail_lines": 5})
    if cs.get("status") != "running":
        break
cancel_ms = (time.time() - cancel_t0) * 1000
check("cancelled compile reports cancelled", cs.get("status") == "cancelled", str(cs)[:200], ms=cancel_ms)
check("cancelled compile writes no pdf", not cs.get("pdf_url"), str(cs.get("pdf_url")))

hr = call("compile_run", {"root_id": "pages", "rel": "doc.tex"})
hjob = hr.get("job_id") or ""
check("3000pp compile starts", hr["ok"] and bool(hjob), str(hr))
ht0 = time.time()
hs = {"status": "running"}
for _ in range(180):
    time.sleep(1)
    hs = call("compile_poll", {"job_id": hjob, "tail_lines": 5})
    if hs.get("status") != "running":
        break
heavy_ms = (time.time() - ht0) * 1000
check("3000pp compile succeeds", hs.get("status") == "success", str(hs)[:200], ms=heavy_ms)
check(
    "cancel returned long before the compile could finish",
    cancel_ms * 2 < heavy_ms,
    f"cancelled in {cancel_ms:.0f}ms vs {heavy_ms:.0f}ms full compile",
)
record(f"cancel mid-compile: settled cancelled in {cancel_ms:.0f}ms; same document compiles in {heavy_ms:.0f}ms")
record(f"1000-file open in {open_ms:.0f}ms ({len(names)} entries, depth 1); 10-entry level {small_ms:.1f}ms")

heavy_pdf = hs.get("pdf_url") or ""
check("3000pp pdf exists outside the project", bool(heavy_pdf) and _os.path.exists(heavy_pdf) and not heavy_pdf.startswith(PAGES_ROOT), heavy_pdf)
probe = subprocess.run(
    ["node", os.environ["PDF_PROBE"], os.environ["DEVROOT"], heavy_pdf],
    capture_output=True, text=True, timeout=600,
)
d5 = {}
d5_skipped = ""
if probe.returncode == 0 and probe.stdout.strip():
    out = json.loads(probe.stdout.strip().splitlines()[-1])
    if out.get("skipped"):
        d5_skipped = out["skipped"]
    else:
        d5 = out
if d5_skipped:
    print("skip: D.5 probe (" + d5_skipped + ")")
    record("D.5 probe SKIPPED: " + d5_skipped)
else:
    check("D.5 probe runs", bool(d5), (probe.stderr or "no output")[-300:])
if d5:
    pages = d5["pages"]
    check("3000pp fixture really is 3000 pages", pages >= 3000, str(pages))
    check("pdf load within the 1313ms baseline", d5["load_ms"] <= 1313, f"{d5['load_ms']}ms", ms=d5["load_ms"])
    check("far-page render within the 3806ms baseline", d5["render_last_ms"] <= 3806, f"{d5['render_last_ms']}ms", ms=d5["render_last_ms"])
    check("first-page render within the 3806ms baseline", d5["render_first_ms"] <= 3806, f"{d5['render_first_ms']}ms", ms=d5["render_first_ms"])
    check("pdf memory within the 294MB baseline", d5["rss_mb"] <= 294, f"{d5['rss_mb']}MB")
    record(f"pdf loaded {pages} pages in {d5['load_ms']}ms (baseline 1313ms)")
    record(f"page 1 rendered in {d5['render_first_ms']}ms (baseline 3806ms)")
    record(f"page {pages} rendered in {d5['render_last_ms']}ms (baseline 3806ms)")
    record(f"pager turn 1 -> {pages} costs {d5['render_last_ms']}ms")
    record(f"peak rss {d5['rss_mb']}MB over load + render (baseline 294MB)")

print("")
print("D.5 BUDGETS + PROBES (observed vs recorded baseline):")
for line in stream:
    print("  stream: " + line)

logf.close()
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
