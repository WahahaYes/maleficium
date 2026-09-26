#!/bin/sh
# Repack a Tauri AppImage without its bundled libwayland-* libraries. The
# bundled copies clash with a newer host Mesa: WebKit aborts with
# EGL_BAD_PARAMETER and the window stays blank (seen on Ubuntu 26.04 with an
# AppImage built on 24.04). Every host that runs the app ships its own.
# Reuses the appimagetool inside the linuxdeploy plugin Tauri already fetched
# and the runtime of the original image, so nothing new is downloaded.
# Usage: sh scripts/repack-appimage.sh <file.AppImage>
# POSIX sh. Needs: the Tauri tool cache (TAURI_TOOLS, default ~/.cache/tauri).
set -eu
unset CDPATH
[ $# -eq 1 ] || { echo "usage: sh scripts/repack-appimage.sh <file.AppImage>" >&2; exit 2; }
IMG=$(realpath "$1")
TOOLS="${TAURI_TOOLS:-$HOME/.cache/tauri}"
PLUGIN="$TOOLS/linuxdeploy-plugin-appimage.AppImage"
[ -x "$PLUGIN" ] || { echo "repack-appimage: missing $PLUGIN (run a Tauri AppImage build first)" >&2; exit 1; }

WORK=$(mktemp -d "${TMPDIR:-/tmp}/maleficium-repack-XXXXXX")
trap 'rm -rf "$WORK"' EXIT
cd "$WORK"

"$PLUGIN" --appimage-extract >/dev/null
mv squashfs-root tool
TOOL=tool/appimagetool-prefix/usr/bin/appimagetool

OFFSET=$("$IMG" --appimage-offset)
head -c "$OFFSET" "$IMG" > runtime

"$IMG" --appimage-extract >/dev/null
rm -f squashfs-root/usr/lib/libwayland-*.so*
for f in squashfs-root/usr/lib/libwayland-*; do
    [ -e "$f" ] && { echo "repack-appimage: libwayland still bundled" >&2; exit 1; }
done

PATH="$WORK/tool/appimagetool-prefix/usr/bin:$PATH" ARCH=x86_64 "$TOOL" --no-appstream --runtime-file runtime squashfs-root repacked.AppImage >/dev/null 2>&1 \
    || { echo "repack-appimage: appimagetool failed" >&2; exit 1; }
chmod 755 repacked.AppImage
mv repacked.AppImage "$IMG"
echo "repack-appimage: dropped bundled libwayland from $(basename "$IMG")"
