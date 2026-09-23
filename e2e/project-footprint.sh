#!/bin/bash
# Project-footprint proof harness — static audit (no window needed).
# Run with: ./e2e/project-footprint.sh (from maleficium/).
#
# The app's file operations cannot litter the project dir: every app-local
# derivation resolves outside it. This replicates each derivation in bash
# against a scratch copy of playground/simple/, then asserts
# `git status --porcelain` stays clean and the computed homes land in
# app-data/app-cache — never under the project root.
#
# What it does not cover (needs the live app): that the running commands
# actually call these derivations with the right arguments (open → setMain
# → compile → delete → undo through IPC) — see e2e/driver-run.sh.
#
# Asserts also cover that no in-project path is re-created
# (.maleficium-trash/, .maleficium.json, in-project out/).
#
# The app's own event log is app-local too. Read the current run with:
#   L="${XDG_DATA_HOME:-$HOME/.local/share}/com.ethan.tauri-app/maleficium-log/events.jsonl"
#   cat "$L"                                # every event of this run, one JSON object per line
#   grep '"action":"compile.finish"' "$L"   # one fact, matched on the payload not the prose

set -euo pipefail

DEVROOT="$(cd "$(dirname "$0")/.." && pwd)"
SCRATCH="$(mktemp -d /tmp/maleficium-footprint-XXXXXX)"
trap 'rm -rf "$SCRATCH"' EXIT
APPSRC="$DEVROOT/src"
FIXTURE="$DEVROOT/playground/simple"

fail() { echo "FAIL: $1"; exit 1; }
pass() { echo "ok: $1"; }

# --- fixture: scratch copy of playground/simple/ as a git repo ---------------
cp -r "$FIXTURE" "$SCRATCH/proj"
cd "$SCRATCH/proj"
git init -q
git add -A
git commit -qm "fixture"
ROOT="$SCRATCH/proj"
porcelain() { git status --porcelain; }
[[ -z "$(porcelain)" ]] || fail "fixture repo not clean at start"

# --- djb2 hex (same algorithm as the app derivations) ---
djb2() {
    python3 -c "
h = 5381
for ch in '''$1''':
    h = ((h << 5) + h + ord(ch)) & 0xFFFFFFFF
print('%08x' % h)"
}
HASH="$(djb2 "$ROOT")"
[[ "$HASH" =~ ^[0-9a-f]{8}$ ]] || fail "hash not 8-hex: $HASH"
pass "hashRoot($ROOT) = $HASH (8-hex, deterministic)"

# --- trash home ---
APPDATA="${XDG_DATA_HOME:-$HOME/.local/share}/maleficium-footprint-check"
TRASH="$APPDATA/maleficium-trash/$HASH"
[[ "$TRASH" == "$ROOT"* ]] && fail "trash home inside project: $TRASH"
pass "trash home outside project: $TRASH"
# simulate one delete→undo cycle THROUGH the derived home (rename semantics)
mkdir -p "$TRASH"
mv chapters/method.tex "$TRASH/method.tex.ts-test.trashed"
# A real delete LEGITIMATELY shows as a deletion entry (user's file trashed).
# The footprint rule forbids OUR artifacts appearing — assert exactly that:
# only the expected deletion, no .maleficium-trash/, no .maleficium.json, no out/.
PORC="$(porcelain)"
echo "$PORC" | grep -q " D chapters/method.tex" || fail "expected deletion missing: $PORC"
if echo "$PORC" | grep -q ".maleficium-trash"; then fail "trash dir appeared in project"; fi
if echo "$PORC" | grep -q ".maleficium.json"; then fail "config appeared in project"; fi
if echo "$PORC" | grep -q "out/"; then fail "out/ appeared in project"; fi
[[ -f "$TRASH/method.tex.ts-test.trashed" ]] || fail "trashed bytes missing in app-data"
mv "$TRASH/method.tex.ts-test.trashed" chapters/method.tex
[[ -z "$(porcelain)" ]] || fail "project dirty after simulated undo"
pass "delete-undo round-trips through app-data (only legit deletion mid-cycle)"
# in-project paths must NOT be re-created
[[ -e "$ROOT/.maleficium-trash" ]] && fail "in-project .maleficium-trash re-created"

# --- main-file association (localStorage) ---
# localStorage lives in the webview profile, never the project: assert NO
# .maleficium.json write-site remains in src (besides test/comment mentions).
if grep -rn "writeTextFile.*maleficium.json\|maleficium.json.*write" \
    $APPSRC 2>/dev/null | grep -v test | grep -qv "^.*//"; then
    : # handled below portably
fi
WRITES="$(grep -rn "\.maleficium\.json" $APPSRC --include="*.ts" --include="*.tsx" 2>/dev/null | grep -v "\.test\." | grep -v "^\s*//" | grep -v "\* " || true)"
if echo "$WRITES" | grep -q "writeTextFile\|writeFile"; then
    fail "a .maleficium.json WRITE site still exists: $WRITES"
fi
[[ -e "$ROOT/.maleficium.json" ]] && fail "in-project .maleficium.json re-created"
pass "no .maleficium.json write-site in src; none in project (association is localStorage-only)"

# --- compile out/ ---
CACHEDIR="${XDG_CACHE_HOME:-$HOME/.cache}/com.ethan.tauri-app"
OUT="$CACHEDIR/maleficium-out/$HASH"
[[ "$OUT" == "$ROOT"* ]] && fail "out dir inside project: $OUT"
pass "compile out dir outside project: $OUT"
mkdir -p "$OUT"
touch "$OUT/main.pdf" "$OUT/main.log" "$OUT/main.synctex.gz"
[[ -z "$(porcelain)" ]] || fail "project dirty after simulated compile output"
[[ -e "$ROOT/out" ]] && fail "in-project out/ re-created"
pass "compile artifacts land in cache shard, porcelain clean, no in-project out/"

# --- engine cache ---
ENGINE_CACHE="$CACHEDIR/maleficium-tectonic"
[[ "$ENGINE_CACHE" == "$ROOT"* ]] && fail "engine cache inside project: $ENGINE_CACHE"
grep -q '"TECTONIC_CACHE_DIR", cache' "$DEVROOT/src-tauri/src/core/engine.rs" \
    || fail "engine spawn no longer sets TECTONIC_CACHE_DIR"
pass "engine cache is app-owned, outside project: $ENGINE_CACHE"

# --- history home ---
HIST="$APPDATA/maleficium-history/$HASH"
[[ "$HIST" == "$ROOT"* ]] && fail "history home inside project: $HIST"
pass "history home outside project: $HIST"
# simulate one snapshot: index + one content-addressed blob
mkdir -p "$HIST/blobs/ab"
printf '{"v":1,"seq":1,"files":{}}' > "$HIST/index.json"
printf 'snapshot bytes' > "$HIST/blobs/ab/abcdef"
[[ -z "$(porcelain)" ]] || fail "project dirty after simulated snapshot"
[[ -e "$ROOT/.maleficium-history" ]] && fail "in-project .maleficium-history re-created"
if echo "$(porcelain)" | grep -q "maleficium-history"; then fail "history dir appeared in project"; fi
pass "revision index + blobs land in app-data shard, porcelain clean"
# history must derive its home from app-data, never from the project root
HISTSRC="$DEVROOT/src-tauri/src/core/history.rs"
[ -f "$HISTSRC" ] || fail "history store missing: $HISTSRC"
grep -q '"maleficium-history"' "$HISTSRC" || fail "history store does not use the app-local derivation"
grep -q "data_base_dir()" "$HISTSRC" || fail "history store does not root itself in app-data"
pass "history store derives its home from app-data only"

# --- event log home ---------------------------------------------------------
# The app records its event stream as JSONL under app-data. It is the
# hands-free read of what a run did, so it must never land in the project.
LOGDIR="$APPDATA/maleficium-log"
LOGFILE="$LOGDIR/events.jsonl"
[[ "$LOGFILE" == "$ROOT"* ]] && fail "event log inside project: $LOGFILE"
pass "event log outside project: $LOGFILE"
mkdir -p "$LOGDIR"
printf '{"at":1,"scope":"fs","kind":"info","actor":"system","message":"revision 1 of main.tex","event":{"action":"revision.record","rel":"main.tex","stored":true,"rev":"1","revisions":1}}\n' > "$LOGFILE"
[[ -z "$(porcelain)" ]] || fail "project dirty after simulated log write"
[[ -e "$ROOT/maleficium-log" ]] && fail "in-project maleficium-log re-created"
if echo "$(porcelain)" | grep -q "events.jsonl"; then fail "event log appeared in project"; fi
grep -q '"action"' "$LOGFILE" || fail "log line carries no structured payload"
pass "event log lines land in app-data, porcelain clean, payloads structured"
LOGSRC="$APPSRC/lib/eventlog.ts"
[ -f "$LOGSRC" ] || fail "event log writer missing: $LOGSRC"
grep -q "eventLogPath" "$LOGSRC" || fail "event log writer does not use the app-local derivation"
grep -q "appDataDir" "$LOGSRC" || fail "event log writer does not root itself in app-data"
grep -q "MAX_LOG_EVENTS" "$LOGSRC" || fail "event log writer states no retention bound"
pass "event log writer derives its home from app-data and states its bound"

# --- no-op saves (the blind spot porcelain cannot cover) ---------------------
# An identical-content rewrite leaves `git status` clean but still moves
# mtime, so a user's latexmk/watcher sees a file they only opened. Porcelain
# is blind to it by construction; assert the gap exists, then assert the
# guard that keeps the app out of it.
touch -d '2020-01-01 00:00:00' main.tex
MT_BEFORE="$(stat -c %Y main.tex)"
cp main.tex "$SCRATCH/identical.bytes"
cp "$SCRATCH/identical.bytes" main.tex
MT_AFTER="$(stat -c %Y main.tex)"
[[ "$MT_BEFORE" != "$MT_AFTER" ]] || fail "identical rewrite left mtime untouched — pin is meaningless"
[[ -z "$(porcelain)" ]] || fail "identical rewrite dirtied porcelain"
pass "identical-content rewrite moves mtime while porcelain stays clean (the blind spot)"
git checkout -q -- main.tex
# The editor re-emits its text whenever a doc is set externally (file switch,
# reload). The buffer layer must treat that echo as a non-edit, or autosave
# rewrites files the user never touched.
grep -q "prev.value === value" "$APPSRC/lib/buffers.ts" ||
    fail "buffer layer no longer guards against editor echoes"
pass "an editor echo cannot dirty a buffer (no autosave on open or switch)"

# --- scope + CSP + opener pins (static, no window) ---
# Static scope stays off $HOME, CSP stays an enforced object, opener stays
# fully removed (lockfiles too).
CAPDIR="$DEVROOT/src-tauri/capabilities"
TAURICONF="$DEVROOT/src-tauri/tauri.conf.json"
HOMEHITS="$(grep -rnF '$HOME' "$CAPDIR" 2>/dev/null || true)"
[[ -z "$HOMEHITS" ]] || fail "static capability went home-wide again: $HOMEHITS"
pass "no home-wide static scope in capabilities"
CSPKIND="$(python3 -c "import json,sys; c=json.load(open(sys.argv[1]))['app']['security']['csp']; print('object' if isinstance(c, dict) else 'null')" "$TAURICONF")"
[[ "$CSPKIND" == "object" ]] || fail "security.csp is null or not an object in tauri.conf.json"
pass "security.csp is an enforced object"
for f in \
    "$CAPDIR/default.json" \
    "$DEVROOT/src-tauri/src/lib.rs" \
    "$DEVROOT/src-tauri/Cargo.toml" \
    "$DEVROOT/package.json" \
    "$DEVROOT/src-tauri/Cargo.lock" \
    "$DEVROOT/package-lock.json"; do
    [ -f "$f" ] || fail "opener pin cannot find watched file: $f"
    if grep -q "opener" "$f"; then
        fail "opener reappeared in $f"
    fi
done
pass "opener stays removed (capabilities, rust, manifests, lockfiles)"

# --- final sweep ---------------------------------------------------------------
[[ -z "$(porcelain)" ]] || fail "final porcelain not clean: $(porcelain)"
echo ""
echo "FOOTPRINT PROOFS COMPLETE: static audit green."
echo "  root:   $ROOT"
echo "  hash:   $HASH"
echo "  trash:  $TRASH"
echo "  hist:   $HIST"
echo "  out:    $OUT"
echo "  log:    $LOGFILE  (live: \${XDG_DATA_HOME:-\$HOME/.local/share}/com.ethan.tauri-app/maleficium-log/events.jsonl)"
echo "  Live driver-driven run: e2e/driver-run.sh."
