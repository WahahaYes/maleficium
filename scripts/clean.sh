#!/bin/sh
# Reclaim disk from build outputs, by generated-files tier (docs/BUILDING.md
# "Generated files"):
#   npm run clean                — remove intermediates (dist/, out/,
#                                  coverage/): the next build regenerates
#                                  them and nothing live depends on them
#   npm run clean-cache          — clean, plus the caches: cargo incremental
#                                  sessions (the safe bulk; cargo rebuilds
#                                  them on demand), the harness target dir,
#                                  and throwaway harness worktrees in OS tmp
#   npm run clean-cache -- --all — remove the target dirs and node_modules
#                                  outright (next build is a full one;
#                                  implies --cache)
#   npm run clean -- --dry-run   — say what would go and how big, touch nothing
#                                  (works for either mode)
#   npm run clean-cache -- --force — clean caches even while something may be
#                                  building
# The engine sidecar (src-tauri/binaries/) and the format dumps
# (src-tauri/resources/dumps/) are pinned inputs: clean never touches them;
# sh scripts/build-engine.sh regenerates them behind its key stamps.
# Refuses cache removal while a build, dev app or harness might be using the
# caches: deleting under a live cargo or app breaks it mid-flight.
# POSIX sh (npm runs scripts under sh).
set -eu
unset CDPATH
ROOT=$(cd -- "$(dirname -- "$0")/.." && pwd -P)

CACHE=""
ALL=""
DRY=""
FORCE=""
for a in "$@"; do
    case "$a" in
        --cache) CACHE=1 ;;
        --all) ALL=1; CACHE=1 ;;
        --dry-run) DRY=1 ;;
        --force) FORCE=1 ;;
        *)
            echo "clean: unknown option $a (--cache, --all, --dry-run, --force)" >&2
            exit 1
            ;;
    esac
done

TARGETS="$ROOT/src-tauri/target ${HARNESS_TARGET_DIR:-/var/tmp/maleficium-harness-target}"

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

# Intermediates: always safe, nothing live depends on them.
for i in "$ROOT/dist" "$ROOT/out" "$ROOT/coverage"; do
    drop "$i"
done

[ -n "$CACHE" ] || exit 0

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

if [ -n "$ALL" ]; then
    drop "$ROOT/node_modules"
fi

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
