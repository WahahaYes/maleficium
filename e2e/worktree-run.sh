#!/bin/sh
# worktree-run.sh — run a harness command from a pinned-commit scratch worktree.
#
# Isolates repeatable runs from live-checkout churn: another writer saving
# src/ mid-run kills vite and wedges captures, and playground fixtures drift
# underfoot. The worktree holds a frozen commit plus only the carrier patch
# (default: uncommitted e2e/ edits under test); untracked files stay behind
# by design. Heavy state is reused, never rebuilt: node_modules is symlinked
# in, Rust shares the main checkout's target dir via CARGO_TARGET_DIR (cargo
# serializes concurrent builds on its own lock), vite's dep cache lives in the
# worktree (VITE_CACHE_DIR) so the shared node_modules/.vite a live dev server
# serves is never rewritten, and app/runtime caches stay
# the caller's job (e.g. stills reuses one STILLS_HOME so Tectonic bundles
# download once).
#
# GUI harnesses still serialize on :1420: stills pins its devUrl there and
# vite binds it with strictPort. Dev loops (scripts/dev.sh) pick a free
# port pair instead, but the first one takes :1420 too. A run whose port is
# held bind-fails loudly instead of killing anyone: reclaim only signals
# orphans whose cwd is inside its own root, so a worktree run can never
# TERM another checkout's (or a live) dev server.
#
# Usage: ./e2e/worktree-run.sh [<ref>] -- <command...>
#   ref defaults to HEAD. The worktree lives in OS tmp; it is removed on
#   success and kept (path printed) on failure.
# Env: WORKTREE_CARRY (default "e2e") — tracked paths whose uncommitted diff
#   is applied on top; WORKTREE_KEEP=1 — keep the worktree even on success.
set -eu
unset CDPATH
ROOT=$(cd -- "$(dirname -- "$0")/.." && pwd -P)
[ -n "${WORKTREE_ACTIVE:-}" ] && { echo "worktree-run: already inside a worktree run — refusing to nest" >&2; exit 1; }
REF="HEAD"
if [ "${1:-}" != "--" ] && [ $# -gt 0 ]; then REF="$1"; shift; fi
[ "${1:-}" = "--" ] || { echo "worktree-run: usage: worktree-run.sh [<ref>] -- <command...>" >&2; exit 1; }
shift
[ $# -gt 0 ] || { echo "worktree-run: no command given" >&2; exit 1; }

SHORT=$(git -C "$ROOT" rev-parse --short "$REF") || exit 1
WT_BASE=$(mktemp -d /tmp/maleficium-worktree-XXXXXX)
rmdir "$WT_BASE"
WT="$WT_BASE-$SHORT"
git -C "$ROOT" worktree add --detach "$WT" "$REF" >&2 || exit 1
echo "worktree-run: $REF ($SHORT) at $WT"

ST=0
finish() {
  if [ "$ST" -eq 0 ] && [ -z "${WORKTREE_KEEP:-}" ]; then
    git -C "$ROOT" worktree remove --force "$WT" >&2 || true
  else
    echo "worktree-run: keeping $WT (exit $ST)" >&2
  fi
}
trap finish EXIT INT TERM

CARRY=${WORKTREE_CARRY:-e2e}
if [ -n "${WORKTREE_PATCH_FILE:-}" ]; then
  # Pre-snapshotted carrier: no live git reads (the live tree may be
  # mid-rebase while another writer stacks commits — a live diff then
  # reads clean and the worktree silently loses the edits under test).
  git -C "$WT" apply "$WORKTREE_PATCH_FILE" || { echo "worktree-run: snapshot patch does not apply onto $REF" >&2; ST=1; exit 1; }
  echo "worktree-run: applied snapshot carrier $WORKTREE_PATCH_FILE"
else
# shellcheck disable=SC2086
UNTRACKED=$(git -C "$ROOT" status --porcelain -- $CARRY | grep '^??' | cut -c4- || true)
if [ -n "$UNTRACKED" ]; then
  # New harness files under test ride along by copy (the worktree is
  # scratch); tracked modifications ride as a patch below.
  echo "$UNTRACKED" | while IFS= read -r f; do
    mkdir -p "$WT/$(dirname "$f")"
    cp -p "$ROOT/$f" "$WT/$f" || { echo "worktree-run: cannot copy $f" >&2; exit 1; }
  done
  echo "worktree-run: copied untracked carrier files ($CARRY):"
  echo "$UNTRACKED" | sed 's/^/  /' >&2
fi
# shellcheck disable=SC2086
PATCH=$(git -C "$ROOT" diff -- $CARRY)
if [ -n "$PATCH" ]; then
  printf '%s\n' "$PATCH" | git -C "$WT" apply - || { echo "worktree-run: carrier patch does not apply — rebase it onto $REF" >&2; ST=1; exit 1; }
  # shellcheck disable=SC2086
  echo "worktree-run: carried uncommitted diff ($CARRY): $(git -C "$ROOT" diff --numstat -- $CARRY | wc -l) files"
else
  echo "worktree-run: clean carrier ($CARRY), worktree is $REF verbatim"
fi
fi

ln -s "$ROOT/node_modules" "$WT/node_modules"
# Engine sidecars are fetched, never tracked: carry the main checkout's.
mkdir -p "$WT/src-tauri/binaries"
for b in "$ROOT"/src-tauri/binaries/*; do
  [ -e "$b" ] && ln -s "$b" "$WT/src-tauri/binaries/"
done
export WORKTREE_ACTIVE=1
export CARGO_TARGET_DIR="$ROOT/src-tauri/target"
# Vite optimizes deps into the worktree's own cache: the shared
# node_modules/.vite belongs to the main checkout's dev server.
export VITE_CACHE_DIR="$WT/.vite-cache"
# The symlinked node_modules resolves outside the worktree root: allow its
# real path, or vite 403s the files it serves from there (pdf.js worker).
VITE_FS_ALLOW=$(cd "$ROOT/node_modules" && pwd -P)
export VITE_FS_ALLOW

set +e
(cd "$WT" && "$@")
ST=$?
set -e
[ "$ST" -eq 0 ] || echo "worktree-run: command failed with $ST" >&2
exit "$ST"
