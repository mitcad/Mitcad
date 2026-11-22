#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/commands/SketchCommands.cpp app/MainWindow.cpp
# check-all sources: app/framework/TestDriver.cpp core/sketch core/model-api/src/sketch.rs tools/cli
# check-all sources: tools/ui-scripts/sketch-extend-to-meet.mitcad-ui tools/ui-scripts/sketch-extend-to-meet.json
# Extend to Meet (mitcad#2178) in the Qt frontend on macOS: the shared
# steps tools/ui-scripts/sketch-extend-to-meet.mitcad-ui on the design
# mitcad-cli makes (tools/ui-scripts/sketch-extend-to-meet.json): two lines
# previewed and brought to one corner, then a line and an arc with two
# results, Tab stepping the preview to the second and the click making it.
# As tools/ui-sketch-extend-to-meet-test.sh (Linux) and
# tools/ui-windows-sketch-extend-to-meet-test.ps1 (Windows).
# Usage: bash tools/ui-macos-sketch-extend-to-meet-test.sh <app> <mitcad-cli> [out directory]
set -u
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=tools/ui-macos-lib.sh
. "$here/ui-macos-lib.sh"
if [ $# -lt 2 ] || [ $# -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [out directory]" >&2
  exit 2
fi
cli=$2
ui_init "$1" "${3:-ui-macos-sketch-extend-to-meet}"
export MITCAD_TEST_WORK=$UI_OUT/files
export MITCAD_TEST_STEP_MS=100
mkdir -p "$MITCAD_TEST_WORK"
"$cli" run "$here/ui-scripts/sketch-extend-to-meet.json" --save "$MITCAD_TEST_WORK/meet.mitcad" \
  > "$UI_OUT/setup.log" 2>&1 || { cat "$UI_OUT/setup.log" >&2; exit 1; }
sed "s/^ENTER_SKETCH$/dblclick-logged '^Timeline Sketch1 at '/" \
  "$here/ui-scripts/sketch-extend-to-meet.mitcad-ui" > "$MITCAD_TEST_WORK/workflow.mitcad-ui"
UI_TIMEOUT=240
MITCAD_TEST_INPUT=$MITCAD_TEST_WORK/workflow.mitcad-ui \
  ui_run_app sketch-extend-to-meet --ui=qt --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/meet.mitcad"
ui_expect_log 'TestDriver: done' 'two lines and a line and an arc extended to meet, Tab stepping to the second result'
ui_expect_no_log 'TestDriver: FAIL|OCCT view event failed' 'all assertions passed'
if [ "$UI_FAILURES" -gt 0 ]; then
  echo "FAIL: sketch Extend to Meet; evidence in $UI_OUT"
  exit 1
fi
echo "PASS: sketch Extend to Meet; evidence in $UI_OUT"
