#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/commands/SketchCommands.cpp app/framework/CommandSession.cpp
# check-all sources: app/framework/Command.hpp app/framework/CommandPanel.cpp app/MainWindowSelection.cpp
# check-all sources: app/framework/TestDriver.cpp core/sketch core/model-api/src/sketch.rs tools/cli
# check-all sources: tools/ui-scripts/sketch-pattern-honeycomb.mitcad-ui tools/ui-scripts/sketch-pattern-honeycomb.json
# The sketch Rectangular Pattern's Direction 2 (mitcad#1969, GitHub
# mitcad/Mitcad#406) in the Qt frontend on macOS: the shared steps
# tools/ui-scripts/sketch-pattern-honeycomb.mitcad-ui on the design
# mitcad-cli makes (tools/ui-scripts/sketch-pattern-honeycomb.json): a line
# at 60 degrees picked as Direction 2 runs the second row of copies along
# it; Edit Pattern keeps it, and At Right Angles turns the row back. As
# tools/ui-sketch-pattern-honeycomb-test.sh (Linux) and
# tools/ui-windows-sketch-pattern-honeycomb-test.ps1 (Windows).
# Usage: bash tools/ui-macos-sketch-pattern-honeycomb-test.sh <app> <mitcad-cli> [out directory]
set -u
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=tools/ui-macos-lib.sh
. "$here/ui-macos-lib.sh"
if [ $# -lt 2 ] || [ $# -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [out directory]" >&2
  exit 2
fi
cli=$2
ui_init "$1" "${3:-ui-macos-sketch-pattern-honeycomb}"
export MITCAD_TEST_WORK=$UI_OUT/files
export MITCAD_TEST_STEP_MS=100
mkdir -p "$MITCAD_TEST_WORK"
"$cli" run "$here/ui-scripts/sketch-pattern-honeycomb.json" --save "$MITCAD_TEST_WORK/pattern.mitcad" \
  > "$UI_OUT/setup.log" 2>&1 || { cat "$UI_OUT/setup.log" >&2; exit 1; }
sed "s/^ENTER_SKETCH$/dblclick-logged '^Timeline Sketch1 at '/" \
  "$here/ui-scripts/sketch-pattern-honeycomb.mitcad-ui" > "$MITCAD_TEST_WORK/workflow.mitcad-ui"
UI_TIMEOUT=240
MITCAD_TEST_INPUT=$MITCAD_TEST_WORK/workflow.mitcad-ui \
  ui_run_app sketch-pattern-honeycomb --ui=qt --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/pattern.mitcad"
ui_expect_log 'TestDriver: done' 'the second row along the picked 60 degree line, kept by Edit Pattern'
ui_expect_no_log 'TestDriver: FAIL|OCCT view event failed' 'all assertions passed'
if [ "$UI_FAILURES" -gt 0 ]; then
  echo "FAIL: sketch Rectangular Pattern's Direction 2 at 60 degrees; evidence in $UI_OUT"
  exit 1
fi
echo "PASS: sketch Rectangular Pattern's Direction 2 at 60 degrees; evidence in $UI_OUT"
