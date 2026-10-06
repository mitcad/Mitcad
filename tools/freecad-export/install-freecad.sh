#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Installs the official FreeCAD AppImages that the reference models are made
# with, into ~/freecad/<version>/ of the isolated Linux distro (never on a
# developer's own system: FreeCAD is third-party code).
#
# Each AppImage comes from FreeCAD's GitHub releases with the SHA-256 file
# published next to it; the image is checked against it and extracted with
# --appimage-extract (no FUSE needed). The command-line FreeCAD then runs
# headless: ~/freecad/<version>/squashfs-root/usr/bin/FreeCADCmd (or
# freecadcmd), as tools/freecad-export/run_all.sh finds it.
#
# Usage: tools/freecad-export/install-freecad.sh [version...]
#   versions: 0.21.2 1.0.2 1.1.4 (default: all three)
set -euo pipefail

ROOT=${FREECAD_ROOT:-$HOME/freecad}
RELEASES=https://github.com/FreeCAD/FreeCAD/releases/download

# The release asset of a version (their names differ between releases).
asset() {
  case $1 in
    0.21.2) echo "FreeCAD-0.21.2-Linux-x86_64.AppImage" ;;
    1.0.2) echo "FreeCAD_1.0.2-conda-Linux-x86_64-py311.AppImage" ;;
    1.1.4) echo "FreeCAD_1.1.4-Linux-x86_64-py311.AppImage" ;;
    *) echo "unknown FreeCAD version $1" >&2; return 1 ;;
  esac
}

versions=("$@")
[ ${#versions[@]} -gt 0 ] || versions=(0.21.2 1.0.2 1.1.4)

for version in "${versions[@]}"; do
  file=$(asset "$version")
  dir="$ROOT/$version"
  mkdir -p "$dir"
  cd "$dir"
  if [ -x squashfs-root/AppRun ]; then
    echo "FreeCAD $version: already installed in $dir"
    continue
  fi
  echo "FreeCAD $version: downloading $file"
  curl -fsSL -o "$file.SHA256.txt" "$RELEASES/$version/$file-SHA256.txt"
  curl -fsSL --retry 3 -o "$file" "$RELEASES/$version/$file"
  expected=$(awk '{print $1; exit}' "$file.SHA256.txt")
  actual=$(sha256sum "$file" | awk '{print $1}')
  if [ -z "$expected" ] || [ "$expected" != "$actual" ]; then
    echo "FreeCAD $version: SHA-256 mismatch (published '$expected', got '$actual')" >&2
    rm -f "$file"
    exit 1
  fi
  echo "FreeCAD $version: SHA-256 $actual matches the release"
  chmod +x "$file"
  "./$file" --appimage-extract > /dev/null
  rm -f "$file"
  echo "FreeCAD $version: extracted to $dir/squashfs-root"
done
