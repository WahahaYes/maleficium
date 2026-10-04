#!/bin/bash
# Widget approval run over the stdio sidecar. Spawns `maleficium-mcp`,
# compiles a scratch copy of e2e/fixtures/interactive/, then checks the
# approval boundary end to end: widgets_status and widget_check report the
# html widget as approval_required (a normal result, never an error) with
# everything an agent needs; no tool or parameter can approve, revoke or set
# auto-approval; files in the project that claim approval are ignored; and
# the approval store in the app data dir (written here the way the desktop's
# Approve writes it) is what counts: approved, edited, auto mode, a widened
# origin, a corrupt store failing closed. The project is never written.
set -euo pipefail

DEVROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN="${CARGO_TARGET_DIR:-$DEVROOT/src-tauri/target}/debug/maleficium-mcp"
FIXTURE="$DEVROOT/e2e/fixtures/interactive"
SCRATCH="$(mktemp -d /tmp/maleficium-approval-XXXXXX)"
trap 'rm -rf "$SCRATCH"' EXIT

fail() { echo "FAIL: $1"; exit 1; }

cargo build -q --manifest-path "$DEVROOT/src-tauri/Cargo.toml" --bin maleficium-mcp \
  || fail "cannot build maleficium-mcp"
[ -x "$BIN" ] || fail "sidecar missing after build: $BIN"

cp -r "$FIXTURE" "$SCRATCH/proj"
cp "$DEVROOT/src-tauri/interactive/maleficium-interactive.sty" "$SCRATCH/proj/"
cd "$SCRATCH/proj"
git init -q
git add -A
git -c user.email=driver@local -c user.name=driver commit -qm "fixture"
ROOT="$(cd "$SCRATCH/proj" && pwd -P)"

export MCP_BIN="$BIN" MCP_ROOT="$ROOT" DEVROOT SCRATCH
# Reuse the warm engine cache: the first cold download is minutes.
export XDG_CACHE_HOME="${DRIVER_CACHE:-$HOME/.cache}"
# The app data dir (approval store, event log) is this run's own.
export XDG_DATA_HOME="$SCRATCH/data"
ENGINE_RS="$DEVROOT/src-tauri/core/src/engine.rs"
BUNDLE_DIGEST="$(sed -n 's/^pub const BUNDLE_DIGEST: &str = "\([0-9a-f]*\)";/\1/p' "$ENGINE_RS")"
[ -n "$BUNDLE_DIGEST" ] || fail "pinned bundle digest not found in $ENGINE_RS"
if [ -f "$XDG_CACHE_HOME/io.github.wahahayes.maleficium/maleficium-tectonic/$BUNDLE_DIGEST/bundles/data/$BUNDLE_DIGEST.index" ]; then
  export POLL_ROUNDS=40
else
  export POLL_ROUNDS=120
fi

python3 - <<'EOF'
import hashlib, json, os, subprocess, sys, time
BIN = os.environ["MCP_BIN"]
ROOT = os.environ["MCP_ROOT"]
DATA = os.environ["XDG_DATA_HOME"]
ROUNDS = int(os.environ["POLL_ROUNDS"])
fails = []
def check(name, cond, detail=""):
    print(("ok: " if cond else "FAIL: ") + name + (f" ({detail})" if detail and not cond else ""))
    if not cond:
        fails.append(name)

sys.path.insert(0, os.path.join(os.environ["DEVROOT"], "e2e"))
from mcp_client import McpClient

mcp = McpClient([BIN], "approval")

def raw(name, args):
    """The whole tools/call response: result (with isError) or a protocol error."""
    return mcp.request("tools/call", {"name": name, "arguments": args})

def call(name, args):
    r = raw(name, args).get("result") or {}
    return r, r.get("structuredContent") or {}

def porcelain():
    return subprocess.run(["git", "status", "--porcelain", "--untracked-files=all"], cwd=ROOT,
                          capture_output=True, text=True).stdout

g = mcp.tool("grant", {"root_id": "ip", "root": ROOT})
check("grant project root", g[0], str(g))
ok, r = mcp.tool("compile_run", {"root_id": "ip", "rel": "main.tex"})
job = (r or {}).get("job_id", "") if ok else ""
rec = {"status": "not-started"}
for _ in range(ROUNDS if job else 0):
    time.sleep(3)
    rec = mcp.tool("compile_poll", {"job_id": job, "tail_lines": 5})[1]
    if rec.get("status") != "running":
        break
check("fixture compiles", rec.get("status") == "success", str(rec)[:300])
# The run commits what the compile left, so later diffs show
# only what this run edits on purpose.
subprocess.run(["git", "add", "-A"], cwd=ROOT, check=True)
subprocess.run(["git", "-c", "user.email=driver@local", "-c", "user.name=driver", "commit", "-qm", "compiled", "--allow-empty"], cwd=ROOT, check=True)

# --- the tool surface --------------------------------------------------------
tools = mcp.request("tools/list")["result"]["tools"]
names = {t["name"]: t for t in tools}
check("widgets_status and widget_check are listed", "widgets_status" in names and "widget_check" in names)
banned = ["approv", "revok", "auto", "mode", "trust"]
bad_names = [t["name"] for t in tools if any(b in t["name"].lower() for b in banned)]
check("no tool is named for approving, revoking or auto mode", not bad_names, str(bad_names))
bad_props = [(t["name"], p) for t in tools for p in (t.get("inputSchema", {}).get("properties") or {})
             if any(b in p.lower() for b in banned)]
check("no tool takes an approval, revoke or mode parameter", not bad_props, str(bad_props))
for n in ["widgets_status", "widget_check"]:
    ann = names[n].get("annotations") or {}
    check(f"{n} is read-only and closed-world", ann.get("readOnlyHint") is True and ann.get("openWorldHint") is False, str(ann))
    check(f"{n} refuses unknown fields", names[n]["inputSchema"].get("additionalProperties") is False)

# --- never approved ----------------------------------------------------------
before = porcelain()
REQUIRED = ["status", "kind", "widget", "path", "digest", "cause", "declaredOrigins", "whatHappens",
            "userAction", "agentMustNot", "message", "panel", "autoApprove"]
def complete(w, cause):
    missing = [k for k in REQUIRED if k not in w]
    return (not missing and w["status"] == "approval_required" and w["cause"] == cause
            and w["widget"] == "fig-demo" and w["path"] == "widgets/demo" and len(w["digest"]) == 64
            and w["panel"] == "View > Widgets"
            and w["userAction"] == "In Maleficium open View > Widgets, find 'fig-demo', review the source diff, and click Approve."
            and "fig-demo" in w["message"] and len(w["agentMustNot"]) >= 3), missing

res, s = call("widgets_status", {"root_id": "ip", "main_rel": "main.tex"})
check("widgets_status is a normal result", res.get("isError") is not True and bool(s), str(res)[:300])
check("one html widget pending, auto-approval off", s.get("pending") == 1 and s.get("autoApprove") is False, str(s)[:300])
check("first-party widgets are exempt", sorted(s.get("exempt", [])) == ["fig-chart", "fig-clip", "fig-mesh", "tab-results"], str(s.get("exempt")))
w = (s.get("widgets") or [{}])[0]
good, missing = complete(w, "never_approved")
check("the pending item carries id, path, digest, cause, panel, user action and message", good, f"missing {missing}: {json.dumps(w)[:400]}")
DIGEST = w.get("digest", "")

res, c = call("widget_check", {"root_id": "ip", "main_rel": "main.tex", "widget": "fig-demo"})
check("widget_check: approval_required is a normal result (isError false)", res.get("isError") is False or res.get("isError") is None, str(res)[:300])
good, missing = complete(c, "never_approved")
check("widget_check: the same complete approval_required", good and c.get("digest") == DIGEST, f"missing {missing}")
text = "".join(x.get("text", "") for x in res.get("content") or [])
check("widget_check: text-only hosts see the same json", "approval_required" in text and "View > Widgets" in text)
ok, e = mcp.tool("widget_check", {"root_id": "ip", "main_rel": "main.tex", "widget": "fig-mesh"})
check("widget_check on a first-party widget is an error naming why", not ok and "needs no approval" in e, str(e)[:200])

for extra in ["approve", "approved", "auto_approve", "revoke", "digest"]:
    resp = raw("widget_check", {"root_id": "ip", "main_rel": "main.tex", "widget": "fig-demo", extra: True})
    refused = "error" in resp or (resp.get("result") or {}).get("isError")
    check(f"widget_check refuses `{extra}`", bool(refused), str(resp)[:200])
    resp = raw("widgets_status", {"root_id": "ip", "main_rel": "main.tex", extra: True})
    refused = "error" in resp or (resp.get("result") or {}).get("isError")
    check(f"widgets_status refuses `{extra}`", bool(refused), str(resp)[:200])
for fake in ["widget_approve", "widget_revoke", "widget_auto_approve", "approve", "set_auto_approve"]:
    resp = raw(fake, {"root_id": "ip", "main_rel": "main.tex", "widget": "fig-demo", "digest": DIGEST, "on": True})
    refused = "error" in resp or (resp.get("result") or {}).get("isError")
    check(f"there is no `{fake}` tool", bool(refused), str(resp)[:200])
check("checking wrote nothing into the project", porcelain() == before, porcelain())

# --- a cloned project's claims count for nothing -----------------------------
claim = {"format": 1, "root": ROOT, "autoApprove": True,
         "widgets": {"widgets/demo": {"widget": "fig-demo", "digest": DIGEST, "origins": {},
                                      "files": {}, "approvedAt": 1, "revoked": False}}}
for rel in [".maleficium/approvals.json", ".maleficium/widgets/approvals.json", "store.json"]:
    os.makedirs(os.path.dirname(os.path.join(ROOT, rel)) or ROOT, exist_ok=True)
    json.dump(claim, open(os.path.join(ROOT, rel), "w"))
_, c = call("widget_check", {"root_id": "ip", "main_rel": "main.tex", "widget": "fig-demo"})
check("approval files inside the project are ignored", c.get("cause") == "never_approved" and c.get("autoApprove") is False, json.dumps(c)[:200])
for rel in [".maleficium", "store.json"]:
    subprocess.run(["rm", "-rf", os.path.join(ROOT, rel)], check=True)

# --- the store in the app data dir is what counts ----------------------------
# Written the way the desktop's Approve writes it (MCP cannot): keyed by the
# canonical project path, outside the project.
key = hashlib.sha256(ROOT.encode()).hexdigest()[:32]
store_dir = os.path.join(DATA, "io.github.wahahayes.maleficium", "maleficium-widgets", "approvals", key)
store = os.path.join(store_dir, "store.json")
check("the approval store home is outside the project", not os.path.realpath(store_dir).startswith(ROOT + os.sep))
def write_store(auto, digest, origins=None):
    os.makedirs(store_dir, exist_ok=True)
    json.dump({"format": 1, "root": ROOT, "autoApprove": auto,
               "widgets": {"widgets/demo": {"widget": "fig-demo", "digest": digest, "origins": origins or {},
                                            "files": {}, "approvedAt": 1, "revoked": False}}}, open(store, "w"))
def checked():
    return call("widget_check", {"root_id": "ip", "main_rel": "main.tex", "widget": "fig-demo"})[1]

write_store(False, DIGEST)
# No session root may reach the store: a granted one would let a replace
# or restore rewrite approvals.
for label, d in [("the project's store folder", store_dir), ("the app data dir holding it", DATA)]:
    ok, e = mcp.tool("grant", {"root_id": "store", "root": d})
    check(f"grant refuses {label}", not ok and "widget approvals" in e, str(e)[:200])
c = checked()
check("approved at its digest", c.get("status") == "approved" and c.get("via") == "user" and c.get("digest") == DIGEST, json.dumps(c)[:200])
_, s = call("widgets_status", {"root_id": "ip", "main_rel": "main.tex"})
check("widgets_status: nothing pending once approved", s.get("pending") == 0, json.dumps(s)[:200])

page = os.path.join(ROOT, "widgets/demo/index.html")
orig = open(page).read()
open(page, "w").write(orig + "<!-- agent edit -->\n")
c = checked()
good, missing = complete(c, "changed_since_approval")
check("an edit drops it to changed_since_approval", good and c.get("approvedDigest") == DIGEST and c["digest"] != DIGEST, json.dumps(c)[:300])
open(page, "w").write(orig)
check("restoring the approved bytes approves it again", checked().get("status") == "approved")

# Auto mode (the user's setting): a content edit is approved, a new origin is not.
write_store(True, DIGEST)
open(page, "w").write(orig + "<!-- iterate -->\n")
c = checked()
check("auto mode approves a content edit", c.get("status") == "approved" and c.get("via") == "auto" and c.get("approvedDigest") == DIGEST, json.dumps(c)[:200])
manifest = os.path.join(ROOT, "widgets/demo/widget.json")
json.dump({"csp": {"frameDomains": ["https://www.youtube-nocookie.com"]}}, open(manifest, "w"))
c = checked()
good, missing = complete(c, "declared_origins_changed")
check("auto mode does not approve a new declared origin", good and c["declaredOrigins"]["frameDomains"] == ["https://www.youtube-nocookie.com"]
      and "youtube-nocookie" in c["message"], json.dumps(c)[:300])
_, s = call("widgets_status", {"root_id": "ip", "main_rel": "main.tex"})
check("widgets_status reports the mode it cannot change", s.get("autoApprove") is True and s.get("pending") == 1, json.dumps(s)[:200])
os.remove(manifest)
open(page, "w").write(orig)

# The same origin declared through the macro option instead: the folder is
# untouched, a recompile records the option, and auto mode still leaves it
# to the user.
def recompile():
    ok, r = mcp.tool("compile_run", {"root_id": "ip", "rel": "main.tex"})
    job = (r or {}).get("job_id", "") if ok else ""
    rec = {"status": "not-started"}
    for _ in range(ROUNDS if job else 0):
        time.sleep(3)
        rec = mcp.tool("compile_poll", {"job_id": job, "tail_lines": 5})[1]
        if rec.get("status") != "running":
            break
    return rec.get("status") == "success"
main = os.path.join(ROOT, "main.tex")
main_orig = open(main).read()
open(main, "w").write(main_orig.replace("id=fig-demo,", "id=fig-demo, framedomains=https://player.vimeo.com,"))
check("a recompile with framedomains= on the html widget succeeds", recompile())
c = checked()
good, missing = complete(c, "declared_origins_changed")
check("an origin added by macro option alone is not auto-approved", good and c["declaredOrigins"]["frameDomains"] == ["https://player.vimeo.com"]
      and c["digest"] != DIGEST, json.dumps(c)[:300])
open(main, "w").write(main_orig)
check("...and the recompile without it is approved again", recompile() and checked().get("status") == "approved")

# A corrupt store fails closed.
for bad in ["{", "", '{"format": 1}', json.dumps({"format": 2, "root": ROOT, "autoApprove": True, "widgets": {}})]:
    open(store, "w").write(bad)
    _, s = call("widgets_status", {"root_id": "ip", "main_rel": "main.tex"})
    w = (s.get("widgets") or [{}])[0]
    check(f"a corrupt store ({bad[:12]!r}) counts as no approvals and auto off",
          w.get("cause") == "never_approved" and s.get("autoApprove") is False and s.get("storeError"), json.dumps(s)[:200])

# A symlink in the widget folder: never approvable, listed as unavailable.
os.symlink("/etc/hostname", os.path.join(ROOT, "widgets/demo/leak.txt"))
_, s = call("widgets_status", {"root_id": "ip", "main_rel": "main.tex"})
check("a symlink in the widget folder makes it unavailable, not approved",
      not s.get("widgets") and s.get("unavailable") and "symlink" in s["unavailable"][0]["error"], json.dumps(s)[:300])
ok, e = mcp.tool("widget_check", {"root_id": "ip", "main_rel": "main.tex", "widget": "fig-demo"})
check("widget_check on it is an error", not ok and "symlink" in e, str(e)[:200])
os.remove(os.path.join(ROOT, "widgets/demo/leak.txt"))

# The digest change was logged, observed by the agent, in the app data log.
log = os.path.join(DATA, "io.github.wahahayes.maleficium", "maleficium-log", "events.jsonl")
lines = [json.loads(l) for l in open(log)] if os.path.isfile(log) else []
changed = [l for l in lines if (l.get("event") or {}).get("action") == "widget.digest-changed"]
check("digest changes are logged as typed events with actor agent",
      changed and all(l["actor"] == "agent" for l in changed) and {l["event"]["cause"] for l in changed} >= {"changed_since_approval", "declared_origins_changed"},
      str(changed)[:300])

check("the run left the project as it found it", porcelain() == before, porcelain())
mcp.p.kill()
sys.exit(1 if fails else 0)
EOF

echo ""
echo "APPROVAL PROOFS COMPLETE: live MCP run green."
