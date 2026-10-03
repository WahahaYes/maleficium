#!/bin/bash
# Paper-bundle export run over the stdio sidecar. Spawns `maleficium-mcp`,
# compiles a scratch copy of e2e/fixtures/interactive/, then exports it in
# every profile and checks the result against the committed manifest schema:
# layout, sha256 of every asset and of the pdf, one inline document per
# widget, destination-inside-the-project refusal, the single-file size cap,
# a remote-only asset refused (and not approvable over MCP), and that the
# export itself runs with no network at all (an empty network namespace).
set -euo pipefail

DEVROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN="${CARGO_TARGET_DIR:-$DEVROOT/src-tauri/target}/debug/maleficium-mcp"
FIXTURE="$DEVROOT/e2e/fixtures/interactive"
SCRATCH="$(mktemp -d /tmp/maleficium-export-XXXXXX)"
trap 'rm -rf "$SCRATCH"' EXIT

fail() { echo "FAIL: $1"; exit 1; }
pass() { echo "ok: $1"; }

cargo build -q --manifest-path "$DEVROOT/src-tauri/Cargo.toml" --bin maleficium-mcp \
  || fail "cannot build maleficium-mcp"
[ -x "$BIN" ] || fail "sidecar missing after build: $BIN"
python3 -c "import jsonschema" 2>/dev/null || fail "python3 jsonschema is required"

cp -r "$FIXTURE" "$SCRATCH/proj"
cd "$SCRATCH/proj"
git init -q
git add -A
git -c user.email=driver@local -c user.name=driver commit -qm "fixture"
ROOT="$SCRATCH/proj"

# A variant whose only video has no local file, only a remote url: hashing it
# would need a download.
cat > "$ROOT/remote.tex" <<'EOF'
\documentclass{article}
\usepackage{maleficium-interactive}
\begin{document}
\begin{figure}
\interactivevideo[poster=figures/clip.png, height=5cm, id=fig-remote, alt={Remote clip}, remote=https://media.example.org/clip.mp4]{media/absent.mp4}
\caption{A remote video.}
\end{figure}
\end{document}
EOF
git add remote.tex
git -c user.email=driver@local -c user.name=driver commit -qm "remote variant"

export MCP_BIN="$BIN" MCP_ROOT="$ROOT"
export DEVROOT SCRATCH
# Reuse the warm engine cache: the first cold download is minutes.
export XDG_CACHE_HOME="${DRIVER_CACHE:-$HOME/.cache}"
export XDG_DATA_HOME="$SCRATCH/data"
ENGINE_RS="$DEVROOT/src-tauri/core/src/engine.rs"
BUNDLE_DIGEST="$(sed -n 's/^pub const BUNDLE_DIGEST: &str = "\([0-9a-f]*\)";/\1/p' "$ENGINE_RS")"
[ -n "$BUNDLE_DIGEST" ] || fail "pinned bundle digest not found in $ENGINE_RS"
if [ -f "$XDG_CACHE_HOME/io.github.wahahayes.maleficium/maleficium-tectonic/$BUNDLE_DIGEST/bundles/data/$BUNDLE_DIGEST.index" ]; then
  ROUNDS=40
else
  ROUNDS=120
fi
export POLL_ROUNDS="$ROUNDS"

# The export phase runs in a process with no network namespace access.
if unshare -rn true 2>/dev/null; then
  export NONET=1
else
  export NONET=0
fi

python3 - <<'EOF'
import base64, hashlib, json, os, re, subprocess, sys, time
import jsonschema
scratch = os.environ["SCRATCH"]
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

def sha(b): return hashlib.sha256(b).hexdigest()

schema_path = os.path.join(os.environ["DEVROOT"], "src-tauri/core/schemas/paper-bundle-1.schema.json")
schema = json.load(open(schema_path))
validator = jsonschema.Draft202012Validator(schema)
def schema_errors(m):
    return [f"{'/'.join(map(str, e.absolute_path))}: {e.message}" for e in validator.iter_errors(m)]

mcp = McpClient([BIN], "export")
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

check("the committed schema is itself a valid 2020-12 schema",
      (jsonschema.Draft202012Validator.check_schema(schema) or True))
g = call("grant", {"root_id": "ip", "root": ROOT})
check("grant project root", g["ok"] and g["path"] == ROOT, str(g))
r = call("interactive_install", {"root_id": "ip"})
check("install writes the package", r["ok"], str(r))

# Before any compile: an error, not an empty bundle.
out0 = os.path.join(scratch, "out0")
early = call("export_bundle", {"root_id": "ip", "main_rel": "main.tex", "dest": out0, "profile": "folder"})
check("export before any compile is refused", not early["ok"] and "compile" in early["error"], str(early)[:200])
check("...and writes nothing", not os.path.exists(out0))

rec = compile("main.tex")
check("fixture compiles", rec.get("status") == "success", str(rec)[:300])
pdf_path = rec.get("pdf_url") or ""
pdf_bytes = open(pdf_path, "rb").read() if pdf_path else b""
check("compile reports the pdf", len(pdf_bytes) > 0)

OUT = os.path.join(scratch, "out")
os.makedirs(OUT)
exp = {}

def export(profile, name, extra=None, main="main.tex"):
    args = {"root_id": "ip", "main_rel": main, "dest": os.path.join(OUT, name), "profile": profile}
    args.update(extra or {})
    return call("export_bundle", args)

def manifest_of(profile, dest):
    if profile == "single-file":
        html = open(dest, encoding="utf-8").read()
        return json.loads(re.search(r'<script type="application/json" id="mfw-manifest">(.*?)</script>', html, re.S).group(1)), html
    return json.load(open(os.path.join(dest, "manifest.json"))), None

def asset_bytes(profile, dest, m, a, html):
    if a["mode"] == "bundled":
        return open(os.path.join(dest, a["path"]), "rb").read()
    blobs = json.loads(re.search(r'<script type="application/json" id="mfw-assets">(.*?)</script>', html, re.S).group(1))
    return base64.b64decode(blobs[a["sha256"]])

WIDGETS = ["fig-mesh", "tab-results", "fig-chart", "fig-clip", "fig-demo"]
for profile, name in [("folder", "folder"), ("single-file", "single.html"), ("hosted", "hosted")]:
    dest = os.path.join(OUT, name)
    r = export(profile, name)
    check(f"{profile}: export succeeds", r["ok"], str(r)[:300])
    if not r["ok"]:
        continue
    exp[profile] = r
    check(f"{profile}: result names the path and five widgets", r["path"] == dest and r["widgets"] == 5 and r["profile"] == profile, str(r)[:200])
    m, html = manifest_of(profile, dest)
    errs = schema_errors(m)
    check(f"{profile}: manifest validates against the committed schema", not errs, "; ".join(errs[:3]))
    check(f"{profile}: widgets are in document order", [w["id"] for w in m["widgets"]] == WIDGETS, str([w["id"] for w in m["widgets"]]))
    check(f"{profile}: pdf sha256 is the compiled pdf's", m["pdf"]["sha256"] == sha(pdf_bytes) and m["pdf"]["bytes"] == len(pdf_bytes))
    bad = []
    for key, a in m["assets"].items():
        b = asset_bytes(profile, dest, m, a, html)
        if sha(b) != a["sha256"] or len(b) != a["bytes"]:
            bad.append(key)
        if a.get("source"):
            src = open(os.path.join(ROOT, a["source"]), "rb").read()
            if sha(src) != a["sha256"]:
                bad.append(key + "(source)")
        if a["mode"] == "bundled" and a["path"] != f"assets/{a['sha256']}.{a['path'].rsplit('.', 1)[1]}":
            bad.append(key + "(name)")
    check(f"{profile}: every asset's bytes hash to its sha256 and its source", not bad, str(bad))
    if profile == "single-file":
        check("single-file: one file, no staging remnant", sorted(os.listdir(OUT)).count("single.html") == 1 and not any(n.startswith(".") for n in os.listdir(OUT)))
        check("single-file: every asset inline, no entry, pdf inlined",
              all(a["mode"] == "inline" for a in m["assets"].values()) and m["pdf"]["path"] is None
              and all("entry" not in w for w in m["widgets"]))
        pdf_b64 = re.search(r'id="pdf-link" href="data:application/pdf;base64,([^"]*)"', html).group(1)
        check("single-file: embedded pdf (the reader's download link) is the compiled pdf", base64.b64decode(pdf_b64) == pdf_bytes)
        docs = json.loads(re.search(r'<script type="application/json" id="mfw-widgets">(.*?)</script>', html, re.S).group(1))
        check("single-file: one document per widget", sorted(docs) == sorted(WIDGETS))
    else:
        check(f"{profile}: paper.pdf is byte-identical to the compiled pdf", open(os.path.join(dest, "paper.pdf"), "rb").read() == pdf_bytes)
        check(f"{profile}: layout", all(os.path.isfile(os.path.join(dest, f)) for f in
              ["manifest.json", "index.html", "paper.pdf", "theme/theme.json", "theme/theme.css"] + [f"widgets/{w}/index.html" for w in WIDGETS]))
        docs = {w: open(os.path.join(dest, f"widgets/{w}/index.html"), encoding="utf-8").read() for w in WIDGETS}
    # Every widget is one inline document with the strict policy first.
    def first_is_policy(doc):
        at = doc.find('<meta http-equiv="Content-Security-Policy"')
        before = re.sub(r"(?i)<!doctype[^>]*>|<html[^>]*>|<head[^>]*>", "", doc[:at]).strip() if at >= 0 else "x"
        return at >= 0 and before == ""
    check(f"{profile}: every widget document carries its policy first", all(first_is_policy(d) for d in docs.values()))
    check(f"{profile}: widget documents load nothing and need no 'self'",
          all("'self'" not in d and "connect-src 'none'" in d and not re.search(r'<script[^>]*\ssrc=', d) for d in docs.values()))

# Destination inside the project: refused, for every profile, nothing written.
before = subprocess.run(["git", "status", "--porcelain"], cwd=ROOT, capture_output=True, text=True).stdout
for profile in ["folder", "single-file", "hosted"]:
    for d in [os.path.join(ROOT, "export-here"), ROOT, os.path.join(ROOT, "figures", "x.html")]:
        r = call("export_bundle", {"root_id": "ip", "main_rel": "main.tex", "dest": d, "profile": profile})
        check(f"{profile}: destination {os.path.relpath(d, ROOT)} inside the project is refused",
              not r["ok"] and "inside the project" in r["error"], str(r)[:200])
check("no export folder appeared in the project", not os.path.exists(os.path.join(ROOT, "export-here")))
rel = call("export_bundle", {"root_id": "ip", "main_rel": "main.tex", "dest": "out/rel", "profile": "folder"})
check("a relative destination is refused", not rel["ok"] and "absolute" in rel["error"], str(rel)[:200])
unknown = call("export_bundle", {"root_id": "nope", "main_rel": "main.tex", "dest": os.path.join(OUT, "x"), "profile": "folder"})
check("an ungranted root is refused", not unknown["ok"], str(unknown)[:200])

# Size cap: a small configured cap, then the real 50 MiB default.
r = export("single-file", "cap-small.html", {"size_cap_bytes": 1000})
check("single-file over a small cap warns and still writes",
      r["ok"] and any(w["kind"] == "size-cap" for w in r["warnings"]) and os.path.isfile(os.path.join(OUT, "cap-small.html")), str(r)[:300])
check("single-file under the default cap does not warn", not any(w["kind"] == "size-cap" for w in exp["single-file"]["warnings"]), str(exp["single-file"]["warnings"]))
def widget_doc(profile_dir, wid):
    return open(os.path.join(profile_dir, "widgets", wid, "index.html"), encoding="utf-8").read()
fdir = exp["folder"]["path"]
model_doc, video_doc = widget_doc(fdir, "fig-mesh"), widget_doc(fdir, "fig-clip")
check("model and video widgets export with their runtime, not as a poster",
      'id="view"' in model_doc and "WebGLRenderer" in model_doc and '<video id="v" controls' in video_doc
      and "<img" not in model_doc and "<img" not in video_doc, "")
check("the exported model and video runtimes are one classic script with no external url",
      all(d.count("<script") == 1 and 'type="module"' not in d and not re.search(r'<script[^>]*\bsrc=|@import|importScripts', d)
          and not re.search(r'(?:src|href)=["\']?(?:https?:)?//', d) for d in (model_doc, video_doc)), "")
check("no export warns about a missing runtime", not any("runtime" in w["message"] for w in exp["folder"]["warnings"]),
      str(exp["folder"]["warnings"])[:300])
glb = os.path.join(ROOT, "models/mesh.glb")
orig = open(glb, "rb").read()
with open(glb, "wb") as f:
    f.truncate(51 * 1024 * 1024)
big = export("single-file", "big.html")
check("a 51 MiB model trips the default 50 MiB cap", big["ok"] and any(w["kind"] == "size-cap" for w in big["warnings"]), str(big)[:300])
bigf = export("folder", "big-folder")
check("the folder profile has no cap", bigf["ok"] and not any(w["kind"] == "size-cap" for w in bigf["warnings"]), str(bigf)[:300])
open(glb, "wb").write(orig)
os.remove(os.path.join(OUT, "big.html"))
subprocess.run(["rm", "-rf", os.path.join(OUT, "big-folder")])

# A remote-only asset: refused without approval, and MCP cannot approve it.
rrec = compile("remote.tex", rounds=30)
check("remote-only variant compiles", rrec.get("status") == "success", str(rrec)[:300])
r = export("folder", "remote-folder", main="remote.tex")
check("a remote asset with no local copy is refused", not r["ok"] and "fig-remote" in r["error"]
      and "https://media.example.org/clip.mp4" in r["error"] and "approve" in r["error"], str(r)[:300])
check("...and nothing is left at the destination", not os.path.exists(os.path.join(OUT, "remote-folder")) and not any(n.startswith(".remote") for n in os.listdir(OUT)))
for extra in ["approved_fetch", "approve_fetch", "approve", "fetch"]:
    resp = mcp.request("tools/call", {"name": "export_bundle", "arguments": {
        "root_id": "ip", "main_rel": "remote.tex", "dest": os.path.join(OUT, "self-approved"),
        "profile": "folder", extra: ["https://media.example.org/clip.mp4"]}})
    refused = "error" in resp or (resp.get("result") or {}).get("isError")
    check(f"MCP cannot self-approve with `{extra}`", bool(refused) and not os.path.exists(os.path.join(OUT, "self-approved")), str(resp)[:200])
tool = next((t for t in mcp.request("tools/list")["result"]["tools"] if t["name"] == "export_bundle"), {})
ann = tool.get("annotations") or {}
check("export_bundle advertises it writes, never reaches the network, and is repeatable",
      ann.get("readOnlyHint") is False and ann.get("openWorldHint") is False and ann.get("idempotentHint") is True, str(ann))
check("export_bundle's input has no approval field",
      sorted(tool.get("inputSchema", {}).get("properties", {})) == ["dest", "main_rel", "profile", "root_id", "size_cap_bytes"]
      and tool["inputSchema"].get("additionalProperties") is False, str(tool.get("inputSchema"))[:300])

# The project is exactly as it was before exporting.
after = subprocess.run(["git", "status", "--porcelain"], cwd=ROOT, capture_output=True, text=True).stdout
check("exports wrote nothing into the project", after == before, f"{before!r} -> {after!r}")

# An earlier bundle is replaced; a foreign folder is not.
again = export("folder", "folder")
check("re-exporting over an earlier bundle succeeds", again["ok"], str(again)[:200])
os.makedirs(os.path.join(OUT, "mine"))
open(os.path.join(OUT, "mine", "keep.txt"), "w").write("keep")
foreign = export("folder", "mine")
check("a non-empty foreign folder is refused and untouched", not foreign["ok"] and os.path.isfile(os.path.join(OUT, "mine", "keep.txt")), str(foreign)[:200])

# Preview: the same single-file export, written by the core to a scratch
# folder under the app data dir (XDG_DATA_HOME here), path returned, nothing
# opened, project untouched, next run replaces the previous preview.
ptool = next((t for t in mcp.request("tools/list")["result"]["tools"] if t["name"] == "preview_bundle"), {})
pann = ptool.get("annotations") or {}
check("preview_bundle takes only a project and main file, and never reaches the network",
      sorted(ptool.get("inputSchema", {}).get("properties", {})) == ["main_rel", "root_id"]
      and pann.get("openWorldHint") is False and pann.get("readOnlyHint") is False, str(ptool)[:300])
status_before = subprocess.run(["git", "status", "--porcelain"], cwd=ROOT, capture_output=True, text=True).stdout
pv = call("preview_bundle", {"root_id": "ip", "main_rel": "main.tex"})
check("preview_bundle succeeds", pv["ok"], str(pv)[:300])
if pv["ok"]:
    pp = pv["path"]
    check("preview path is an existing index.html", os.path.isabs(pp) and os.path.basename(pp) == "index.html" and os.path.isfile(pp), pp)
    check("preview is outside the project", not os.path.realpath(pp).startswith(os.path.realpath(ROOT) + os.sep), pp)
    check("preview is under the app data dir", os.path.realpath(pp).startswith(os.path.realpath(os.environ["XDG_DATA_HOME"]) + os.sep), pp)
    html = open(pp, encoding="utf-8").read()
    check("preview is the self-contained single-file bundle", 'id="mfw-manifest"' in html and 'id="pdf-link"' in html and pv["widgets"] == 5)
    check("preview returns a hint and opens nothing itself", pp in pv["hint"])
    stale = os.path.join(os.path.dirname(pp), "stale.txt")
    open(stale, "w").write("old")
    pv2 = call("preview_bundle", {"root_id": "ip", "main_rel": "main.tex"})
    check("the next preview reuses the folder and cleans the last one", pv2["ok"] and pv2["path"] == pp and not os.path.exists(stale) and os.listdir(os.path.dirname(pp)) == ["index.html"], str(pv2)[:200])
status_after = subprocess.run(["git", "status", "--porcelain"], cwd=ROOT, capture_output=True, text=True).stdout
check("previews leave the project tree unchanged (porcelain)", status_after == status_before == before, repr(status_after))
pbad = call("preview_bundle", {"root_id": "nope", "main_rel": "main.tex"})
check("preview of an unknown project is refused", not pbad["ok"], str(pbad)[:200])

# Declared frame origins. Three https listeners on loopback (started later
# by reader-run.mjs, one self-signed certificate) play a widget.json origin
# (A), a macro-option origin (B) and an origin nobody declared (U). fig-embed
# declares A in widget.json and frames A and U; fig-macro declares B with
# framedomains= and frames B and A (A is fig-embed's, not its own), then
# navigates its own frame to A (allowed by the single-file reader's union).
import socket
EMBED = os.path.join(scratch, "embed")
os.makedirs(EMBED, exist_ok=True)
cert, key = os.path.join(EMBED, "cert.pem"), os.path.join(EMBED, "key.pem")
ssl_ok = subprocess.run(["openssl", "req", "-x509", "-newkey", "rsa:2048", "-nodes", "-keyout", key, "-out", cert,
                         "-days", "1", "-subj", "/CN=127.0.0.1", "-addext", "subjectAltName=IP:127.0.0.1"],
                        capture_output=True).returncode == 0
check("embed: a self-signed certificate for the loopback origins", ssl_ok)
def free_port():
    s = socket.socket()
    s.bind(("127.0.0.1", 0))
    port = s.getsockname()[1]
    s.close()
    return port
PA, PB, PU = free_port(), free_port(), free_port()
A, B, U = (f"https://127.0.0.1:{p}" for p in (PA, PB, PU))
open(os.path.join(EMBED, "ports"), "w").write(f"{PA},{PB},{PU}")
for folder, frames in [("embed", [f"{A}/w1-declared", f"{U}/w1-undeclared"]),
                       ("macro", [f"{B}/w2-declared", f"{A}/w2-cross"])]:
    d = os.path.join(ROOT, "widgets", folder)
    os.makedirs(d, exist_ok=True)
    open(os.path.join(d, "index.html"), "w").write(
        f"<!doctype html><title>{folder}</title><p>{folder}</p>"
        + "".join(f'<iframe src="{u}" width="80" height="40"></iframe>' for u in frames)
        + (f'<script>setTimeout(function () {{ location.href = "{A}/w2-selfnav"; }}, 1000)</script>'
           if folder == "macro" else ""))
json.dump({"csp": {"frameDomains": [A]}}, open(os.path.join(ROOT, "widgets/embed/widget.json"), "w"))
open(os.path.join(ROOT, "embed.tex"), "w").write(r"""\documentclass{article}
\usepackage{maleficium-interactive}
\begin{document}
\interactive[height=3cm, id=fig-embed, alt={Frames its declared origin and an undeclared one}]{widgets/embed/}

\interactive[height=3cm, id=fig-macro, alt={Frames its macro origin and the other widget's},
  framedomains=%s]{widgets/macro/}
\end{document}
""" % B)
erec = compile("embed.tex")
check("embed: the two-widget variant compiles", erec.get("status") == "success", str(erec)[:300])
wl = call("widgets", {"root_id": "ip", "main_rel": "embed.tex"})
csps = {w["id"]: w.get("csp") for w in wl.get("widgets", [])} if wl["ok"] else {}
check("embed: the widget list carries each widget's own declared origins",
      csps.get("fig-embed", {}).get("frameDomains") == [A] and csps.get("fig-macro", {}).get("frameDomains") == [B], str(wl)[:300])
st = call("widgets_status", {"root_id": "ip", "main_rel": "embed.tex"})
listed = {w["widget"]: w for w in st.get("widgets", [])} if st["ok"] else {}
check("embed: both widgets await approval with their declared origins",
      all(listed.get(i, {}).get("status") == "approval_required" for i in ("fig-embed", "fig-macro"))
      and listed["fig-macro"]["declaredOrigins"]["frameDomains"] == [B], str(st)[:300])
for profile, name in [("single-file", "embed-single.html"), ("folder", "embed-folder")]:
    r = export(profile, name, main="embed.tex")
    check(f"embed: {profile} export succeeds", r["ok"], str(r)[:300])
    if not r["ok"]:
        continue
    dest = os.path.join(OUT, name)
    m, html = manifest_of(profile, dest)
    by = {w["id"]: w for w in m["widgets"]}
    check(f"embed: {profile} manifest validates and records each widget's csp",
          not schema_errors(m) and by["fig-embed"].get("csp") == {"connectDomains": [], "resourceDomains": [], "frameDomains": [A]}
          and by["fig-macro"]["csp"]["frameDomains"] == [B] and "framedomains" not in by["fig-macro"].get("options", {}),
          str(schema_errors(m)) + str(by)[:300])
    if profile == "single-file":
        docs = json.loads(re.search(r'<script type="application/json" id="mfw-widgets">(.*?)</script>', html, re.S).group(1))
        reader = html
    else:
        docs = {w: open(os.path.join(dest, f"widgets/{w}/index.html"), encoding="utf-8").read() for w in by}
        reader = open(os.path.join(dest, "index.html"), encoding="utf-8").read()
    pol = {w: re.search(r'<meta http-equiv="Content-Security-Policy" content="([^"]*)">', d).group(1) for w, d in docs.items()}
    check(f"embed: {profile} each widget's policy frames its own origin only",
          f"frame-src {A};" in pol["fig-embed"] and B not in pol["fig-embed"]
          and f"frame-src {B};" in pol["fig-macro"] and A not in pol["fig-macro"]
          and all(U not in p for p in pol.values()), str(pol))
    rpol = re.search(r'<meta http-equiv="Content-Security-Policy" content="([^"]*)">', reader).group(1)
    want = "frame-src " + " ".join(sorted([A, B])) if profile == "single-file" else "frame-src 'self'"
    check(f"embed: {profile} reader frame-src is {want}", want in rpol and U not in rpol and rpol.count("frame-src") == 1, rpol)

mcp.p.kill()

# No network: the same exports in a process inside an empty network
# namespace (loopback only, nothing routable). Roots are per-process, so it
# grants the project again; the compiled outputs are on disk already.
if os.environ.get("NONET") == "1":
    offline = McpClient(["unshare", "-rn", BIN], "export-offline")
    og = flat(offline.tool("grant", {"root_id": "ip", "root": ROOT}))
    check("offline: grant", og["ok"], str(og))
    net = subprocess.run(["unshare", "-rn", "sh", "-c", "cat /proc/net/dev | tail -n +3 | cut -d: -f1 | tr -d ' ' | tr '\\n' ' '"],
                         capture_output=True, text=True).stdout.strip()
    check("offline: the namespace has loopback only", net in ("lo", ""), net)
    for profile, name in [("folder", "o-folder"), ("single-file", "o-single.html"), ("hosted", "o-hosted")]:
        r = flat(offline.tool("export_bundle", {"root_id": "ip", "main_rel": "main.tex", "dest": os.path.join(OUT, name), "profile": profile}))
        check(f"offline: {profile} export needs no network", r["ok"], str(r)[:300])
    offline.p.kill()
else:
    print("skip: no network-namespace support here (unshare -rn); the core test pins that it has no http client")

sys.exit(1 if fails else 0)
EOF
driver_status=$?
[ "$driver_status" -eq 0 ] || fail "export run failed"

# The companion reader, in each headless browser, over the bundles just
# exported, then the declared-origin cells over the two-widget variant.
for browser in chromium firefox webkit; do
  timeout 400 node "$DEVROOT/e2e/reader-run.mjs" --single "$SCRATCH/out/single.html" \
    --folder "$SCRATCH/out/folder" --browser "$browser" \
    --embed-single "$SCRATCH/out/embed-single.html" --embed-folder "$SCRATCH/out/embed-folder" \
    --embed-ports "$(cat "$SCRATCH/embed/ports")" \
    --cert "$SCRATCH/embed/cert.pem" --key "$SCRATCH/embed/key.pem" || fail "reader run failed in $browser"
done

echo ""
echo "EXPORT PROOFS COMPLETE: live MCP run green."
echo "  root: $ROOT"
