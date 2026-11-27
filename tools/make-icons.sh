#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Renders the application icon (app/branding/mitcad.svg, mitcad-small.svg)
# into the PNG files the application loads and the Windows icon mitcad.ico
# (mitcad.exe and the installer). The results are committed; run this after
# changing an SVG. Needs rsvg-convert and ImageMagick (install-packages.sh).
#
# Usage: tools/make-icons.sh
set -euo pipefail
cd "$(dirname "$0")/../app/branding"

small=(16 20 24 32 40 48)
large=(64 96 128 256)
mkdir -p png
pngs=()
for size in "${small[@]}"; do
  rsvg-convert -w "$size" -h "$size" mitcad-small.svg -o "png/mitcad-$size.png"
  pngs+=("png/mitcad-$size.png")
done
for size in "${large[@]}"; do
  rsvg-convert -w "$size" -h "$size" mitcad.svg -o "png/mitcad-$size.png"
  pngs+=("png/mitcad-$size.png")
done
# Keep each size's own image; no resampling, no metadata (reproducible).
convert "${pngs[@]}" -strip mitcad.ico
echo "Wrote app/branding/png/*.png and app/branding/mitcad.ico"
