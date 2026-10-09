#!/bin/sh
# Assemble a Windows x86_64 LaTeXML add-on tree ON LINUX from:
#   - a Strawberry Perl portable zip (perl.exe, core lib, the prebuilt
#     XML::LibXML / XML::LibXSLT / JSON::XS / Clone XS DLLs and the
#     libxml2/libxslt/libexslt/libiconv/liblzma/zlib DLLs), and
#   - the pure-Perl parts of a Linux add-on tree from build-linux.sh
#     (LaTeXML itself, Image::Size, the in-memory DB_File stub).
# Strawberry's own DB_File (Berkeley DB 6.2, AGPL-3) is NOT copied.
#
# UNVERIFIED: this produces a tree and an archive; it has never been run on
# Windows. Proving it needs a Windows host (see the spike note).
#
#   assemble-windows.sh STRAWBERRY_PORTABLE.zip LINUX_TREE OUTDIR
# POSIX sh. Needs unzip, zip, zstd.
set -eu
unset CDPATH
ZIP=${1:?usage: assemble-windows.sh STRAWBERRY.zip LINUX_TREE OUTDIR}
LTREE=${2:?linux add-on tree (the latexml-addon dir)}
OUT=${3:?output dir}
mkdir -p "$OUT"
OUT=$(cd -- "$OUT" && pwd -P)
HERE=$(cd -- "$(dirname -- "$0")" && pwd -P)
WORK="$OUT/strawberry"
DEST="$OUT/latexml-addon"
rm -rf "$WORK" "$DEST"
mkdir -p "$WORK" "$DEST/bin" "$DEST/site/lib/auto"
unzip -q "$ZIP" 'perl/bin/*' 'perl/lib/*' 'perl/vendor/lib/*' 'c/bin/*.dll' -d "$WORK"
SP="$WORK/perl"

# perl.exe finds lib\ and site\lib\ relative to its own DLL, so the tree is
# relocatable without Strawberry's Portable.pm shim.
for f in perl.exe perl542.dll libgcc_s_seh-1.dll libstdc++-6.dll libwinpthread-1.dll; do
  cp "$SP/bin/$f" "$DEST/bin/"
done
for f in libxml2-2__ libxslt-1__ libexslt-0__ libiconv-2__ liblzma-5__ zlib1__; do
  cp "$WORK/c/bin/$f.dll" "$DEST/bin/"
done
cp -R "$SP/lib" "$DEST/lib"

# Vendor distributions LaTeXML loads (from a %INC dump of a real conversion),
# copied whole so on-demand submodules (URI::*, LWP::Protocol::*,
# Text::Unidecode tables) come along.
V="$SP/vendor/lib"
for p in Archive Clone.pm common Encode/Locale.pm File/Which.pm HTTP IO/String.pm \
  JSON/XS.pm JSON/XS LWP LWP.pm Parse Pod/Find.pm Pod/Parser.pm Text/Unidecode.pm \
  Text/Unidecode Try Types URI URI.pm XML/LibXML XML/LibXML.pm XML/LibXSLT.pm \
  XML/SAX XML/SAX.pm XML/NamespaceSupport.pm Win32/Console Win32/Console.pm \
  Win32/ShellQuote.pm; do
  [ -e "$V/$p" ] || { echo "warn: $p not in this Strawberry" >&2; continue; }
  mkdir -p "$DEST/site/lib/$(dirname "$p")"
  cp -R "$V/$p" "$DEST/site/lib/$p"
done
for a in XML/LibXML XML/LibXSLT JSON/XS Clone Win32/Console Win32/Console/ANSI; do
  [ -d "$V/auto/$a" ] || continue
  mkdir -p "$DEST/site/lib/auto/$a"
  find "$V/auto/$a" -maxdepth 1 -type f -exec cp {} "$DEST/site/lib/auto/$a/" \;
done

# Pure Perl from the Linux build.
L=$(find "$LTREE/lib/perl5/site_perl" -maxdepth 1 -mindepth 1 -type d | head -1)
cp -R "$L/LaTeXML" "$L/LaTeXML.pm" "$L/DB_File.pm" "$DEST/site/lib/"
mkdir -p "$DEST/site/lib/Image"
cp "$L/Image/Size.pm" "$DEST/site/lib/Image/"
for s in latexmlc latexml latexmlpost; do cp "$LTREE/bin/$s" "$DEST/bin/"; done
cp -R "$HERE/stubs" "$DEST/stubs"
cat > "$DEST/latexmlc.cmd" <<'EOF'
@"%~dp0bin\perl.exe" "%~dp0bin\latexmlc" %*
EOF

find "$DEST" -name '*.pod' -delete
find "$DEST" -type d -name pod -prune -exec rm -rf {} +
chmod -R u+w "$DEST"
rm -rf "$WORK"
du -sm "$DEST"
cd "$OUT"
rm -f latexml-addon-windows-x86_64.zip latexml-addon-windows-x86_64.tar.zst
zip -qr9 latexml-addon-windows-x86_64.zip latexml-addon
tar -cf - latexml-addon | zstd -q -19 -o latexml-addon-windows-x86_64.tar.zst
ls -l "$OUT"
