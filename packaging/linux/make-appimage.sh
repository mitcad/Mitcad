#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Makes Mitcad's Linux AppImage from a build; the CMake target appimage
# (cmake/AppImage.cmake) runs it:
#   1. The build's install rules put Mitcad into an AppDir (usr/bin, the
#      desktop file, the icons, the licences in usr/share/doc/mitcad).
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

# linuxdeploy finds the libraries where the build's executable finds them
# (its RUNPATH: Qt's and vcpkg's folders).
runpath=$(readelf -d "$APP" | sed -n 's/.*(RUNPATH).*\[\(.*\)\]$/\1/p')
export LD_LIBRARY_PATH=$runpath${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}
export PATH=$TOOLS/bin:$PATH
export QMAKE
linuxdeploy --appdir "$APPDIR" \
  --executable "$APPDIR/usr/bin/mitcad" \
  --executable "$APPDIR/usr/bin/mitcad-cli" \
  --desktop-file "$APPDIR/usr/share/applications/mitcad.desktop" \
  --plugin qt
# Qt's own translations: Mitcad's interface is English and loads none.
rm -rf "$APPDIR/usr/translations"

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
