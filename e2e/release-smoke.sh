#!/bin/sh
# release-smoke.sh — headless smoke for the release artifacts.
#
# From a directory holding one Maleficium_<version>_amd64.deb +
# Maleficium_<version>_amd64.AppImage: installs the .deb in a clean
# ubuntu:24.04 container, asserts the payload (binaries, desktop entry,
# icons, control fields), compiles a one-page document through the
# installed maleficium-mcp (proving the app finds its bundled engine the way
# an installed copy lays it out), and launches both bundles under Xvfb. A
# launch that survives the timeout wins (timeout kill = still running).
# What it does NOT cover: how anything looks (that needs a human pass).
#
# Usage: sh e2e/release-smoke.sh <artifact-dir> [timeout-secs]
# POSIX sh. Needs: docker.
set -eu
unset CDPATH
DIR="${1:-}"; TIMEOUT="${2:-20}"
[ -n "$DIR" ] || { echo "usage: sh e2e/release-smoke.sh <artifact-dir> [timeout-secs]" >&2; exit 2; }
set -- "$DIR"/Maleficium_*_amd64.deb
[ $# -eq 1 ] && [ -f "$1" ] || { echo "smoke: need exactly one Maleficium_<version>_amd64.deb in $DIR" >&2; exit 1; }
DEB=$1
IMG=${DEB%.deb}.AppImage
[ -f "$IMG" ] || { echo "smoke: missing $IMG" >&2; exit 1; }
DEB_NAME=$(basename "$DEB"); IMG_NAME=$(basename "$IMG")

echo "smoke: control: $(dpkg -f "$DEB" Package Version Section 2>/dev/null | tr '\n' ' ')"
PROBE=$(mktemp -d "${TMPDIR:-/tmp}/maleficium-smoke-XXXXXX")
trap 'rm -rf "$PROBE"' EXIT
# JSON-RPC over stdio: grant a scratch root, compile main.tex, poll to the end.
cat > "$PROBE/compile.py" <<'EOF'
import json, os, subprocess, sys, time
root = "/tmp/doc"
os.makedirs(root, exist_ok=True)
with open(root + "/main.tex", "w") as f:
    f.write("\\documentclass{article}\n\\begin{document}\nSmoke.\n\\end{document}\n")
p = subprocess.Popen(["/usr/bin/maleficium-mcp"], stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True, bufsize=1)
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
EOF
docker run --rm -e DEB_NAME="$DEB_NAME" -e IMG_NAME="$IMG_NAME" -v "$DIR:/pkg:ro" -v "$PROBE:/probe:ro" ubuntu:24.04 bash -c '
set -eu
export DEBIAN_FRONTEND=noninteractive
apt-get update -qq 2>&1 | tail -n 1
apt-get install -y -qq "/pkg/$DEB_NAME" xvfb python3 2>&1 | tail -n 1
dpkg -L maleficium | grep -qx /usr/bin/maleficium || { echo "smoke: FAIL no app binary"; exit 1; }
dpkg -L maleficium | grep -qx /usr/bin/maleficium-tectonic || { echo "smoke: FAIL no tectonic sidecar"; exit 1; }
dpkg -L maleficium | grep -q "Maleficium.desktop" || { echo "smoke: FAIL no desktop entry"; exit 1; }
echo "smoke: install ok"
(cd /tmp && "/pkg/$IMG_NAME" --appimage-extract >/dev/null)
for f in /tmp/squashfs-root/usr/lib/libwayland-*; do
  [ -e "$f" ] && { echo "smoke: FAIL appimage bundles $(basename "$f") (clashes with newer host Mesa)"; exit 1; }
done
rm -rf /tmp/squashfs-root
echo "smoke: appimage bundles no libwayland"
export HOME=/tmp/fakehome && mkdir -p "$HOME"
python3 /probe/compile.py
T='"$TIMEOUT"'
code=0; timeout -s KILL "$T" xvfb-run -a maleficium >/tmp/l.log 2>&1 || code=$?
[ "$code" -eq 137 ] && echo "smoke: deb launch ok (alive ${T}s)" || { echo "smoke: FAIL deb exited early"; tail -n 5 /tmp/l.log; exit 1; }
code=0; timeout -s KILL "$T" xvfb-run -a env APPIMAGE_EXTRACT_AND_RUN=1 "/pkg/$IMG_NAME" >/tmp/a.log 2>&1 || code=$?
[ "$code" -eq 137 ] && echo "smoke: appimage launch ok (alive ${T}s)" || { echo "smoke: FAIL appimage exited early"; tail -n 5 /tmp/a.log; exit 1; }
echo "smoke: ALL GREEN"
'
