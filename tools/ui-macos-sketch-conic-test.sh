#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/commands/SketchCommands.cpp app/MainWindow.cpp
# check-all sources: app/framework/TestDriver.cpp core/sketch core/model-api/src/sketch.rs tools/cli
# check-all sources: tools/ui-scripts/sketch-conic.mitcad-ui tools/ui-scripts/sketch-conic.json
# Conic Curve and Edit Rho (mitcad#2236) in the Qt frontend on macOS: the
# shared steps tools/ui-scripts/sketch-conic.mitcad-ui on the design
# mitcad-cli makes (tools/ui-scripts/sketch-conic.json): a conic drawn by
# three clicks with Rho 0.7 typed, then its Rho changed to 0.3 from its
# right-click menu. As tools/ui-sketch-conic-test.sh (Linux) and
# tools/ui-windows-sketch-conic-test.ps1 (Windows).
# Usage: bash tools/ui-macos-sketch-conic-test.sh <app> <mitcad-cli> [out directory]
set -u
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=tools/ui-macos-lib.sh
. "$here/ui-macos-lib.sh"
if [ $# -lt 2 ] || [ $# -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [out directory]" >&2
  exit 2
fi
cli=$2
ui_init "$1" "${3:-ui-macos-sketch-conic}"
export MITCAD_TEST_WORK=$UI_OUT/files
export MITCAD_TEST_STEP_MS=100
mkdir -p "$MITCAD_TEST_WORK"
"$cli" run "$here/ui-scripts/sketch-conic.json" --save "$MITCAD_TEST_WORK/conic.mitcad" \
  > "$UI_OUT/setup.log" 2>&1 || { cat "$UI_OUT/setup.log" >&2; exit 1; }
sed "s/^ENTER_SKETCH$/dblclick-logged '^Timeline Sketch1 at '/" \
  "$here/ui-scripts/sketch-conic.mitcad-ui" > "$MITCAD_TEST_WORK/workflow.mitcad-ui"
UI_TIMEOUT=240
MITCAD_TEST_INPUT=$MITCAD_TEST_WORK/workflow.mitcad-ui \
  ui_run_app sketch-conic --ui=qt --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/conic.mitcad"
ui_expect_log 'TestDriver: done' 'a conic drawn with Rho 0.7 and its Rho changed to 0.3 from its menu'
ui_expect_no_log 'TestDriver: FAIL|OCCT view event failed' 'all assertions passed'
if [ "$UI_FAILURES" -gt 0 ]; then
  echo "FAIL: sketch Conic Curve; evidence in $UI_OUT"
  exit 1
fi
echo "PASS: sketch Conic Curve; evidence in $UI_OUT"
