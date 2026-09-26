#!/bin/sh
# release-smoke.sh — headless smoke for the release artifacts.
#
# From a directory holding Maleficium_0.1.0_amd64.deb +
# Maleficium_0.1.0_amd64.AppImage: installs the .deb in a clean
# ubuntu:24.04 container, asserts the payload (binaries, desktop entry,
# icons, control fields), and launches both bundles under Xvfb. A launch
# that survives the timeout wins (timeout kill = still running).
# What it does NOT cover: how anything looks (that needs a human pass).
#
# Usage: sh e2e/release-smoke.sh <artifact-dir> [timeout-secs]
# POSIX sh. Needs: docker.
set -eu
unset CDPATH
DIR="${1:-}"; TIMEOUT="${2:-20}"
[ -n "$DIR" ] || { echo "usage: sh e2e/release-smoke.sh <artifact-dir> [timeout-secs]" >&2; exit 2; }
DEB="$DIR"/Maleficium_0.1.0_amd64.deb
IMG="$DIR"/Maleficium_0.1.0_amd64.AppImage
[ -f "$DEB" ] || { echo "smoke: missing $DEB" >&2; exit 1; }
[ -f "$IMG" ] || { echo "smoke: missing $IMG" >&2; exit 1; }

echo "smoke: control: $(dpkg -f "$DEB" Package Version Section 2>/dev/null | tr '\n' ' ')"
docker run --rm -v "$DIR:/pkg:ro" ubuntu:24.04 bash -c '
set -eu
export DEBIAN_FRONTEND=noninteractive
apt-get update -qq 2>&1 | tail -n 1
apt-get install -y -qq /pkg/Maleficium_0.1.0_amd64.deb xvfb 2>&1 | tail -n 1
dpkg -L maleficium | grep -qx /usr/bin/maleficium || { echo "smoke: FAIL no app binary"; exit 1; }
dpkg -L maleficium | grep -qx /usr/bin/maleficium-tectonic || { echo "smoke: FAIL no tectonic sidecar"; exit 1; }
dpkg -L maleficium | grep -q "Maleficium.desktop" || { echo "smoke: FAIL no desktop entry"; exit 1; }
echo "smoke: install ok"
export HOME=/tmp/fakehome && mkdir -p "$HOME"
T='"$TIMEOUT"'
code=0; timeout -s KILL "$T" xvfb-run -a maleficium >/tmp/l.log 2>&1 || code=$?
[ "$code" -eq 137 ] && echo "smoke: deb launch ok (alive ${T}s)" || { echo "smoke: FAIL deb exited early"; tail -n 5 /tmp/l.log; exit 1; }
code=0; timeout -s KILL "$T" xvfb-run -a env APPIMAGE_EXTRACT_AND_RUN=1 /pkg/Maleficium_0.1.0_amd64.AppImage >/tmp/a.log 2>&1 || code=$?
[ "$code" -eq 137 ] && echo "smoke: appimage launch ok (alive ${T}s)" || { echo "smoke: FAIL appimage exited early"; tail -n 5 /tmp/a.log; exit 1; }
echo "smoke: ALL GREEN"
'
