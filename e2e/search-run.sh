#!/bin/bash
# Search driver over the stdio sidecar: spawns `maleficium-mcp` and scripts
# the project-index tools (search, find_files, the structure tools that read
# the index) over JSON-RPC against a scratch copy of e2e/fixtures/simple/.
# No compile, no network. Writes nothing outside OS tmp.
set -euo pipefail

DEVROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN="${CARGO_TARGET_DIR:-$DEVROOT/src-tauri/target}/debug/maleficium-mcp"
SCRATCH="$(mktemp -d /tmp/maleficium-search-XXXXXX)"
trap 'rm -rf "$SCRATCH"' EXIT

[ -x "$BIN" ] || { echo "FAIL: sidecar missing: build with cargo build --manifest-path src-tauri/Cargo.toml --bin maleficium-mcp"; exit 1; }

cp -r "$DEVROOT/e2e/fixtures/simple" "$SCRATCH/proj"
BEFORE="$(cd "$SCRATCH/proj" && find . -type f | sort)"
export MCP_BIN="$BIN" MCP_ROOT="$SCRATCH/proj"
# App data (history, readiness) is per run.
export XDG_DATA_HOME="$SCRATCH/data" XDG_CACHE_HOME="$SCRATCH/cache"

python3 - <<'EOF'
import json, os, subprocess, sys, time
BIN, ROOT = os.environ["MCP_BIN"], os.environ["MCP_ROOT"]
fails = []
def check(name, cond, detail=""):
    print(("ok: " if cond else "FAIL: ") + name + (f" ({str(detail)[:300]})" if not cond else ""))
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
def call(name, args):
    r = send("tools/call", {"name": name, "arguments": args})["result"]
    if r.get("isError"):
        return {"ok": False, "error": "".join(c.get("text", "") for c in r.get("content") or [])}
    return {"ok": True, **r["structuredContent"]}

send("initialize", {"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "search-driver", "version": "0"}})
p.stdin.write(json.dumps({"jsonrpc": "2.0", "method": "notifications/initialized"}) + "\n"); p.stdin.flush()

check("grant", call("grant", {"root_id": "s", "root": ROOT})["ok"])

t0 = time.time()
r = call("search", {"root_id": "s", "pattern": "figure", "main_rel": "main.tex"})
ms = (time.time() - t0) * 1000
files = [f["rel"] for f in r.get("files", [])]
check("literal search returns ranked rows, main document first", r["ok"] and files and files[0] == "main.tex", r)
print(f"   search: {r.get('hits')} hits in {len(files)} files, {r.get('searched')} searched, {ms:.0f} ms")
check("every file carries source and revision", all(f["source"] == "disk" and len(f["revision"]) == 16 for f in r["files"]), r)
hit = r["files"][0]["hits"][0]
line = open(os.path.join(ROOT, "main.tex"), encoding="utf-8").read().split("\n")[hit["line"] - 1]
check("hit line and column point at the match", line[hit["col"]:hit["col"] + hit["len"]].lower() == "figure", (hit, line))

r = call("search", {"root_id": "s", "pattern": r"\\ref\{[^}]*\}", "regex": True, "case_sensitive": True})
check("regex search", r["ok"] and r["hits"] > 0, r)
r = call("search", {"root_id": "s", "pattern": "(", "regex": True})
check("an invalid regex is an error result", not r["ok"] and "invalid pattern" in r["error"], r)
r = call("search", {"root_id": "s", "pattern": "e", "max": 5})
check("cap truncates and counts the rest", r["ok"] and r["hits"] == 5 and r["truncated"] > 0, r)

open(os.path.join(ROOT, "chapters", "fresh.tex"), "w").write("a brand new zebracorn\n")
r = call("search", {"root_id": "s", "pattern": "zebracorn"})
check("a file written after the first search is found (no watcher: stat walk)", r["ok"] and [f["rel"] for f in r["files"]] == ["chapters/fresh.tex"], r)

r = call("find_files", {"root_id": "s", "query": "meth"})
check("find_files ranks the fuzzy match first", r["ok"] and r["files"][0]["rel"] == "chapters/method.tex", r)

lr = call("labels_refs", {"root_id": "s", "main_rel": "main.tex"})
check("labels_refs reads the index", lr["ok"] and lr["source"] == "disk" and any(l["key"] == "sec:intro" for l in lr["labels"]), lr)
fg = call("file_graph", {"root_id": "s", "main_rel": "main.tex"})
check("file_graph reads the index", fg["ok"] and any(f["rel"] == "chapters/method.tex" for f in fg["files"]), fg)

check("escape fails closed", not call("search", {"root_id": "nope", "pattern": "x"})["ok"])

# Replace: preview writes nothing, apply by token, stale plans refused
# whole, undo restores exact bytes.
def snap():
    out = {}
    for d, _, fs in os.walk(ROOT):
        for f in fs:
            path = os.path.join(d, f)
            out[os.path.relpath(path, ROOT)] = open(path, "rb").read()
    return out
orig = snap()
pv = call("replace_preview", {"root_id": "s", "pattern": r"\\ref\{(sec:\w+)\}", "regex": True, "case_sensitive": True, "replacement": r"\cref{$1}"})
check("replace_preview plans files and writes nothing", pv["ok"] and pv["replacements"] > 0 and snap() == orig, pv)
ap = call("replace_apply", {"root_id": "s", "token": pv["token"]})
check("replace_apply writes every planned file", ap["ok"] and sorted(ap["written"]) == sorted(f["rel"] for f in pv["files"]), ap)
after = snap()
changed = [k for k in orig if orig[k] != after[k]]
check("only planned files changed, with groups expanded", sorted(changed) == sorted(ap["written"]) and b"\\cref{sec:" in after[changed[0]], changed)
check("a plan applies once", not call("replace_apply", {"root_id": "s", "token": pv["token"]})["ok"])
un = call("replace_undo", {"root_id": "s", "batch": ap["batch"]})
check("replace_undo restores the exact bytes", un["ok"] and snap() == orig, un)

d = call("definition", {"root_id": "s", "kind": "label", "key": "sec:intro"})
check("definition finds a label", d["ok"] and d["lookup"]["definitions"][0]["rel"] == "main.tex", d)
d = call("definition", {"root_id": "s", "kind": "citation", "key": "knuth1984texbook"})
check("definition summarizes a bib entry", d["ok"] and d["lookup"]["definitions"][0]["rel"] == "refs.bib" and "—" in d["lookup"]["definitions"][0]["summary"], d)
main_lines = open(os.path.join(ROOT, "main.tex"), encoding="utf-8").read().split("\n")
ln = next(i for i, l in enumerate(main_lines) if "\\input{chapters/method}" in l)
d = call("definition", {"root_id": "s", "rel": "main.tex", "line": ln + 1, "col": main_lines[ln].index("chapters"), "main_rel": "main.tex"})
check("definition at a position resolves an input", d["ok"] and d["lookup"]["definitions"][0]["rel"] == "chapters/method.tex", d)
d = call("definition", {"root_id": "s", "kind": "label", "key": "no:such"})
check("an undefined key has no definitions", d["ok"] and d["lookup"]["definitions"] == [], d)

pv = call("replace_preview", {"root_id": "s", "pattern": "Figure", "case_sensitive": True, "replacement": "Fig."})
target = os.path.join(ROOT, pv["files"][-1]["rel"])
open(target, "a").write("\n% edited after the preview\n")
before_apply = snap()
ap = call("replace_apply", {"root_id": "s", "token": pv["token"]})
check("a stale plan is refused whole", not ap["ok"] and "changed since the preview" in ap["error"] and snap() == before_apply, ap)

p.stdin.close(); p.wait(timeout=10)
if fails:
    print(f"SEARCH DRIVER: {len(fails)} failed"); sys.exit(1)
print("SEARCH DRIVER: all checks green")
EOF

# The project holds nothing of ours.
AFTER="$(cd "$SCRATCH/proj" && find . -type f ! -name fresh.tex | sort)"
[ "$BEFORE" = "$AFTER" ] || { echo "FAIL: files appeared in the project: $(comm -13 <(echo "$BEFORE") <(echo "$AFTER"))"; exit 1; }
echo "ok: nothing written into the project"
