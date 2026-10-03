#!/bin/sh
# Build the relocatable LaTeXML add-on for Linux x86_64 in an old-glibc
# container (default ubuntu:22.04, glibc 2.35) and write the packed tree to
# OUTDIR. Needs docker and network (sources are sha256-pinned, CPAN deps are
# not yet). Optional DLDIR caches downloaded sources between runs.
# POSIX sh.
set -eu
unset CDPATH
OUT=${1:?usage: build-linux.sh OUTDIR [DLDIR]}
DLDIR=${2:-}
IMAGE=${LATEXML_BUILD_IMAGE:-ubuntu:22.04}
HERE=$(cd -- "$(dirname -- "$0")" && pwd -P)
mkdir -p "$OUT"
OUT=$(cd -- "$OUT" && pwd -P)
set -- -v "$HERE:/src:ro" -v "$OUT:/out"
if [ -n "$DLDIR" ]; then
  mkdir -p "$DLDIR"
  set -- "$@" -v "$(cd -- "$DLDIR" && pwd -P):/dl"
fi
exec docker run --rm "$@" -e HOST_UID="$(id -u)" -e HOST_GID="$(id -g)" \
  "$IMAGE" sh /src/build-inside.sh
