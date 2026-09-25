#!/bin/sh
# Cut a release: bump X.Y.Z in every manifest, prepend the CHANGELOG entry,
# commit it, tag vX.Y.Z, build the .deb + AppImage, and (only with
# --publish) push and open the GitHub release. Local steps run by default;
# nothing leaves the machine without --publish.
# Usage: sh scripts/release.sh X.Y.Z [--notes-file FILE] [--host] [--out DIR] [--publish] [--skip-build]
# POSIX sh. Needs: git, python3, npm, cargo, docker (or a release-ready host with --host), gh (with --publish).
set -eu
unset CDPATH
ROOT=$(cd -- "$(dirname -- "$0")/.." && pwd -P)

usage() { printf 'usage: sh scripts/release.sh X.Y.Z [--notes-file FILE] [--host] [--out DIR] [--publish] [--skip-build]\n' >&2; exit 2; }

VER="${1:-}"; shift || usage
case "$VER" in (*[!0-9.]*|""|.*|*.|*..*) usage;; esac
case "$VER" in ?*.?*.?*) ;; (*) usage;; esac

NOTES_FILE=""; HOST=0; OUT="../maleficium-release"; PUBLISH=0; SKIP_BUILD=0
while [ $# -gt 0 ]; do
    case "$1" in
        --notes-file) NOTES_FILE="${2:-}"; shift 2 || usage ;;
        --host) HOST=1; shift ;;
        --out) OUT="${2:-}"; shift 2 || usage ;;
        --publish) PUBLISH=1; shift ;;
        --skip-build) SKIP_BUILD=1; shift ;;
        *) usage ;;
    esac
done

die() { printf 'release: %s\n' "$1" >&2; exit 1; }
need() { command -v "$1" >/dev/null 2>&1 || die "missing required tool: $1"; }
need git; need python3; need npm; need cargo
[ "$SKIP_BUILD" = 1 ] || { [ "$HOST" = 1 ] || need docker; }
[ "$PUBLISH" = 1 ] && need gh

[ -z "$(git -C "$ROOT" status --porcelain)" ] || die "tree is dirty; commit or stash first"
[ "$(git -C "$ROOT" rev-parse --abbrev-ref HEAD)" = "main" ] || die "run from main"
git -C "$ROOT" rev-parse "v$VER" >/dev/null 2>&1 && die "tag v$VER already exists"

# Bump package.json + package-lock.json together, no tag, no network.
(cd "$ROOT" && npm version --no-git-tag-version --allow-same-version "$VER" >/dev/null)
# Bump the Rust and Tauri manifests (first version line is the package one).
# sed, not a JSON round-trip: loaders reformat unrelated inline arrays.
sed -i '0,/^version = "[^"]*"/s//version = "'"$VER"'"/' "$ROOT/src-tauri/Cargo.toml"
sed -i '0,/"version": "[^"]*"/s//"version": "'"$VER"'"/' "$ROOT/src-tauri/tauri.conf.json"
# Re-sync Cargo.lock to the bumped version. The check needs the sidecars
# present (the Tauri build script resolves them as resources).
[ -n "$(ls -A "$ROOT/src-tauri/binaries" 2>/dev/null)" ] \
    || die "no sidecars in src-tauri/binaries; run sh scripts/fetch-sidecars.sh first"
(cd "$ROOT" && cargo check --manifest-path src-tauri/Cargo.toml --offline >/dev/null 2>&1) \
    || (cd "$ROOT" && cargo check --manifest-path src-tauri/Cargo.toml >/dev/null)
python3 - "$ROOT/package.json" "$ROOT/src-tauri/Cargo.toml" "$ROOT/src-tauri/tauri.conf.json" "$VER" <<'EOF'
import json, re, sys
pkg, cargo, tauri, ver = sys.argv[1:5]
assert json.load(open(pkg))["version"] == ver, "package.json not bumped"
assert re.search(r'^version = "%s"$' % re.escape(ver), open(cargo).read(), re.M), "Cargo.toml not bumped"
assert json.load(open(tauri))["version"] == ver, "tauri.conf.json not bumped"
EOF

# Prepend the CHANGELOG section: notes file, else subjects since the last
# tag, else a stub the releaser fills before publishing.
DATE=$(date +%Y-%m-%d)
if [ -n "$NOTES_FILE" ]; then
    NOTES=$(cat "$NOTES_FILE")
elif PREV=$(git -C "$ROOT" describe --tags --abbrev=0 2>/dev/null); then
    NOTES=$(git -C "$ROOT" log "$PREV..HEAD" --format='- %s')
else
    NOTES="- TBD: fill highlights before publishing."
    printf 'release: no notes file and no previous tag; using a stub\n' >&2
fi
python3 - "$ROOT/CHANGELOG.md" "$VER" "$DATE" "$NOTES" <<'EOF'
import sys
path, ver, date, notes = sys.argv[1:5]
with open(path) as f:
    text = f.read()
section = "## %s - %s\n\n%s\n\n" % (ver, date, notes.rstrip())
lines = text.split("\n")
for i, line in enumerate(lines):
    if line.startswith("## "):
        lines.insert(i, section.rstrip("\n"))
        lines.insert(i + 1, "")
        break
else:
    lines.extend(["", section.rstrip("\n")])
with open(path, "w") as f:
    f.write("\n".join(lines))
EOF
git -C "$ROOT" add package.json package-lock.json src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/tauri.conf.json CHANGELOG.md
git -C "$ROOT" commit -m "release $VER" >/dev/null
git -C "$ROOT" tag -a "v$VER" -m "v$VER"

if [ "$SKIP_BUILD" = 0 ]; then
    mkdir -p "$OUT"
    if [ "$HOST" = 1 ]; then
        [ -n "$(ls -A "$ROOT/src-tauri/binaries" 2>/dev/null)" ] || die "no sidecars; run sh scripts/fetch-sidecars.sh first"
        (cd "$ROOT" && npm run tauri build -- --bundles deb,appimage)
    else
        docker build --output "type=local,dest=$OUT" "$ROOT"
    fi
fi

if [ "$PUBLISH" = 0 ]; then
    printf 'tagged v%s locally. To publish:\n  git push origin main v%s\n' "$VER" "$VER"
    [ "$SKIP_BUILD" = 1 ] || printf '  gh release create v%s --title v%s --notes-file <notes> %s/*.deb %s/*.AppImage\n' "$VER" "$VER" "$OUT" "$OUT"
    exit 0
fi
git -C "$ROOT" push origin main "v$VER"
[ -n "$(ls "$OUT"/*.deb "$OUT"/*.AppImage 2>/dev/null)" ] || die "no artifacts in $OUT; build first"
NOTES_TMP=$(mktemp /tmp/maleficium-release-notes-XXXXXX.md)
trap 'rm -f "$NOTES_TMP"' EXIT
python3 - "$ROOT/CHANGELOG.md" "$VER" <<'EOF' > "$NOTES_TMP"
import re, sys
text = open(sys.argv[1]).read()
m = re.search(r'^## %s.*?\n\n(.*?)\n\n(?=## |\Z)' % re.escape(sys.argv[2]), text, re.S | re.M)
sys.stdout.write(m.group(1) if m else "")
EOF
gh release create "v$VER" --title "v$VER" --notes-file "$NOTES_TMP" "$OUT"/*.deb "$OUT"/*.AppImage
