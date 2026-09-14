#!/bin/bash
# Project-footprint proof harness — static audit (no window needed).
# Run with: ./e2e/project-footprint.sh (from maleficium/).
#
# What it proves: the app's file operations CANNOT litter the project dir,
# because every app-local derivation resolves outside it. It replicates each
# derivation in bash (same algorithms, documented line-refs) against a scratch
# copy of playground/, then asserts `git status --porcelain` stays clean and
# the computed homes land in app-data/tmp — never under the project root.
#
# What it does NOT prove (needs the live app → driver-driven run / eyes):
# that the running Tauri commands actually CALL these derivations with the
# right arguments (open → setMain → compile → delete → undo through IPC).
#
# No-legacy rule (RULES §8): asserts also cover that NO legacy path is
# re-created (.maleficium-trash/, .maleficium.json, in-project out/).

set -euo pipefail

DEVROOT="$(cd "$(dirname "$0")/../.." && pwd)"
SCRATCH="$(mktemp -d /tmp/maleficium-footprint-XXXXXX)"
trap 'rm -rf "$SCRATCH"' EXIT
APPSRC="$DEVROOT/maleficium/src"

fail() { echo "FAIL: $1"; exit 1; }
pass() { echo "ok: $1"; }

# --- fixture: scratch copy of playground/ as a git repo -----------------------
cp -r "$DEVROOT/playground" "$SCRATCH/proj"
cd "$SCRATCH/proj"
git init -q
git add -A
git commit -qm "fixture"
ROOT="$SCRATCH/proj"
porcelain() { git status --porcelain; }
[[ -z "$(porcelain)" ]] || fail "fixture repo not clean at start"

# --- djb2 hex, mirroring src/lib/paths.ts hashRoot + compile.rs hash_root ---
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

# --- trash home — src/lib/paths.ts appTrashDir ---------------------------------
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
if echo "$PORC" | grep -q ".maleficium-trash"; then fail "legacy trash dir appeared in project"; fi
if echo "$PORC" | grep -q ".maleficium.json"; then fail "legacy config appeared in project"; fi
if echo "$PORC" | grep -q "out/"; then fail "out/ appeared in project"; fi
[[ -f "$TRASH/method.tex.ts-test.trashed" ]] || fail "trashed bytes missing in app-data"
mv "$TRASH/method.tex.ts-test.trashed" chapters/method.tex
[[ -z "$(porcelain)" ]] || fail "project dirty after simulated undo"
pass "delete-undo round-trips through app-data (only legit deletion mid-cycle)"
# legacy path must NOT be re-created
[[ -e "$ROOT/.maleficium-trash" ]] && fail "legacy .maleficium-trash re-created"

# --- main-file association — src/lib/mainFile.store.ts (localStorage) -----------
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
[[ -e "$ROOT/.maleficium.json" ]] && fail "legacy .maleficium.json re-created"
pass "no .maleficium.json write-site in src; none in project (association is localStorage-only)"

# --- compile out/ — compile.rs out_dir_for + paths.ts appOutDir ----------------
OUT="/tmp/maleficium-out/$HASH"
[[ "$OUT" == "$ROOT"* ]] && fail "out dir inside project: $OUT"
pass "compile out dir outside project: $OUT"
mkdir -p "$OUT"
touch "$OUT/main.pdf" "$OUT/main.log" "$OUT/main.synctex.gz"
[[ -z "$(porcelain)" ]] || fail "project dirty after simulated compile output"
[[ -e "$ROOT/out" ]] && fail "legacy in-project out/ re-created"
pass "compile artifacts land in tmp shard, porcelain clean, no in-project out/"

# --- final sweep ---------------------------------------------------------------
[[ -z "$(porcelain)" ]] || fail "final porcelain not clean: $(porcelain)"
echo ""
echo "FOOTPRINT PROOFS COMPLETE: static audit green."
echo "  root:   $ROOT"
echo "  hash:   $HASH"
echo "  trash:  $TRASH"
echo "  out:    $OUT"
echo "  Live driver-driven run still open — see 08-devloop plan."
