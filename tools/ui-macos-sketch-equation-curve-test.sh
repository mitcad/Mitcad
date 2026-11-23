#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/commands/SketchCommands.cpp app/MainWindow.cpp app/framework/CommandPanel.cpp
# check-all sources: app/framework/FormulaEditor.cpp app/framework/TestDriver.cpp core/sketch core/model-api/src/sketch.rs tools/cli
# check-all sources: tools/ui-scripts/sketch-equation-curve.mitcad-ui tools/ui-scripts/sketch-equation-curve.json
# Equation Curve (mitcad#2237) in the Qt frontend on macOS: the shared steps
# tools/ui-scripts/sketch-equation-curve.mitcad-ui on the design mitcad-cli
# makes (tools/ui-scripts/sketch-equation-curve.json): an involute of a
# parameter made in the panel, its closed profile extruded, the parameter
# changed and the curve, the profile and the body following, then its
# formula edited. As tools/ui-sketch-equation-curve-test.sh (Linux) and
# tools/ui-windows-sketch-equation-curve-test.ps1 (Windows).
# Usage: bash tools/ui-macos-sketch-equation-curve-test.sh <app> <mitcad-cli> [out directory]
set -u
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=tools/ui-macos-lib.sh
. "$here/ui-macos-lib.sh"
if [ $# -lt 2 ] || [ $# -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [out directory]" >&2
  exit 2
fi
cli=$2
ui_init "$1" "${3:-ui-macos-sketch-equation-curve}"
export MITCAD_TEST_WORK=$UI_OUT/files
export MITCAD_TEST_STEP_MS=100
mkdir -p "$MITCAD_TEST_WORK"
"$cli" run "$here/ui-scripts/sketch-equation-curve.json" --save "$MITCAD_TEST_WORK/equation-curve.mitcad" \
  > "$UI_OUT/setup.log" 2>&1 || { cat "$UI_OUT/setup.log" >&2; exit 1; }
sed "s/^ENTER_SKETCH$/dblclick-logged '^Timeline Sketch1 at '/" \
  "$here/ui-scripts/sketch-equation-curve.mitcad-ui" > "$MITCAD_TEST_WORK/workflow.mitcad-ui"
UI_TIMEOUT=300
MITCAD_TEST_INPUT=$MITCAD_TEST_WORK/workflow.mitcad-ui \
  ui_run_app sketch-equation-curve --ui=qt --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/equation-curve.mitcad"
ui_expect_log 'TestDriver: done' 'an involute of a parameter made, extruded, followed and edited'
ui_expect_no_log 'TestDriver: FAIL|OCCT view event failed' 'all assertions passed'
if [ "$UI_FAILURES" -gt 0 ]; then
  echo "FAIL: sketch Equation Curve involute follows its parameter; evidence in $UI_OUT"
  exit 1
fi
echo "PASS: sketch Equation Curve involute follows its parameter; evidence in $UI_OUT"
