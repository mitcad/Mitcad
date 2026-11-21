#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch/EditTools.cpp app/sketch/SketchController.cpp app/MainWindow.cpp
# check-all sources: core/sketch core/solver tools/cli core/model-api/src/sketch.rs
# check-all sources: tools/ui-scripts/sketch-tangent-dimension.mitcad-ui tools/ui-scripts/sketch-tangent-dimension.json
# Sketch Dimension's right-click menu (mitcad#1540) and Make Driven / Make
# Driving on an existing dimension (mitcad#2055) in the Qt frontend on
# macOS, as tools/ui-sketch-tangent-dimension-test.sh on Linux: the shared
# steps tools/ui-scripts/sketch-tangent-dimension.mitcad-ui on the design
# that mitcad-cli makes (tools/ui-scripts/sketch-tangent-dimension.json).
# Usage: tools/ui-macos-sketch-tangent-dimension-test.sh <app> <mitcad-cli> <output>
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=tools/ui-macos-lib.sh
. "$here/ui-macos-lib.sh"
if [ "$#" != 3 ]; then
  echo 'usage: ui-macos-sketch-tangent-dimension-test.sh <app> <cli> <output>' >&2
  exit 2
fi
ui_init "$1" "$3"
cli=$2
UI_TIMEOUT=180
export MITCAD_TEST_WORK=$UI_OUT/files
rm -rf "$MITCAD_TEST_WORK"
mkdir -p "$MITCAD_TEST_WORK"
design=$MITCAD_TEST_WORK/circles.mitcad
"$cli" run "$here/ui-scripts/sketch-tangent-dimension.json" --save "$design" > "$UI_OUT/setup.log" 2>&1 ||
  { cat "$UI_OUT/setup.log"; echo 'FAIL: mitcad-cli run sketch-tangent-dimension.json' >&2; exit 1; }
MITCAD_TEST_INPUT=$here/ui-scripts/sketch-tangent-dimension.mitcad-ui MITCAD_TEST_STEP_MS=100 \
  ui_run_app sketch-tangent-dimension --ui=qt --no-recovery --no-mcp --no-native-dialogs --open "$design"
ui_expect_log '^Sketch Dimension prompt: .*then right-click for Driven[.]$' 'the Pick prompt names the right-click Driven'
ui_expect_log '^Sketch Dimension prompt: .*Right-click: Driven[.]$' 'the Place prompt names the right-click Driven'
ui_expect_log '^Undo: Make Driven in Sketch1$' 'Make Driven from the context menu, one undo step'
ui_expect_log '^Undo: Make Driving in Sketch1$' 'Make Driving from the context menu, one undo step'
ui_expect_log '^TestDriver: done' 'the right-click Driven and Make Driven / Make Driving'
ui_expect_no_log 'TestDriver: FAIL' 'no failed sketch dimension assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt sketch dimension driven choices'
