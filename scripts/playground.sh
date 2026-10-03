#!/bin/sh
# Make a scratch playground of projects to open in the app by hand:
#   <dir>/simple      a copy of the e2e/fixtures/simple test fixture
#   <dir>/<paper>     a copy of each real paper in e2e/fixtures/vendored
#   <dir>/interactive-paper
#                     a copy of the e2e/fixtures/playground interactive paper
#                     (a real glb model, h264 clip, csv, vega chart and an html
#                     widget; media provenance in its NOTICE). It does not hold
#                     maleficium-interactive.sty: open the folder in the app,
#                     run Tools > Install Interactive Package (or the fix on
#                     the pre-compile warning), then compile. The first compile
#                     fetches `subcaption` from the TeX bundle, so allow the
#                     network once (Make available offline); later ones are
#                     offline. File > Preview in Browser shows the widgets.
#   <dir>/<template>  one project per built-in template, made the way the
#                     app makes one (the template's files minus template.json)
# <dir> defaults to playground/ in the repo root, which git ignores.
# A project folder that already exists is left alone, so edits survive a
# rerun; delete it to get a fresh copy.
# Usage: sh scripts/playground.sh [<dir>]
# POSIX sh.
set -eu
unset CDPATH
ROOT=$(cd -- "$(dirname -- "$0")/.." && pwd -P)
DEST=${1:-"$ROOT/playground"}
mkdir -p "$DEST"

add() { # <name> <source dir>
    if [ -e "$DEST/$1" ]; then
        echo "playground: kept $DEST/$1 (exists)"
        return
    fi
    mkdir "$DEST/$1"
    cp -R "$2/." "$DEST/$1/"
    rm -f "$DEST/$1/template.json" "$DEST/$1/fixture.json"
    echo "playground: made $DEST/$1"
}

add simple "$ROOT/e2e/fixtures/simple"
add interactive-paper "$ROOT/e2e/fixtures/playground"
for p in "$ROOT"/e2e/fixtures/vendored/*/; do
    add "$(basename "$p")" "$p"
done
for t in "$ROOT"/src-tauri/templates/*/; do
    [ -f "$t/template.json" ] || continue
    add "$(basename "$t")" "$t"
done
