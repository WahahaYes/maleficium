#!/bin/sh
# Wipe the scratch playground under /tmp and rebuild it from the fixtures, for
# a clean-state test by hand. Every project in it is a fresh copy, so edits,
# the poster cache (.maleficium/) and widget approvals from a last run are gone.
# What it builds is scripts/playground.sh's set (see its header).
# <dir> defaults to /tmp/maleficium-playground and must be a folder under /tmp
# whose name starts with maleficium-, so a typo cannot delete anything else.
# Usage: sh scripts/reset-playground.sh [<dir>]
# POSIX sh.
set -eu
unset CDPATH
ROOT=$(cd -- "$(dirname -- "$0")/.." && pwd -P)
DEST=${1:-/tmp/maleficium-playground}

case "$DEST" in
    (/tmp/maleficium-?*) ;;
    (*) echo "reset-playground: refusing $DEST (use /tmp/maleficium-<name>)" >&2; exit 1 ;;
esac
case "$DEST" in
    (*/../*|*/..) echo "reset-playground: refusing $DEST (no ..)" >&2; exit 1 ;;
esac

if [ -e "$DEST" ] || [ -L "$DEST" ]; then
    rm -rf -- "$DEST"
    echo "reset-playground: cleared $DEST"
fi
sh "$ROOT/scripts/playground.sh" "$DEST"
echo "reset-playground: open $DEST/interactive-paper in the app"
