#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/SketchCommands.cpp app/framework/CommandSession.cpp
# check-all sources: app/sketch/SketchController.cpp core/sketch core/solver core/model-api/src/sketch.rs tools/cli
# check-all sources: tools/ui-scripts/sketch-scale.mitcad-ui tools/ui-scripts/sketch-scale.json
# Sketch Scale (mitcad#2109) in the Qt frontend on macOS: the shared steps
# tools/ui-scripts/sketch-scale.mitcad-ui on the design mitcad-cli makes
# (tools/ui-scripts/sketch-scale.json): a circle and a line scaled by 1.5
# about the center of the circle with their dimensions, undone, then
# scaled copies with Keep Values and Create Copy. As
# tools/ui-sketch-scale-test.sh (Linux) and
# tools/ui-windows-sketch-scale-test.ps1 (Windows).
# Usage: bash tools/ui-macos-sketch-scale-test.sh <app> <mitcad-cli> [out directory]
set -u
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=tools/ui-macos-lib.sh
. "$here/ui-macos-lib.sh"
if [ $# -lt 2 ] || [ $# -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [out directory]" >&2
  exit 2
fi
cli=$2
ui_init "$1" "${3:-ui-macos-sketch-scale}"
export MITCAD_TEST_WORK=$UI_OUT/files
export MITCAD_TEST_STEP_MS=100
mkdir -p "$MITCAD_TEST_WORK"
"$cli" run "$here/ui-scripts/sketch-scale.json" --save "$MITCAD_TEST_WORK/scale.mitcad" \
  > "$UI_OUT/setup.log" 2>&1 || { cat "$UI_OUT/setup.log" >&2; exit 1; }
UI_TIMEOUT=240
MITCAD_TEST_INPUT=$here/ui-scripts/sketch-scale.mitcad-ui \
  ui_run_app sketch-scale --ui=qt --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/scale.mitcad"
ui_expect_log 'Preview Sketch Scale sizes: c3 radius 7[.]500' 'the circle scaled by 1.5'
ui_expect_log 'Added Sketch Scale' 'OK kept Sketch Scale'
ui_expect_log 'TestDriver: done' 'Sketch Scale scaled, undid, and scaled copies with the values kept'
ui_expect_no_log 'TestDriver: FAIL|OCCT view event failed' 'all assertions passed'
if [ "$UI_FAILURES" -gt 0 ]; then
  echo "FAIL: Sketch Scale; evidence in $UI_OUT"
  exit 1
fi
echo "PASS: Sketch Scale; evidence in $UI_OUT"
