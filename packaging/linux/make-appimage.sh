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
#      image formats, the SVG icon engine, TLS, the CUPS print support of
#      File › Print) and a qt.conf. What every
#      Linux desktop has (glibc, X11 and xcb, OpenGL, fontconfig,
#      FreeType, ...) stays the system's: linuxdeploy's exclude list, and
#      ours: GLib (below), libcups and the GPU drivers. The libraries
#      nothing in the AppDir needs then go, and the ELF files are
#      stripped.
#      The AppImage's entry point is packaging/linux/AppRun (mitcad#1170):
#      `MitCad.AppImage cli <arguments>` runs the bundled mitcad-cli,
#      anything else MitCad.
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
# /etc/pki/tls/certs/ca-bundle.crt and others).
# A system's openssl.cnf that this OpenSSL build cannot load (mitcad#1208:
# openSUSE's crypto policy sets rh-allow-sha1-signatures, an option only
# the distribution's patched OpenSSL knows) would leave Qt without TLS. So
# MitCad checks at start-up whether the system's configuration loads in
# the bundled libcrypto (app/update/OpenSslConfiguration.hpp): where it
# does, it is used as before, the distribution's policy applies; where it
# fails, MitCad loads the AppImage's own minimal configuration
# (packaging/linux/openssl.cnf, copied into usr/share/mitcad) in its place
# and logs "TLS: the system's OpenSSL configuration failed to load (<first
# error>); using the AppImage's own". Never a silent bypass of a policy
# that loads; the certificate folders stay the system's either way.
# tools/appimage-test.sh checks the pair, the folders, which OpenSSL the
# update check uses, and its TLS with a configuration that does not load
# (it falls back) and with one that does (no fallback).
#
# GLib (mitcad#1654): libglib-2.0, libgobject-2.0, libgio-2.0,
# libgmodule-2.0 and libgthread-2.0 are the system's, never bundled. Qt
# links libglib; Insert › Component from Web's web browser component, which
# MitCad loads at run time from the user's data folder, links libgobject
# and libgio through CEF, and its libraries find the system's (its RUNPATH
# is its own folder). With the build machine's GLib 2.80 in usr/lib, the
# bundled libglib was loaded at start-up and the system's libgobject came
# later on top of it: two GLib builds in one process, and on a system with
# GLib 2.86 or newer no browser at all ("libgobject-2.0.so.0: undefined
# symbol: g_string_copy", a function of 2.86 that the system's libgobject
# calls and the bundled libglib lacks). GLib keeps its interface from one
# version to the next, and every system the AppImage starts on (glibc 2.39,
# Ubuntu 24.04 and newer) has GLib 2.80 or newer, the version it is built
# against. tools/appimage-test.sh checks that the AppImage has none of them
# and loads the web browser component in it.
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
# GLib's libraries (see the header), which stay the system's.
GLIB_FAMILY="libglib-2.0 libgobject-2.0 libgio-2.0 libgmodule-2.0 libgthread-2.0"
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
excluded=('libcuda.so*' 'libamdhip64.so*')
# File › Print (mitcad#1089): Qt's CUPS print support plugin talks to the
# system's print service through the system's libcups, which stays the
# system's like the GPU drivers (a desktop without it prints to PDF files
# only: Qt then leaves the plugin unloaded).
excluded+=('libcups.so*')
# GLib (mitcad#1654, see the header): the system's, like glibc.
for library in $GLIB_FAMILY; do
  excluded+=("$library.so*")
done
for pattern in "${excluded[@]}"; do
  executables+=(--exclude-library "$pattern")
done
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
# linuxdeploy does not pass its --exclude-library patterns on to its Qt
# plugin, which deploys the libraries of Qt's plugins with its own copy of
# linuxdeploy's code: the plugin reads them from this variable (semicolons
# between the patterns). Without it the plugin bundled what linuxdeploy had
# left out (GLib through Qt's plugins, libcups through the print support
# plugin; mitcad#1654).
LINUXDEPLOY_EXCLUDED_LIBRARIES=$(IFS=';'; echo "${excluded[*]}")
export LINUXDEPLOY_EXCLUDED_LIBRARIES
linuxdeploy --appdir "$APPDIR" \
  "${executables[@]}" \
  --desktop-file "$APPDIR/usr/share/applications/mitcad.desktop" \
  --plugin qt
# None of them in the AppDir, whichever step deployed it.
for pattern in "${excluded[@]}"; do
  for library in "$APPDIR"/usr/lib/$pattern; do
    [ -e "$library" ] || [ -L "$library" ] || continue
    echo "the AppDir has ${library#"$APPDIR"/}, which stays the system's" >&2
    exit 1
  done
done
# linuxdeploy and its Qt plugin still copy the libraries an excluded one
# needs (libmount and libselinux of libgio, Avahi and GnuTLS of libcups,
# libXau of libxcb): libraries nothing in the AppDir needs, which only
# look as if MitCad used them. They go, again while a removal leaves
# another one unneeded; what is loaded by name (--library: OpenSSL's
# libssl, Open Image Denoise's device modules) stays.
kept=()
for ((i = 0; i < ${#executables[@]}; i++)); do
  [ "${executables[i]}" = --library ] && kept+=("$(basename "${executables[i + 1]}")")
done
while :; do
  needed=$(find "$APPDIR/usr" -type f -print0 | while IFS= read -r -d '' file; do
    if [ "$(head -c 4 "$file" | od -An -c | tr -d ' ')" = '177ELF' ]; then
      readelf -d "$file" | sed -n 's/.*(NEEDED).*\[\(.*\)\]$/\1/p'
    fi
  done | sort -u)
  unneeded=()
  for library in "$APPDIR"/usr/lib/*.so*; do
    name=$(basename "$library")
    if [ -f "$library" ] && ! grep -qxF "$name" <<< "$needed" && ! printf '%s\n' "${kept[@]}" | grep -qxF "$name"; then
      unneeded+=("$library")
    fi
  done
  [ ${#unneeded[@]} -gt 0 ] || break
  echo "Not needed in the AppDir: $(basename -a "${unneeded[@]}" | tr '\n' ' ')"
  rm -f "${unneeded[@]}"
done
# Qt's own translations as linuxdeploy copies them (all of Qt's languages):
# MitCad loads Qt's catalogue of its own interface languages from
# usr/share/mitcad/translations, which the install rules fill
# (cmake/AppImage.cmake, mitcad#1460).
rm -rf "$APPDIR/usr/translations"

# The AppImage's entry point (mitcad#1170): packaging/linux/AppRun, which
# runs the bundled mitcad-cli for `MitCad.AppImage cli <arguments>` and
# MitCad for anything else, in place of linuxdeploy's AppRun (a link to
# usr/bin/mitcad). Where linuxdeploy wrapped that link in a script that
# sets up its plugins' hooks first (AppRun.wrapped), the wrapped link is
# replaced, so the hooks still run.
if [ -e "$APPDIR/AppRun.wrapped" ] || [ -L "$APPDIR/AppRun.wrapped" ]; then
  ENTRY=$APPDIR/AppRun.wrapped
elif [ -L "$APPDIR/AppRun" ]; then
  ENTRY=$APPDIR/AppRun
else
  echo "linuxdeploy's AppRun is not the link to usr/bin/mitcad this script replaces" >&2
  exit 1
fi
rm -f "$ENTRY"
install -m 755 "$(dirname "$0")/AppRun" "$ENTRY"

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
  # The configuration MitCad loads where the system's does not (see the
  # header).
  install -D -m 644 "$(dirname "$0")/openssl.cnf" "$APPDIR/usr/share/mitcad/openssl.cnf"
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
