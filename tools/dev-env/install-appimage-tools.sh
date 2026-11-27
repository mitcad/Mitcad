#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Installs the tools that make Mitcad's Linux AppImage (the CMake target
# appimage, cmake/AppImage.cmake) into ~/appimage-tools
# ($MITCAD_APPIMAGE_TOOLS) of the isolated Linux build environment:
# linuxdeploy and its Qt plugin, which gather the libraries and Qt's
# plugins into the AppDir, and appimagetool, which packs it. Build tools;
# of them only the AppImage runtime ends up in the AppImage (below).
#
# Each comes from its project's GitHub releases, pinned to one asset and
# checked against the SHA-256 digest GitHub publishes for it, and is
# extracted with --appimage-extract, so FUSE is not needed;
# ~/appimage-tools/bin gets a command for each. The AppImage runtime (the
# program at the start of every AppImage, which mounts or extracts the rest)
# is the one appimagetool's own AppImage starts with, with the sections
# that hold that AppImage's update information and signature cleared:
# appimagetool would otherwise download one when it packs.
#
# Usage: tools/dev-env/install-appimage-tools.sh
set -euo pipefail

DIR=${MITCAD_APPIMAGE_TOOLS:-$HOME/appimage-tools}

# name, release asset, SHA-256 published with it, for this machine's
# architecture (x86-64, or aarch64 for Linux on 64-bit ARM: the same
# releases' aarch64 assets). linuxdeploy-plugin-qt has published digests
# only on its continuous build (of 22.8.2026): when that is rebuilt, the
# check fails and the digests here are to be updated.
ARCH=$(uname -m)
case "$ARCH" in
  x86_64)
    TOOLS=(
      linuxdeploy
      https://github.com/linuxdeploy/linuxdeploy/releases/download/1-alpha-20251107-1/linuxdeploy-x86_64.AppImage
      c20cd71e3a4e3b80c3483cef793cda3f4e990aca14014d23c544ca3ce1270b4d
      linuxdeploy-plugin-qt
      https://github.com/linuxdeploy/linuxdeploy-plugin-qt/releases/download/continuous/linuxdeploy-plugin-qt-x86_64.AppImage
      cfc1055b2b9dbc08412b579f20990b7b41a17b61beaa5847dc9477c96c9e9617
      appimagetool
      https://github.com/AppImage/appimagetool/releases/download/1.9.1/appimagetool-x86_64.AppImage
      ed4ce84f0d9caff66f50bcca6ff6f35aae54ce8135408b3fa33abfc3cb384eb0
    )
    ;;
  aarch64)
    TOOLS=(
      linuxdeploy
      https://github.com/linuxdeploy/linuxdeploy/releases/download/1-alpha-20251107-1/linuxdeploy-aarch64.AppImage
      620095110d693282b8ebeb244a95b5e911cf8f65f76c88b4b47d16ae6346fcff
      linuxdeploy-plugin-qt
      https://github.com/linuxdeploy/linuxdeploy-plugin-qt/releases/download/continuous/linuxdeploy-plugin-qt-aarch64.AppImage
      d5b5a9020065cefda8c7af7c9500f686b23640eac50c88201a81752c6ee2e49c
      appimagetool
      https://github.com/AppImage/appimagetool/releases/download/1.9.1/appimagetool-aarch64.AppImage
      f0837e7448a0c1e4e650a93bb3e85802546e60654ef287576f46c71c126a9158
    )
    ;;
  *)
    echo "install-appimage-tools.sh: no AppImage tools for $ARCH (x86_64 and aarch64 only)" >&2
    exit 1
    ;;
esac

mkdir -p "$DIR/bin"
for ((i = 0; i < ${#TOOLS[@]}; i += 3)); do
  name=${TOOLS[i]} url=${TOOLS[i + 1]} sha256=${TOOLS[i + 2]}
  file=$DIR/$name.AppImage
  if [ -x "$DIR/$name/squashfs-root/AppRun" ] && [ "$(cat "$DIR/$name/sha256" 2> /dev/null)" = "$sha256" ]; then
    echo "$name: already installed"
  else
    echo "$name: downloading $url"
    curl -fsSL --retry 3 -o "$file.part" "$url"
    actual=$(sha256sum "$file.part" | cut -d' ' -f1)
    if [ "$actual" != "$sha256" ]; then
      rm -f "$file.part"
      echo "$name: SHA-256 mismatch (published $sha256, got $actual)" >&2
      exit 1
    fi
    echo "$name: SHA-256 $actual matches the release"
    mv "$file.part" "$file"
    chmod +x "$file"
    rm -rf "$DIR/$name"
    mkdir -p "$DIR/$name"
    (cd "$DIR/$name" && "$file" --appimage-extract > /dev/null)
    echo "$sha256" > "$DIR/$name/sha256"
  fi
  printf '#!/bin/sh\nexec "%s" "$@"\n' "$DIR/$name/squashfs-root/AppRun" > "$DIR/bin/$name"
  chmod +x "$DIR/bin/$name"
done

# The runtime: appimagetool's AppImage up to its file system, with the
# sections of its update information and signature zeroed.
image=$DIR/appimagetool.AppImage
runtime=$DIR/runtime-$ARCH
offset=$("$image" --appimage-offset)
head -c "$offset" "$image" > "$runtime.part"
for section in .upd_info .sha256_sig .sig_key .digest_md5; do
  read -r start size < <(readelf -SW "$runtime.part" |
    awk -v name="$section" '$0 ~ " " name " " { for (i = 1; i <= NF; i++) if ($i == name) { print $(i + 3), $(i + 4); exit } }')
  if [ -z "${start:-}" ]; then
    echo "runtime: no section $section" >&2
    exit 1
  fi
  dd if=/dev/zero of="$runtime.part" bs=1 seek=$((16#$start)) count=$((16#$size)) conv=notrunc status=none
done
chmod +x "$runtime.part"
mv "$runtime.part" "$runtime"
echo "runtime: $runtime ($offset bytes, SHA-256 $(sha256sum "$runtime" | cut -d' ' -f1))"
echo "Done: $DIR/bin"
