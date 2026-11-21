#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/commands/SketchCommands.cpp app/MainWindow.cpp
# check-all sources: app/framework/TestDriver.cpp core/sketch core/model-api/src/sketch.rs tools/cli
# check-all sources: tools/ui-scripts/sketch-break.mitcad-ui tools/ui-scripts/sketch-break.json
# Break and Split at Point (mitcad#2108) in the Qt frontend on macOS: the
# shared steps
# tools/ui-scripts/sketch-break.mitcad-ui on the design mitcad-cli
# makes (tools/ui-scripts/sketch-break.json): Split at Point splits an arc and
# keeps its radius dimension, Break splits a line crossed by a circle into
# three collinear pieces, and the pieces turn together when the line's free
# end is dragged. As
# tools/ui-sketch-break-test.sh (Linux) and
# tools/ui-windows-sketch-break-test.ps1 (Windows).
# Usage: bash tools/ui-macos-sketch-break-test.sh <app> <mitcad-cli> [out directory]
set -u
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=tools/ui-macos-lib.sh
. "$here/ui-macos-lib.sh"
if [ $# -lt 2 ] || [ $# -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [out directory]" >&2
  exit 2
fi
cli=$2
ui_init "$1" "${3:-ui-macos-sketch-break}"
export MITCAD_TEST_WORK=$UI_OUT/files
export MITCAD_TEST_STEP_MS=100
mkdir -p "$MITCAD_TEST_WORK"
"$cli" run "$here/ui-scripts/sketch-break.json" --save "$MITCAD_TEST_WORK/break.mitcad" \
  > "$UI_OUT/setup.log" 2>&1 || { cat "$UI_OUT/setup.log" >&2; exit 1; }
sed "s/^ENTER_SKETCH$/dblclick-logged '^Timeline Sketch1 at '/" \
  "$here/ui-scripts/sketch-break.mitcad-ui" > "$MITCAD_TEST_WORK/workflow.mitcad-ui"
UI_TIMEOUT=240
MITCAD_TEST_INPUT=$MITCAD_TEST_WORK/workflow.mitcad-ui \
  ui_run_app sketch-break --ui=qt --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/break.mitcad"
ui_expect_log 'TestDriver: done' 'an arc split at a point, a line broken at its crossings, the pieces held together'
ui_expect_no_log 'TestDriver: FAIL|OCCT view event failed' 'all assertions passed'
if [ "$UI_FAILURES" -gt 0 ]; then
  echo "FAIL: sketch Break and Split at Point; evidence in $UI_OUT"
  exit 1
fi
echo "PASS: sketch Break and Split at Point; evidence in $UI_OUT"
