#!/bin/sh
# Report whether upstream Tectonic has moved off the pins the app compiles
# against. Dev/CI tooling that needs network; the app never runs it.
#   bundle:  the pinned URL still serves the pinned digest (its SHA256SUM
#            member, located through the bundle index);
#   relay:   the relay's default bundle still redirects to the pinned URL,
#            and no newer bundle format is published;
#   engine:  the latest Tectonic release is the sidecar version fetched.
# Pins are read from the source that uses them. Exits 1 when anything moved
# (after reporting every check), 2 when a check could not run.
# POSIX sh; needs curl, gzip.
set -eu
unset CDPATH
ROOT=$(cd -- "$(dirname -- "$0")/.." && pwd -P)
ENGINE_RS="$ROOT/src-tauri/core/src/engine.rs"
ENGINE_CARGO="$ROOT/src-tauri/engine/Cargo.toml"
RELAY=https://relay.fullyjustified.net
RELAY_FORMAT=33
RELEASES=https://api.github.com/repos/tectonic-typesetting/tectonic/releases/latest

die() { echo "check-upstream: $1" >&2; exit 2; }
command -v curl >/dev/null 2>&1 || die "missing tool: curl"
command -v gzip >/dev/null 2>&1 || die "missing tool: gzip"

pin() { sed -n "s/^pub const $1: &str = \"\\(.*\\)\";/\\1/p" "$ENGINE_RS"; }
BUNDLE_URL=$(pin BUNDLE_URL)
BUNDLE_DIGEST=$(pin BUNDLE_DIGEST)
TECTONIC_VERSION=$(sed -n 's/^tectonic = "=\(.*\)"$/\1/p' "$ENGINE_CARGO")
[ -n "$BUNDLE_URL" ] && [ -n "$BUNDLE_DIGEST" ] || die "bundle pins not found in $ENGINE_RS"
[ -n "$TECTONIC_VERSION" ] || die "tectonic pin not found in $ENGINE_CARGO"

fetch() { curl -fsS --max-time 60 "$@"; }
moved=0
report() {
    # report <check> <pinned> <upstream>
    if [ "$2" = "$3" ]; then
        echo "ok     $1: $2"
    else
        echo "MOVED  $1: pinned $2, upstream $3"
        moved=1
    fi
}

echo "pins: bundle $BUNDLE_URL"
echo "      digest $BUNDLE_DIGEST"
echo "      tectonic $TECTONIC_VERSION"

sum_at=$(fetch "$BUNDLE_URL.index.gz" | gzip -dc | sed -n 's/^SHA256SUM \([0-9]*\) \([0-9]*\)$/\1 \2/p') ||
    die "bundle index unreachable: $BUNDLE_URL.index.gz"
[ -n "$sum_at" ] || die "bundle index lists no SHA256SUM"
sum_off=${sum_at% *}
sum_len=${sum_at#* }
digest=$(fetch -r "$sum_off-$((sum_off + sum_len - 1))" "$BUNDLE_URL") || die "bundle unreachable: $BUNDLE_URL"
report "bundle digest" "$BUNDLE_DIGEST" "$digest"

target=$(fetch -o /dev/null -w '%{redirect_url}' "$RELAY/default_bundle_v$RELAY_FORMAT.tar" -I) ||
    die "relay unreachable: $RELAY"
report "relay default bundle" "$BUNDLE_URL" "$target"
next=$((RELAY_FORMAT + 1))
code=$(curl -sS --max-time 60 -o /dev/null -w '%{http_code}' -I "$RELAY/default_bundle_v$next.tar") ||
    die "relay unreachable: $RELAY"
case "$code" in
    404) echo "ok     relay format: no default_bundle_v$next" ;;
    *) echo "MOVED  relay format: default_bundle_v$next answers $code"; moved=1 ;;
esac

tag=$(fetch "$RELEASES" | sed -n 's/.*"tag_name": *"tectonic@\([^"]*\)".*/\1/p') ||
    die "release feed unreachable: $RELEASES"
[ -n "$tag" ] || die "release feed carries no tectonic tag"
report "tectonic release" "$TECTONIC_VERSION" "$tag"

exit "$moved"
