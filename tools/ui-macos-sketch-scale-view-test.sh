#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch/SketchController.cpp app/sketch/SketchPreferences.cpp app/OcctViewer.cpp
# check-all sources: app/framework/TestDriverOrbitCenter.cpp core/sketch core/solver tools/cli
# check-all sources: tools/ui-scripts/sketch-scale-view.mitcad-ui tools/ui-scripts/sketch-scale-view.json
# The view follows a sketch its first dimension scales (mitcad#1948,
# GitHub #400) in the Qt frontend on macOS: the shared steps
# tools/ui-scripts/sketch-scale-view.mitcad-ui on the design mitcad-cli makes
# (tools/ui-scripts/sketch-scale-view.json): a diameter of 50 scales the
# sketch by 1.25, then (undone) by 0.8, and the dimensioned circle's centre
# stays at its place in the window within a pixel, at the same zoom. As
# tools/ui-sketch-scale-view-test.sh (Linux) and
# tools/ui-windows-sketch-scale-view-test.ps1 (Windows).
# Usage: bash tools/ui-macos-sketch-scale-view-test.sh <app> <mitcad-cli> [out directory]
set -u
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=tools/ui-macos-lib.sh
. "$here/ui-macos-lib.sh"
if [ $# -lt 2 ] || [ $# -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [out directory]" >&2
  exit 2
fi
cli=$2
ui_init "$1" "${3:-ui-macos-sketch-scale-view}"
export MITCAD_TEST_WORK=$UI_OUT/files
export MITCAD_TEST_STEP_MS=100
mkdir -p "$MITCAD_TEST_WORK"
"$cli" run "$here/ui-scripts/sketch-scale-view.json" --save "$MITCAD_TEST_WORK/circles.mitcad" \
  > "$UI_OUT/setup.log" 2>&1 || { cat "$UI_OUT/setup.log" >&2; exit 1; }
UI_TIMEOUT=240
MITCAD_TEST_INPUT=$here/ui-scripts/sketch-scale-view.mitcad-ui \
  ui_run_app sketch-scale-view --ui=qt --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/circles.mitcad"
ui_expect_log 'Scaled the sketch by 1[.]25 to its first dimension' 'the first diameter scaled the sketch up by 1.25'
ui_expect_log 'Scaled the sketch by 0[.]8 to its first dimension' 'the first diameter scaled the sketch down by 0.8'
ui_expect_log 'Sketch view moved with the scaled sketch by 150[.]000 mm' 'the view followed the circle scaled up'
ui_expect_log 'Sketch view moved with the scaled sketch by 144[.]000 mm' 'the view followed the circle scaled down'
ui_expect_log 'TestDriver: done' 'the dimensioned circle kept its place in the window'
ui_expect_no_log 'TestDriver: FAIL|OCCT view event failed' 'all assertions passed'
if [ "$UI_FAILURES" -gt 0 ]; then
  echo "FAIL: sketch view follows the first-dimension scale; evidence in $UI_OUT"
  exit 1
fi
echo "PASS: sketch view follows the first-dimension scale; evidence in $UI_OUT"
