#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Makes Mitcad's Linux AppImage from a build; the CMake target appimage
# (cmake/AppImage.cmake) runs it:
#   1. The build's install rules put Mitcad into an AppDir (usr/bin, the
#      desktop file, the icons, the licences in usr/share/doc/mitcad, the
#      render worker mitcad-render).
#   2. linuxdeploy copies the libraries the executables need into usr/lib
#      (Qt, OCCT and the rest from vcpkg, ICU, ...) and points their
#      RUNPATH there; its Qt plugin adds Qt's plugins (the X11 platform,
#      image formats, the SVG icon engine, TLS) and a qt.conf. What every
#      Linux desktop has (glibc, X11 and xcb, OpenGL, fontconfig,
#      FreeType, ...) stays the system's: linuxdeploy's exclude list. Then
#      the ELF files are stripped.
#   3. appimagetool packs the AppDir behind the AppImage runtime of
#      tools/dev-env/install-appimage-tools.sh (it downloads nothing).
#
# OpenSSL (mitcad#954): the AppImage carries libssl.so.3 and libcrypto.so.3
# of one build, the one the executables link (vcpkg's). The render worker
# links libcrypto (through minizip-ng, which OpenColorIO uses), so
# linuxdeploy bundles it; Qt's TLS plugin links no OpenSSL but loads
# libssl.so.3 and libcrypto.so.3 by name at run time, and libQt6Core's
# $ORIGIN RUNPATH finds them in usr/lib before the system's. With only
# libcrypto bundled, the plugin took the system's libssl, which then ran on
# the AppImage's libcrypto: two OpenSSL builds in one process, and no TLS
# at all ("TLS initialization failed") where the system's libssl does not
# work with another build's libcrypto. Bundling the pair rather than none:
# the update check then does not depend on which OpenSSL a distribution
# ships and how it patches it, and dropping libcrypto would need
# minizip-ng and the render libraries built again without it.
# OpenSSL compiles in the folders it looks in at run time. OPENSSLDIR is
# vcpkg's /etc/ssl, the system's: its openssl.cnf (the distribution's
# crypto policy) and certificate folder are used, as by the system's
# OpenSSL. ENGINESDIR and MODULESDIR follow the install prefix (OpenSSL's
# Configure has no option for them), which is vcpkg's install folder on
# the build machine; the copy in usr/lib gets them rewritten to a folder
# that does not exist (/nonexistent/engines-3, /nonexistent/ossl-modules,
# of the same length with NULs after them), so no engine or provider
# module of another OpenSSL build is ever loaded into it. Nothing needs
# one: TLS uses the built-in default provider. Qt reads the system's CA
# certificates itself (/etc/ssl/certs, Fedora's
# /etc/pki/tls/certs/ca-bundle.crt and others). tools/appimage-test.sh
# checks the pair, the folders and which OpenSSL the update check uses.
#
# Usage: packaging/linux/make-appimage.sh <build dir> <built mitcad> <AppImage>
# Environment: MITCAD_APPIMAGE_TOOLS (default ~/appimage-tools), QMAKE
# (Qt's qmake, which tells the Qt plugin where Qt is), CMAKE.
set -euo pipefail

BUILD=$1
APP=$2
OUT=$3
TOOLS=${MITCAD_APPIMAGE_TOOLS:-$HOME/appimage-tools}
CMAKE=${CMAKE:-cmake}
WORK=$BUILD/appimage
APPDIR=$WORK/AppDir
RUNTIME=$TOOLS/runtime-x86_64

for tool in linuxdeploy linuxdeploy-plugin-qt appimagetool; do
  if [ ! -x "$TOOLS/bin/$tool" ]; then
    echo "No $tool in $TOOLS/bin: run tools/dev-env/install-appimage-tools.sh" >&2
    exit 1
  fi
done
[ -f "$RUNTIME" ] || { echo "No AppImage runtime $RUNTIME: run tools/dev-env/install-appimage-tools.sh" >&2; exit 1; }
[ -n "${QMAKE:-}" ] || { echo "QMAKE (Qt's qmake) is not set" >&2; exit 1; }

rm -rf "$APPDIR"
mkdir -p "$WORK"
"$CMAKE" --install "$BUILD" --prefix "$APPDIR/usr" > "$WORK/install.log"

# linuxdeploy finds the libraries where the build's executables find them
# (their RUNPATH: Qt's and vcpkg's folders, and the render worker's).
runpath() { readelf -d "$1" | sed -n 's/.*(RUNPATH).*\[\(.*\)\]$/\1/p'; }
libraries=$(runpath "$APP")
# Every executable the install rules put in usr/bin (mitcad, mitcad-cli,
# the render worker, the assistant's launcher, ...): linuxdeploy gives
# each its $ORIGIN RUNPATH, so none of them can load a system's Qt or
# other libraries in place of the AppImage's (mitcad#789).
executables=()
for file in "$APPDIR"/usr/bin/*; do
  [ -f "$file" ] && [ ! -L "$file" ] && [ "$(head -c 4 "$file" | od -An -c | tr -d ' ')" = '177ELF' ] &&
    executables+=(--executable "$file")
done
# The render worker (docs/rendering.md), which every build has, and what
# it loads without linking it: Open Image Denoise's device modules, next
# to its core library.
WORKER=$(dirname "$APP")/mitcad-render
[ -f "$APPDIR/usr/bin/mitcad-render" ] || { echo "no render worker in $APPDIR/usr/bin" >&2; exit 1; }
libraries=$libraries:$(runpath "$WORKER")
oidn=$(LD_LIBRARY_PATH=$libraries ldd "$WORKER" | sed -n 's/.*libOpenImageDenoise_core[^ ]* => \([^ ]*\) .*/\1/p')
[ -n "$oidn" ] || { echo "mitcad-render does not link Open Image Denoise's core library" >&2; exit 1; }
# The GPU devices' modules too, when Open Image Denoise has them
# (build-cycles.sh --oidn-gpu, mitcad#50); they load the GPU's driver at
# run time, which stays the system's.
for module in "$(dirname "$oidn")"/libOpenImageDenoise_device_{cpu,cuda,hip,sycl}.so*; do
  [ -e "$module" ] && [ ! -L "$module" ] && executables+=(--library "$module")
done
executables+=(--exclude-library 'libcuda.so*' --exclude-library 'libamdhip64.so*')
# OpenSSL (see the header): the libssl.so.3 next to the libcrypto.so.3 that
# the executables link, so that the AppImage has both or neither.
crypto=""
for program in "$APP" "$WORKER" "$APPDIR"/usr/bin/*; do
  [ -f "$program" ] && [ ! -L "$program" ] || continue
  crypto=$({ LD_LIBRARY_PATH=$libraries ldd "$program" 2> /dev/null || true; } |
    sed -n 's/^[[:space:]]*libcrypto\.so\.3 => \(\/[^ ]*\) .*/\1/p' | head -1) || true
  [ -n "$crypto" ] && break
done
if [ -n "$crypto" ]; then
  ssl=$(dirname "$crypto")/libssl.so.3
  [ -f "$ssl" ] || { echo "no libssl.so.3 next to $crypto: Qt's TLS would mix the system's libssl with it" >&2; exit 1; }
  executables+=(--library "$ssl")
fi
export LD_LIBRARY_PATH=$libraries${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}
export PATH=$TOOLS/bin:$PATH
export QMAKE
linuxdeploy --appdir "$APPDIR" \
  "${executables[@]}" \
  --desktop-file "$APPDIR/usr/share/applications/mitcad.desktop" \
  --plugin qt
# Qt's own translations: Mitcad's interface is English and loads none.
rm -rf "$APPDIR/usr/translations"

# OpenSSL's run-time folders in the bundled libcrypto (see the header).
# openssl_dir name file: the folder that OpenSSL's version text names (the
# same string as the one it looks in).
openssl_dir() { { grep -aoE "$1: \"[^\"]*\"" "$2" || true; } | head -1 | sed -E "s/^$1: \"(.*)\"\$/\1/"; }
LIBCRYPTO=$APPDIR/usr/lib/libcrypto.so.3
if [ -e "$LIBCRYPTO" ] || [ -e "$APPDIR/usr/lib/libssl.so.3" ]; then
  [ -f "$LIBCRYPTO" ] && [ -f "$APPDIR/usr/lib/libssl.so.3" ] ||
    { echo "the AppDir has only one of libssl.so.3 and libcrypto.so.3" >&2; exit 1; }
  for name in ENGINESDIR MODULESDIR; do
    old=$(openssl_dir "$name" "$LIBCRYPTO")
    [ -n "$old" ] || continue
    new=/nonexistent/${old##*/}
    [ "$old" = "$new" ] && continue
    [ "${#new}" -le "${#old}" ] || { echo "$name $old is shorter than $new" >&2; exit 1; }
    # Each copy of the folder's C string, the plain one and the version
    # text's (followed by its closing quote), as the new folder padded with
    # NULs to the same length.
    OLD=$old NEW=$new perl -0777 -pi -e \
      'BEGIN { ($o, $n) = ($ENV{OLD}, $ENV{NEW}); } s/\Q$o\E("?)/$n . $1 . ("\0" x (length($o) - length($n)))/ge' \
      "$LIBCRYPTO"
    if grep -qaF "$old" "$LIBCRYPTO" || [ "$(openssl_dir "$name" "$LIBCRYPTO")" != "$new" ]; then
      echo "could not rewrite $name of $LIBCRYPTO" >&2
      exit 1
    fi
    echo "OpenSSL's $name: $new (was $old)"
  done
  case $(openssl_dir OPENSSLDIR "$LIBCRYPTO") in
    /etc/* | /usr/*) ;;
    *) echo "the bundled libcrypto's OPENSSLDIR is not a system folder" >&2; exit 1 ;;
  esac
fi

# linuxdeploy does not strip a file whose RUNPATH starts with $ORIGIN (a
# precaution for old versions of strip), which is every file it deployed:
# strip them here (the debug information of a debug build's executables
# and libraries; the symbols a dynamic linker needs stay).
while IFS= read -r -d '' file; do
  if [ "$(head -c 4 "$file" | od -An -c | tr -d ' ')" = '177ELF' ] && readelf -S "$file" | grep -q ' \.symtab '; then
    strip --strip-unneeded "$file"
  fi
done < <(find "$APPDIR/usr" -type f -print0)

# There is no AppStream metadata to check, and no update information of
# AppImage's tools is written: Mitcad updates itself (docs/updates.md).
rm -f "$OUT.part"
ARCH=x86_64 appimagetool --no-appstream --runtime-file "$RUNTIME" "$APPDIR" "$OUT.part"
mv -f "$OUT.part" "$OUT"
echo "Made $OUT ($(du -h "$OUT" | cut -f1))"
