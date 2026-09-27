"""mcp-compile-smoke.py — compile one page through an installed maleficium-mcp.

Speaks JSON-RPC over stdio: grants a scratch root, compiles main.tex, polls
to the end. Passing proves the installed copy finds its bundled engine, the
engine runs, and the app's data dirs resolve on this OS.

Usage: python3 e2e/mcp-compile-smoke.py <maleficium-mcp> <scratch-dir>
Used by e2e/release-smoke.sh (in the container) and build.yml (macOS, Windows).
"""
import json, os, subprocess, sys, time

mcp, root = sys.argv[1], os.path.abspath(sys.argv[2])
os.makedirs(root, exist_ok=True)
with open(os.path.join(root, "main.tex"), "w") as f:
    f.write("\\documentclass{article}\n\\begin{document}\nSmoke.\n\\end{document}\n")
p = subprocess.Popen([mcp], stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True, bufsize=1)
n = [0]
def send(method, params):
    n[0] += 1
    p.stdin.write(json.dumps({"jsonrpc": "2.0", "id": n[0], "method": method, "params": params}) + "\n")
    p.stdin.flush()
    return json.loads(p.stdout.readline())
def call(name, args):
    r = send("tools/call", {"name": name, "arguments": args})["result"]
    if r.get("isError"):
        sys.exit("smoke: FAIL %s: %s" % (name, "".join(c.get("text", "") for c in r["content"])))
    return r["structuredContent"]
send("initialize", {"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "smoke", "version": "0"}})
p.stdin.write(json.dumps({"jsonrpc": "2.0", "method": "notifications/initialized"}) + "\n"); p.stdin.flush()
call("grant", {"root_id": "smoke", "root": root})
job = call("compile_run", {"root_id": "smoke", "rel": "main.tex"})["job_id"]
deadline = time.time() + 300
rec = {"status": "running"}
while rec["status"] == "running" and time.time() < deadline:
    time.sleep(2)
    rec = call("compile_poll", {"job_id": job, "tail_lines": 5})
if rec["status"] != "success":
    sys.exit("smoke: FAIL compile %s: %s" % (rec["status"], str(rec)[:300]))
print("smoke: compile ok (installed engine, %s)" % rec.get("pdf_url"))
p.stdin.close()
p.wait(timeout=10)
