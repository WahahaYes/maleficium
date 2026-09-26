#!/bin/sh
# worktree-run.sh — run a harness command from a pinned-commit scratch worktree.
#
# Isolates repeatable runs from live-checkout churn: another writer saving
# src/ mid-run kills vite and wedges captures, and playground fixtures drift
# underfoot. The worktree holds a frozen commit plus only the carrier patch
# (default: uncommitted e2e/ edits under test); untracked files stay behind
# by design. Heavy state is reused, never rebuilt: node_modules is symlinked
# in; the worktree itself is one persistent checkout, moved to each ref with
# `checkout --force`, so unchanged files keep their mtimes and cargo sees
# nothing to rebuild; Rust builds into the harness's own target dir
# (HARNESS_TARGET_DIR), never the main checkout's, so a harness run never
# invalidates a live dev build and vice versa; vite's dep cache lives in the
# worktree (VITE_CACHE_DIR) and survives between runs; app/runtime caches
# stay the caller's job (e.g. stills reuses one STILLS_HOME so Tectonic
# bundles download once).
#
# GUI harnesses still serialize on :1420: stills pins its devUrl there and
# vite binds it with strictPort. Dev loops (scripts/dev.sh) pick a free
# port pair instead, but the first one takes :1420 too. A run whose port is
# held bind-fails loudly instead of killing anyone: reclaim only signals
# orphans whose cwd is inside its own root, so a worktree run can never
# TERM another checkout's (or a live) dev server.
#
# Usage: ./e2e/worktree-run.sh [<ref>] -- <command...>
#   ref defaults to HEAD. The worktree is HARNESS_WORKTREE (default
#   /var/tmp/maleficium-harness-wt), held under a lock for the run and left
#   in place afterwards (inspect it after a failure; the next run resets it).
#   If another run holds it, this run falls back to a throwaway worktree in
#   OS tmp (a full rebuild; removed on success, kept on failure).
# Env: WORKTREE_CARRY (default "e2e") — tracked paths whose uncommitted diff
#   is applied on top; WORKTREE_KEEP=1 — keep a throwaway worktree even on
#   success; HARNESS_TARGET_DIR (default /var/tmp/maleficium-harness-target).
set -eu
unset CDPATH
ROOT=$(cd -- "$(dirname -- "$0")/.." && pwd -P)
[ -n "${WORKTREE_ACTIVE:-}" ] && { echo "worktree-run: already inside a worktree run — refusing to nest" >&2; exit 1; }
REF="HEAD"
if [ "${1:-}" != "--" ] && [ $# -gt 0 ]; then REF="$1"; shift; fi
[ "${1:-}" = "--" ] || { echo "worktree-run: usage: worktree-run.sh [<ref>] -- <command...>" >&2; exit 1; }
shift
[ $# -gt 0 ] || { echo "worktree-run: no command given" >&2; exit 1; }

# Resolved here, in the caller's checkout: inside the worktree, HEAD would
# name the worktree's own last commit.
SHA=$(git -C "$ROOT" rev-parse --verify "$REF^{commit}") || exit 1
SHORT=$(git -C "$ROOT" rev-parse --short "$SHA")
# Throwaway worktrees kept by failed runs pile up in OS tmp; a day on, no
# run is still using one.
find /tmp -maxdepth 1 -type d -name 'maleficium-worktree-*' -mtime +0 2>/dev/null |
  while IFS= read -r old; do
    git -C "$ROOT" worktree remove --force "$old" 2>/dev/null || rm -rf "$old"
    echo "worktree-run: pruned stale $old" >&2
  done
git -C "$ROOT" worktree prune

WT=${HARNESS_WORKTREE:-/var/tmp/maleficium-harness-wt}
THROWAWAY=""
# The lock lives for this shell (fd 9) and dies with it, crash included.
exec 9>"$WT.lock"
if flock -n 9; then
  if git -C "$ROOT" worktree list --porcelain | grep -qx "worktree $WT"; then
    git -C "$WT" checkout -q --detach --force "$SHA" >&2 || exit 1
    # Everything untracked goes (last run's carrier, fixtures, outputs) but
    # vite's dep cache, which is the point of keeping the worktree.
    git -C "$WT" clean -q -fdx -e .vite-cache >&2 || exit 1
  else
    rm -rf "$WT"
    git -C "$ROOT" worktree prune
    git -C "$ROOT" worktree add --detach "$WT" "$SHA" >&2 || exit 1
  fi
else
  echo "worktree-run: $WT is held by another run — using a throwaway worktree (full rebuild)" >&2
  exec 9>&-
  WT_BASE=$(mktemp -d /tmp/maleficium-worktree-XXXXXX)
  rmdir "$WT_BASE"
  WT="$WT_BASE-$SHORT"
  THROWAWAY=1
  git -C "$ROOT" worktree add --detach "$WT" "$SHA" >&2 || exit 1
fi
echo "worktree-run: $REF ($SHORT) at $WT"

ST=0
finish() {
  if [ -z "$THROWAWAY" ]; then
    [ "$ST" -eq 0 ] || echo "worktree-run: $WT left at the failing state (exit $ST)" >&2
  elif [ "$ST" -eq 0 ] && [ -z "${WORKTREE_KEEP:-}" ]; then
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
UNTRACKED=$(git -C "$ROOT" status --porcelain --untracked-files=all -- $CARRY | grep '^??' | cut -c4- || true)
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

ln -sfn "$ROOT/node_modules" "$WT/node_modules"
# Engine sidecars are fetched, never tracked: carry the main checkout's.
mkdir -p "$WT/src-tauri/binaries"
for b in "$ROOT"/src-tauri/binaries/*; do
  [ -e "$b" ] && ln -sf "$b" "$WT/src-tauri/binaries/"
done
export WORKTREE_ACTIVE=1
export CARGO_TARGET_DIR="${HARNESS_TARGET_DIR:-/var/tmp/maleficium-harness-target}"
# Vite optimizes deps into the worktree's own cache: the shared
# node_modules/.vite belongs to the main checkout's dev server.
export VITE_CACHE_DIR="$WT/.vite-cache"
# The symlinked node_modules resolves outside the worktree root: allow its
# real path, or vite 403s the files it serves from there (pdf.js worker).
VITE_FS_ALLOW=$(cd "$ROOT/node_modules" && pwd -P)
export VITE_FS_ALLOW

set +e
# 9>&-: the command's own children (vite, the app) never inherit the lock.
(cd "$WT" && "$@") 9>&-
ST=$?
set -e
[ "$ST" -eq 0 ] || echo "worktree-run: command failed with $ST" >&2
exit "$ST"
