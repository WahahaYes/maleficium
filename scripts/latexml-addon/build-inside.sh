#!/bin/sh
# Runs INSIDE the build container (see build-linux.sh). Builds a relocatable
# LaTeXML tree at $PREFIX and packs it into /out.
#   perl       built with -Duserelocatableinc, so @INC follows the binary;
#              crypt() left out so it needs no libcrypt.so.1 (absent on
#              some libxcrypt-only distros)
#   libxml2    shared, in $PREFIX/lib, no ICU/zlib/lzma/http
#   libxslt    shared, in $PREFIX/lib, no libgcrypt
#              Shared, not static: XML::LibXML and XML::LibXSLT must share
#              ONE libxml2. Two static copies break pointer-compared globals
#              (xmlStringTextNoenc), which turned LaTeXML's HTML5 doctype
#              into "&lt;!DOCTYPE html&gt;". RPATHs are $ORIGIN-relative.
#   DB_File    in-memory stub (LaTeXML ties it only for its graphics cache
#              and --dbfile; this keeps Berkeley DB, Sleepycat or AGPL-3
#              depending on version, out of the tree)
#   ImageMagick dropped: no Image::Magick / Graphics::Magick
# CPAN dependencies resolve to current CPAN at build time (not yet pinned).
# POSIX sh.
set -eu
PREFIX=/opt/latexml-addon
DL=/dl
JOBS=$(nproc)

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

export DEBIAN_FRONTEND=noninteractive
APT="-o Acquire::http::Timeout=30 -o Acquire::Retries=3"
apt-get $APT update -qq
apt-get $APT install -y -qq --no-install-recommends \
  build-essential ca-certificates curl pkg-config xz-utils zstd file \
  zlib1g-dev patchelf >/dev/null
# zlib1g-dev only because XML::LibXSLT's Makefile.PL appends -lz regardless;
# libz.so.1 is present on every glibc distro.

mkdir -p "$DL" /build
fetch() { # url sha
  f="$DL/$(basename "$1")"
  if [ ! -f "$f" ] || ! echo "$2  $f" | sha256sum -c - >/dev/null 2>&1; then
    curl -fsSL -o "$f" "$1"
  fi
  echo "$2  $f" | sha256sum -c -
}
fetch https://www.cpan.org/src/5.0/perl-$PERL_V.tar.gz $PERL_SHA
fetch https://download.gnome.org/sources/libxml2/2.13/libxml2-$XML2_V.tar.xz $XML2_SHA
fetch https://download.gnome.org/sources/libxslt/1.1/libxslt-$XSLT_V.tar.xz $XSLT_SHA
fetch https://cpan.metacpan.org/authors/id/M/MI/MIYAGAWA/App-cpanminus-$CPANM_V.tar.gz $CPANM_SHA
fetch https://cpan.metacpan.org/authors/id/B/BR/BRMILLER/LaTeXML-$LATEXML_V.tar.gz $LATEXML_SHA

cd /build
echo "== libxml2 $XML2_V (shared)"
tar xf "$DL/libxml2-$XML2_V.tar.xz"
(cd libxml2-$XML2_V && ./configure -q --prefix=$PREFIX --enable-shared --disable-static \
  --without-python --without-icu --without-lzma --without-zlib \
  --without-http --without-ftp --without-debug >/dev/null && make -s -j"$JOBS" >/dev/null \
  && make -s install >/dev/null)

echo "== libxslt $XSLT_V (shared)"
tar xf "$DL/libxslt-$XSLT_V.tar.xz"
(cd libxslt-$XSLT_V && ./configure -q --prefix=$PREFIX --enable-shared --disable-static \
  --without-python --without-crypto --with-libxml-prefix=$PREFIX >/dev/null \
  && make -s -j"$JOBS" >/dev/null && make -s install >/dev/null)

echo "== perl $PERL_V (relocatable @INC)"
tar xf "$DL/perl-$PERL_V.tar.gz"
(cd perl-$PERL_V && ./Configure -des -Dprefix=$PREFIX -Duserelocatableinc \
  -Dman1dir=none -Dman3dir=none -Dinstallusrbinperl=undef -Ui_db \
  -Ud_crypt -Ud_crypt_r -Ui_crypt >/dev/null \
  && make -s -j"$JOBS" >/dev/null 2>&1 && make -s install >/dev/null 2>&1)
P=$PREFIX/bin/perl
$P -V:userelocatableinc

echo "== DB_File stub"
SITELIB=$($P -MConfig -e 'print $Config{installsitelib}')
cat > "$SITELIB/DB_File.pm" <<'EOF'
package DB_File;
# Stub installed by the Maleficium LaTeXML add-on build. LaTeXML ties
# DB_File for its graphics cache (LaTeXML.cache) and for --dbfile. Both
# become plain in-memory hashes: nothing persists between runs and no
# LaTeXML.cache lands in the output. Keeps Berkeley DB out of the tree.
use strict;
use Fcntl qw(O_RDONLY O_RDWR O_CREAT);
use Tie::Hash ();
use Exporter 'import';
our @ISA     = ('Tie::StdHash');
our @EXPORT  = qw(O_RDONLY O_RDWR O_CREAT);
our $VERSION = '1.859';
sub TIEHASH { my ($class) = @_; return bless {}, $class }
1;
EOF

echo "== cpanm + LaTeXML deps"
tar xf "$DL/App-cpanminus-$CPANM_V.tar.gz"
CPANM="$P App-cpanminus-$CPANM_V/bin/cpanm"
export PKG_CONFIG_PATH=$PREFIX/lib/pkgconfig PATH=$PREFIX/bin:$PATH
export ALIEN_INSTALL_TYPE=system
$CPANM -q --notest Alien::Libxml2 XML::LibXML XML::LibXSLT
$CPANM -q --notest --installdeps "$DL/LaTeXML-$LATEXML_V.tar.gz"
echo "== LaTeXML $LATEXML_V (with its test suite)"
$CPANM -q "$DL/LaTeXML-$LATEXML_V.tar.gz" || {
  echo "LaTeXML tests failed; see build.log"; cat ~/.cpanm/build.log | tail -60; exit 1; }

echo "== relocatable RPATHs"
for so in $PREFIX/lib/libxslt.so.* $PREFIX/lib/libexslt.so.*; do
  [ -L "$so" ] || patchelf --set-rpath '$ORIGIN' "$so"
done
# auto/XML/<Mod>/<Mod>.so sits 7 levels below $PREFIX/lib
find $PREFIX/lib/perl5 \( -path '*/auto/XML/LibXML/LibXML.so' \
  -o -path '*/auto/XML/LibXSLT/LibXSLT.so' \) \
  -exec patchelf --set-rpath '$ORIGIN/../../../../../../..' {} \;

echo "== manifest"
$P -MXML::LibXML -MXML::LibXSLT -MLaTeXML -e '
  printf "perl %vd\nXML::LibXML %s (libxml2 %s)\nXML::LibXSLT %s (libxslt %s)\nLaTeXML %s\n",
    $^V, $XML::LibXML::VERSION, XML::LibXML::LIBXML_DOTTED_VERSION(),
    $XML::LibXSLT::VERSION, XML::LibXSLT::LIBXSLT_DOTTED_VERSION(), $LaTeXML::VERSION' \
  > $PREFIX/MANIFEST.txt
$P -MExtUtils::Installed -e '
  my $i = ExtUtils::Installed->new(skip_cwd => 1);
  printf "%s %s\n", $_, ($i->version($_) // "?") for $i->modules' >> $PREFIX/MANIFEST.txt
cat $PREFIX/MANIFEST.txt
du -sm $PREFIX | sed 's/^/raw MB: /'

echo "== prune (docs, pod, tests, static libs) and strip"
find $PREFIX -name '*.pod' -delete
find $PREFIX -type d \( -name pod -o -name t \) -path '*/perl5/*' -prune -exec rm -rf {} +
rm -rf $PREFIX/man $PREFIX/share/man
find $PREFIX -name '*.a' -delete
rm -rf $PREFIX/include $PREFIX/share/doc $PREFIX/share/gtk-doc $PREFIX/share/aclocal \
  $PREFIX/lib/pkgconfig $PREFIX/lib/cmake $PREFIX/lib/*.la $PREFIX/lib/xsltConf.sh \
  $PREFIX/lib/libxslt-plugins $PREFIX/bin/xml2-config $PREFIX/bin/xslt-config
find $PREFIX -type f \( -name '*.so' -o -name '*.so.*' -o -name perl -o -name "perl$PERL_V" \
  -o -name xmllint -o -name xsltproc -o -name xmlcatalog \) \
  -exec strip --strip-unneeded {} + 2>/dev/null || true
du -sm $PREFIX | sed 's/^/pruned MB: /'

echo "== launcher"
cat > $PREFIX/latexmlc <<'EOF'
#!/bin/sh
# Relocatable launcher: run LaTeXML with the perl that sits next to it.
here=$(cd -- "$(dirname -- "$0")" && pwd -P)
exec "$here/bin/perl" "$here/bin/latexmlc" "$@"
EOF
chmod +x $PREFIX/latexmlc
# Bindings that shadow packages known to hang LaTeXML (put first on --path).
cp -R /src/stubs $PREFIX/stubs

echo "== pack"
cd /opt
tar -cf /out/latexml-addon-linux-x86_64.tar latexml-addon
zstd -q -19 -f /out/latexml-addon-linux-x86_64.tar -o /out/latexml-addon-linux-x86_64.tar.zst
xz -9 -k -f /out/latexml-addon-linux-x86_64.tar
gzip -9 -k -f /out/latexml-addon-linux-x86_64.tar
ls -l /out
chown -R "${HOST_UID:-0}:${HOST_GID:-0}" /out
