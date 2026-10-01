#!/bin/sh
# Build the relocatable LaTeXML add-on NATIVELY on macOS (arm64 or x86_64,
# whichever this Mac is). Mirrors build-inside.sh (Linux): relocatable perl,
# shared libxml2/libxslt inside the tree, in-memory DB_File stub, no
# ImageMagick. Needs Xcode Command Line Tools and network; no Homebrew.
#
# UNVERIFIED: written on Linux and never run on a Mac. Differences from the
# Linux recipe are the Mach-O steps: dylib install names become
# @rpath/..., the XS .bundle files get an LC_RPATH relative to
# @loader_path, and every rewritten Mach-O is re-signed ad hoc (arm64 kills
# binaries whose signature was invalidated by install_name_tool).
#
#   build-macos.sh OUTDIR
# POSIX sh.
set -eu
unset CDPATH
OUT=${1:?usage: build-macos.sh OUTDIR}
mkdir -p "$OUT"
OUT=$(cd -- "$OUT" && pwd -P)
HERE=$(cd -- "$(dirname -- "$0")" && pwd -P)
WORK=$(mktemp -d "${TMPDIR:-/tmp}/latexml-addon.XXXXXX")
PREFIX="$WORK/latexml-addon"
DL="$WORK/dl"
ARCH=$(uname -m)
JOBS=$(sysctl -n hw.ncpu)
export MACOSX_DEPLOYMENT_TARGET=${MACOSX_DEPLOYMENT_TARGET:-11.0}

# Same pins as build-inside.sh.
PERL_V=5.40.2
PERL_SHA=10d4647cfbb543a7f9ae3e5f6851ec49305232ea7621aed24c7cfbb0bef4b70d
XML2_V=2.13.8
XML2_SHA=277294cb33119ab71b2bc81f2f445e9bc9435b893ad15bb2cd2b0e859a0ee84a
XSLT_V=1.1.43
XSLT_SHA=5a3d6b383ca5afc235b171118e90f5ff6aa27e9fea3303065231a6d403f0183a
CPANM_V=1.7048
CPANM_SHA=59b60907ab9fa4f72ca2004fbe6054911439ae9a906890b4d842a87b25f20f3c
LATEXML_V=0.8.8
LATEXML_SHA=7d2bbe2ce252baf86ba3f388cd0dec3aa4838f49d612b9ec7cc4ff88105badcc

mkdir -p "$DL" "$WORK/build"
fetch() { # url sha
  f="$DL/$(basename "$1")"
  curl -fsSL -o "$f" "$1"
  echo "$2  $f" | shasum -a 256 -c -
}
fetch https://www.cpan.org/src/5.0/perl-$PERL_V.tar.gz $PERL_SHA
fetch https://download.gnome.org/sources/libxml2/2.13/libxml2-$XML2_V.tar.xz $XML2_SHA
fetch https://download.gnome.org/sources/libxslt/1.1/libxslt-$XSLT_V.tar.xz $XSLT_SHA
fetch https://cpan.metacpan.org/authors/id/M/MI/MIYAGAWA/App-cpanminus-$CPANM_V.tar.gz $CPANM_SHA
fetch https://cpan.metacpan.org/authors/id/B/BR/BRMILLER/LaTeXML-$LATEXML_V.tar.gz $LATEXML_SHA

cd "$WORK/build"
LDHP=-Wl,-headerpad_max_install_names
echo "== libxml2 $XML2_V"
tar xf "$DL/libxml2-$XML2_V.tar.xz"
(cd libxml2-$XML2_V && LDFLAGS=$LDHP ./configure -q --prefix="$PREFIX" --enable-shared \
  --disable-static --without-python --without-icu --without-lzma --without-zlib \
  --without-http --without-ftp --without-debug >/dev/null && make -s -j"$JOBS" >/dev/null \
  && make -s install >/dev/null)
echo "== libxslt $XSLT_V"
tar xf "$DL/libxslt-$XSLT_V.tar.xz"
(cd libxslt-$XSLT_V && LDFLAGS=$LDHP ./configure -q --prefix="$PREFIX" --enable-shared \
  --disable-static --without-python --without-crypto --with-libxml-prefix="$PREFIX" \
  >/dev/null && make -s -j"$JOBS" >/dev/null && make -s install >/dev/null)

echo "== perl $PERL_V"
tar xf "$DL/perl-$PERL_V.tar.gz"
(cd perl-$PERL_V && ./Configure -des -Dprefix="$PREFIX" -Duserelocatableinc \
  -Dman1dir=none -Dman3dir=none -Dinstallusrbinperl=undef -Ui_db \
  -Ud_crypt -Ud_crypt_r -Ui_crypt >/dev/null \
  && make -s -j"$JOBS" >/dev/null 2>&1 && make -s install >/dev/null 2>&1)
P="$PREFIX/bin/perl"
"$P" -V:userelocatableinc

SITELIB=$("$P" -MConfig -e 'print $Config{installsitelib}')
# Same in-memory stub as build-inside.sh (extracted from it, not duplicated).
sed -n "/^cat > \"\$SITELIB\/DB_File.pm\" <<'EOF'$/,/^EOF$/p" "$HERE/build-inside.sh" \
  | sed '1d;$d' > "$SITELIB/DB_File.pm"
grep -q 'Tie::StdHash' "$SITELIB/DB_File.pm"

echo "== cpanm + LaTeXML"
tar xf "$DL/App-cpanminus-$CPANM_V.tar.gz"
CPANM="$P App-cpanminus-$CPANM_V/bin/cpanm"
export PKG_CONFIG_PATH="$PREFIX/lib/pkgconfig" PATH="$PREFIX/bin:$PATH" ALIEN_INSTALL_TYPE=system
# macOS has no pkg-config by default; give the Makefile.PLs explicit flags.
XINC="-I$PREFIX/include/libxml2 -I$PREFIX/include"
$CPANM -q --notest Alien::Build
$CPANM -q --notest --configure-args="LIBS='-L$PREFIX/lib -lxml2' INC='$XINC'" XML::LibXML
$CPANM -q --notest --configure-args="LIBS='-L$PREFIX/lib -lxslt -lexslt -lxml2 -lz -lm' INC='$XINC'" XML::LibXSLT
$CPANM -q --notest --installdeps "$DL/LaTeXML-$LATEXML_V.tar.gz"
$CPANM -q "$DL/LaTeXML-$LATEXML_V.tar.gz"

echo "== relocatable install names and rpaths"
for lib in libxml2.2.dylib libxslt.1.dylib libexslt.0.dylib; do
  install_name_tool -id "@rpath/$lib" "$PREFIX/lib/$lib"
done
for lib in libxslt.1.dylib libexslt.0.dylib; do
  install_name_tool -add_rpath @loader_path "$PREFIX/lib/$lib"
done
fix_refs() { # rewrite absolute references to our dylibs into @rpath
  otool -L "$1" | awk 'NR>1 {print $1}' | grep "^$PREFIX/lib/" | while read -r dep; do
    install_name_tool -change "$dep" "@rpath/$(basename "$dep")" "$1"
  done
}
for f in "$PREFIX"/lib/lib*.dylib; do [ -L "$f" ] || fix_refs "$f"; done
find "$PREFIX/lib/perl5" -path '*/auto/XML/LibXML/LibXML.bundle' \
  -o -path '*/auto/XML/LibXSLT/LibXSLT.bundle' | while read -r b; do
  fix_refs "$b"
  install_name_tool -add_rpath '@loader_path/../../../../../../..' "$b"
done
find "$PREFIX" -type f \( -name '*.dylib' -o -name '*.bundle' -o -path '*/bin/perl*' \) \
  | while read -r f; do file "$f" | grep -q Mach-O && codesign -f -s - "$f"; done

echo "== prune"
find "$PREFIX" -name '*.pod' -delete
find "$PREFIX" -type d \( -name pod -o -name t \) -path '*/perl5/*' -prune -exec rm -rf {} +
rm -rf "$PREFIX/man" "$PREFIX/share/man" "$PREFIX/share/doc" "$PREFIX/share/gtk-doc" \
  "$PREFIX/include" "$PREFIX/lib/pkgconfig" "$PREFIX/lib/cmake" "$PREFIX"/lib/*.la
cat > "$PREFIX/latexmlc" <<'EOF'
#!/bin/sh
here=$(cd -- "$(dirname -- "$0")" && pwd -P)
exec "$here/bin/perl" "$here/bin/latexmlc" "$@"
EOF
chmod +x "$PREFIX/latexmlc"
"$P" -MXML::LibXML -MXML::LibXSLT -MLaTeXML -e 'printf "perl %vd libxml2 %s libxslt %s LaTeXML %s\n",
  $^V, XML::LibXML::LIBXML_DOTTED_VERSION(), XML::LibXSLT::LIBXSLT_DOTTED_VERSION(), $LaTeXML::VERSION' \
  | tee "$PREFIX/MANIFEST.txt"

echo "== pack"
cd "$WORK"
tar -cf - latexml-addon | xz -9 > "$OUT/latexml-addon-macos-$ARCH.tar.xz"
du -sm latexml-addon
ls -l "$OUT"
echo "work dir (delete when done): $WORK"
