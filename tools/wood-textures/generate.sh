#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Makes the texture sets of the built-in woods and panels (mitcad#2186):
# builds generate.rs with rustc, writes its PPM and PGM images into a work
# folder (default build/wood-textures) and turns them into the JPEG files
# core/model-base/textures/<id>-{color,normal,roughness}.jpg that the
# model's library embeds. Needs rustc and ImageMagick (magick or convert).
#
#   tools/wood-textures/generate.sh [work folder] [look id ...]
set -euo pipefail
repo=$(cd "$(dirname "$0")/../.." && pwd)
work=${1:-$repo/build/wood-textures}
shift || true
out=$repo/core/model-base/textures
mkdir -p "$work" "$out"
if command -v magick > /dev/null; then
  convert=(magick)
else
  convert=(convert)
fi
rustc -O --edition 2021 -o "$work/generate" "$repo/tools/wood-textures/generate.rs"
"$work/generate" "$work" "$@" | while read -r id; do
  # The base colour and the normal map as colour JPEG, the roughness as a
  # grey one; no metadata, so that the files depend only on the pixels.
  "${convert[@]}" "$work/$id-color.ppm" -strip -quality 85 -sampling-factor 2x2 "$out/$id-color.jpg"
  "${convert[@]}" "$work/$id-normal.ppm" -strip -quality 85 -sampling-factor 1x1 "$out/$id-normal.jpg"
  "${convert[@]}" "$work/$id-roughness.pgm" -strip -quality 85 "$out/$id-roughness.jpg"
  echo "$id"
done
