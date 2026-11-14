#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Pictures of a radius dimension's arrow and leader on a small fillet
# (mitcad#1770): a one-off visual check, not a recurring test (no check
# picks it up). The example `radius_leader_look` of mitcad-model-drawing
# lays out a radius of 1.5 mm at 2:1 on a fillet's arc (right and left of
# a turned part's axis, and a quarter arc the other way round), its text
# in eight directions round the arc's centre, and writes it as an SVG
# sheet; this script runs it in this tree and, given a git ref, in that
# ref's tree too (the example copied in), and turns both into PNG files:
#   <dir>/after.svg  <dir>/after.png
#   <dir>/before.svg <dir>/before.png   (with <before-ref>)
# Builds only Rust (no OCCT); cargo's builds go to <dir>/target-after and
# <dir>/target-before, the ref's sources to <dir>/before-src. Heavy enough
# to run under the machine's memory cap for cargo jobs.
#
# Usage: tools/look-drawing-radius-leader.sh <dir> [<before-ref>]
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
DIR=${1:?usage: tools/look-drawing-radius-leader.sh <dir> [<before-ref>]}
REF=${2:-}
mkdir -p "$DIR"
DIR=$(cd "$DIR" && pwd)
EXAMPLE=$ROOT/core/model-drawing/examples/radius_leader_look.rs

# picture tree name title: the example run in <tree>/core, its SVG and a
# PNG of it (10 pixels a millimetre).
picture() {
  local tree=$1 name=$2 title=$3
  (cd "$tree/core" &&
    CARGO_TARGET_DIR="$DIR/target-$name" cargo run --offline --quiet \
      -p mitcad-model-drawing --example radius_leader_look -- "$DIR/$name.svg" "$title")
  rsvg-convert --dpi-x 254 --dpi-y 254 --background-color white \
    "$DIR/$name.svg" -o "$DIR/$name.png"
  echo "ok   $DIR/$name.png"
}

picture "$ROOT" after "After: $(git -C "$ROOT" rev-parse --short HEAD)"

if [ -n "$REF" ]; then
  mkdir -p "$DIR/before-src"
  git -C "$ROOT" archive "$REF" core tools/testrun third_party rust-toolchain.toml | tar -x -C "$DIR/before-src"
  mkdir -p "$DIR/before-src/core/model-drawing/examples"
  cp -f "$EXAMPLE" "$DIR/before-src/core/model-drawing/examples/"
  picture "$DIR/before-src" before "Before: $(git -C "$ROOT" rev-parse --short "$REF")"
fi
