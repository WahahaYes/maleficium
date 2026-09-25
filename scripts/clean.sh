#!/bin/sh
# Reclaim disk from Rust build caches. Cargo never deletes anything: stale
# incremental sessions and dependency builds for old configs pile up until
# a target dir is several times the size of a fresh build.
#   npm run clean                — drop incremental caches (the safe bulk;
#                                  cargo rebuilds them on demand)
#   npm run clean -- --all       — remove the target dirs outright (next
#                                  build is a full one)
#   npm run clean -- --dry-run   — say what would go and how big, touch nothing
#   npm run clean -- --force     — clean even while something may be building
# Covers the main checkout's target, the harness target (HARNESS_TARGET_DIR,
# e2e/worktree-run.sh) and throwaway harness worktrees in OS tmp. Refuses
# while a build, dev app or harness might be using them: deleting under a
# live cargo or app breaks it mid-flight. POSIX sh (npm runs scripts under sh).
set -eu
unset CDPATH
ROOT=$(cd -- "$(dirname -- "$0")/.." && pwd -P)

ALL=""
DRY=""
FORCE=""
for a in "$@"; do
    case "$a" in
        --all) ALL=1 ;;
        --dry-run) DRY=1 ;;
        --force) FORCE=1 ;;
        *)
            echo "clean: unknown option $a (--all, --dry-run, --force)" >&2
            exit 1
            ;;
    esac
done

TARGETS="$ROOT/src-tauri/target ${HARNESS_TARGET_DIR:-/var/tmp/maleficium-harness-target}"

# Bracket patterns never match this script's own pgrep command line.
BUSY=$(pgrep -af '[c]argo (build|run|test|check|clippy)|[r]ustc |[t]auri dev|[s]tills-run\.sh|[d]river-run\.sh|target/debug/[m]aleficium' 2>/dev/null || true)
if [ -n "$BUSY" ]; then
    if [ -n "$FORCE" ]; then
        echo "clean: --force: cleaning although these may be using the caches:"
        echo "$BUSY" | cut -c1-150 | sed 's/^/  /'
    elif [ -z "$DRY" ]; then
        echo "clean: refusing, these may be using the caches (--force to override):" >&2
        echo "$BUSY" | cut -c1-150 | sed 's/^/  /' >&2
        exit 1
    fi
fi

size() { du -sh "$1" 2>/dev/null | cut -f1; }

drop() {
    # $1 = path to remove.
    [ -e "$1" ] || return 0
    if [ -n "$DRY" ]; then
        echo "clean: would remove $1 ($(size "$1"))"
    else
        echo "clean: removing $1 ($(size "$1"))"
        rm -rf "$1"
    fi
}

for t in $TARGETS; do
    [ -d "$t" ] || continue
    echo "clean: $t is $(size "$t")"
    if [ -n "$ALL" ]; then
        drop "$t"
    else
        for profile in "$t"/*/; do
            drop "${profile}incremental"
        done
    fi
    [ -z "$DRY" ] && [ -d "$t" ] && echo "clean: $t now $(size "$t")"
done

# Throwaway worktrees (a harness run that found the shared one busy and
# failed keeps its own); git forgets them only once pruned.
for wt in /tmp/maleficium-worktree-*; do
    [ -d "$wt" ] || continue
    if [ -n "$DRY" ]; then
        echo "clean: would remove worktree $wt ($(size "$wt"))"
    else
        git -C "$ROOT" worktree remove --force "$wt" 2>/dev/null || rm -rf "$wt"
        echo "clean: removed worktree $wt"
    fi
done
[ -n "$DRY" ] || git -C "$ROOT" worktree prune
exit 0
