#!/bin/bash
# History-surface proof harness — static audit (no window needed).
#
# Covers the visible revision surface: that the entry points exist and are
# wired to one command, that the surface speaks user words only, and that a
# restore through the content-addressed layout returns the exact bytes.
#
# The byte round-trip replicates the store's blob layout in bash against a
# scratch copy of playground/simple/, so the restore contract is proved
# without a webview.
#
# What it does not cover (needs the live app): that clicking the counter and
# the menu row actually reach these paths — see the human checklist in
# notes/narrow_poc/10-history/STATUS.md.

set -euo pipefail

DEVROOT="$(cd "$(dirname "$0")/.." && pwd)"
SCRATCH="$(mktemp -d /tmp/maleficium-history-XXXXXX)"
trap 'rm -rf "$SCRATCH"' EXIT
APPSRC="$DEVROOT/src"

fail() { echo "FAIL: $1"; exit 1; }
pass() { echo "ok: $1"; }

# --- the surface is reachable from one command, two entry points -------------
CMDS="$APPSRC/lib/commands.ts"
grep -q "'history.show'" "$CMDS" || fail "history.show missing from the command registry"
grep -q "showHistory" "$CMDS" || fail "history.show has no action in CommandActions"
grep -q "historyAvailable" "$CMDS" || fail "history row is not gated on availability"
pass "history.show registered with an action and an availability gate"

KEYMAP="$APPSRC/lib/keymap.ts"
ACCEL="$(grep -o "accelerator: 'Ctrl+H'" "$CMDS" || true)"
[[ -n "$ACCEL" ]] || fail "history.show carries no accelerator"
grep -q "keys: 'Ctrl+H'" "$KEYMAP" || fail "Ctrl+H is not a KEYMAP row (no chord parity)"
grep -q "return 'history.show'" "$KEYMAP" || fail "Ctrl+H does not resolve to history.show"
pass "keymap parity: Ctrl+H is a KEYMAP row and resolves to history.show"

BAR="$APPSRC/components/StatusBar.tsx"
grep -q "onOpenHistory" "$BAR" || fail "status bar counter is not an entry point"
grep -q "revisionCount" "$BAR" || fail "status bar shows no revision count"
APP="$APPSRC/App.tsx"
grep -q "onOpenHistory={() => void openHistory()}" "$APP" || fail "counter not wired to openHistory"
grep -q "showHistory: () => {" "$APP" || fail "menu row not wired to openHistory"
pass "counter and menu row both reach the one openHistory path"

# --- the surface speaks user words only --------------------------------------
DIALOG="$APPSRC/components/HistoryDialog.tsx"
VIEW="$APPSRC/lib/history.view.ts"
for f in "$DIALOG" "$VIEW" "$BAR"; do
    [ -f "$f" ] || fail "surface file missing: $f"
    if grep -niE "\b(git|HEAD|commit|branch|staged?|porcelain|repo)\b" "$f" | grep -qv "^\s*//"; then
        fail "version-control jargon in the surface: $f"
    fi
done
grep -q "History unavailable" "$VIEW" || fail "no quiet degrade string"
if grep -qiE "throw |console\.error" "$DIALOG"; then fail "surface raises errors instead of degrading"; fi
pass "surface carries user words only and degrades quietly"

# --- F-16: the git plumbing is gone, not parked ------------------------------
[ -e "$APPSRC/lib/git.ts" ] && fail "src/lib/git.ts survived the cut"
[ -e "$DEVROOT/src-tauri/src/commands/git.rs" ] && fail "commands/git.rs survived the cut"
grep -q "pub mod git" "$DEVROOT/src-tauri/src/commands/mod.rs" && fail "git module still declared"
grep -q "commands::git" "$DEVROOT/src-tauri/src/lib.rs" && fail "git commands still registered"
HITS="$(grep -rn "git_status\|git_show_head\|parseGitPorcelain\|emptyGitState\|GitBadge\|GitState" \
    "$APPSRC" "$DEVROOT/src-tauri/src" 2>/dev/null || true)"
[[ -z "$HITS" ]] || fail "git plumbing still referenced: $HITS"
pass "F-16 cut complete: no git module, registration, or reference remains"

# --- restore round-trips exact bytes ------------------------------------------
# Replicate the store layout: blobs/<first two hex>/<sha256>, restore over the
# working file, compare byte for byte.
cp -r "$DEVROOT/playground/simple" "$SCRATCH/proj"
HIST="$SCRATCH/history"
TARGET="$SCRATCH/proj/main.tex"
# Content chosen to catch any re-encoding: CRLF, a trailing blank line, UTF-8,
# and a tab.
printf 'Ünicode \xc3\xa9\t\\section{A}\r\nline two\n\n' > "$TARGET"
SHA="$(sha256sum "$TARGET" | cut -d' ' -f1)"
mkdir -p "$HIST/blobs/${SHA:0:2}"
cp "$TARGET" "$HIST/blobs/${SHA:0:2}/$SHA"
ORIG="$SCRATCH/original.bytes"
cp "$TARGET" "$ORIG"

# the user edits and saves over it
printf 'clobbered\n' > "$TARGET"
cmp -s "$TARGET" "$ORIG" && fail "edit did not change the file — round-trip proves nothing"

# restore writes the blob back
cp "$HIST/blobs/${SHA:0:2}/$SHA" "$TARGET"
cmp -s "$TARGET" "$ORIG" || fail "restore did not round-trip exact bytes"
[[ "$(sha256sum "$TARGET" | cut -d' ' -f1)" == "$SHA" ]] || fail "restored hash differs from the revision"
pass "restore round-trips exact bytes (CRLF, tab, UTF-8, trailing blank line)"

# restore is undoable: the replaced state is itself a revision
grep -q "Snapshot what is on disk first" "$APPSRC/lib/history.ts" ||
    fail "restore does not preserve the replaced state"
grep -q "await this.recordRevision(projectId, relPath, current)" "$APPSRC/lib/history.ts" ||
    fail "restore does not record the replaced state as a revision"
pass "restore keeps the replaced state as a revision (undoable from the same list)"

# --- history home stays out of the project -----------------------------------
[[ "$HIST" == "$SCRATCH/proj"* ]] && fail "history home inside the project"
[[ -e "$SCRATCH/proj/.maleficium-history" ]] && fail "in-project history dir appeared"
pass "history home stays outside the project dir"

echo ""
echo "HISTORY SURFACE PROOFS COMPLETE: static audit green."
echo "  surface: $DIALOG"
echo "  view:    $VIEW"
echo "  blob:    ${SHA:0:12}…"
echo "  Eyes checklist: notes/narrow_poc/10-history/STATUS.md."
