#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Pictures of weld symbols (mitcad#2350) for the owner: a one-off visual
# check, not a recurring test (no check picks it up). The example
# `weld_look` of mitcad-model-drawing lays out every basic symbol on the
# arrow side, the other side's, symmetrical, staggered, all around, field
# weld, tail, contours, finishes and blended toes, by ISO 2553 (upper half)
# and by AWS A2.4 (lower half), and writes them as an SVG sheet; this
# script runs it and turns it into a PNG file:
#   <dir>/welds.svg  <dir>/welds.png
# Builds only Rust (no OCCT); cargo's build goes to <dir>/target. Heavy
# enough to run under the machine's memory cap for cargo jobs.
#
# Usage: tools/look-drawing-weld.sh <dir>
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
DIR=${1:?usage: tools/look-drawing-weld.sh <dir>}
mkdir -p "$DIR"
DIR=$(cd "$DIR" && pwd)

(cd "$ROOT/core" &&
  CARGO_TARGET_DIR="$DIR/target" cargo run --offline --quiet \
    -p mitcad-model-drawing --example weld_look -- "$DIR/welds.svg" \
    "Weld symbols: $(git -C "$ROOT" rev-parse --short HEAD)")
rsvg-convert --dpi-x 254 --dpi-y 254 --background-color white \
  "$DIR/welds.svg" -o "$DIR/welds.png"
echo "ok   $DIR/welds.png"
