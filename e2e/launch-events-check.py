"""launch-events-check.py — prove a first launch really came up, from its event log.

A process that stays alive can still be a blank window. A first launch (no
recent projects) opens the welcome project, and the frontend records each
step to the app's JSONL event log: the webview booted and ran the UI, IPC
reached the Rust side, and project files resolved through the fs scope on
this OS's native paths. Waits for the required actions, fails on any
error-kind event, and prints warnings.

Usage: python3 e2e/launch-events-check.py <events.jsonl> [timeout-secs]
Used by e2e/release-smoke.sh (in the container) and build.yml (macOS, Windows).
"""
import json, os, sys, time

REQUIRED = ["log.open", "template.welcome", "project.open", "index.open", "file.open"]

path = sys.argv[1]
deadline = time.time() + (float(sys.argv[2]) if len(sys.argv) > 2 else 60)

def read():
    try:
        with open(path, encoding="utf-8") as f:
            lines = [l for l in f.read().splitlines() if l.strip()]
    except FileNotFoundError:
        return []
    out = []
    for l in lines:
        try:
            out.append(json.loads(l))
        except ValueError:
            pass  # a line mid-write; the next poll sees it whole
    return out

while True:
    events = read()
    actions = [e["event"].get("action") for e in events if isinstance(e.get("event"), dict)]
    errors = [e for e in events if e.get("kind") == "error"]
    missing = [a for a in REQUIRED if a not in actions]
    if errors or not missing or time.time() > deadline:
        break
    time.sleep(1)

for e in events:
    if e.get("kind") == "warn":
        print("launch: warn: %s" % e.get("message", "")[:200])
if not events:
    sys.exit("launch: FAIL no event log at %s (the UI never started recording)" % path)
if errors:
    sys.exit("launch: FAIL error events: %s" % "; ".join(e.get("message", "")[:200] for e in errors))
if missing:
    sys.exit("launch: FAIL missing %s (have %s)" % (missing, sorted(set(actions))))
print("launch: events ok: %d events, %s" % (len(events), ",".join(REQUIRED)))
