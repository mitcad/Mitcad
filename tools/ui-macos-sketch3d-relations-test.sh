#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch/Sketch3dPalette.cpp app/sketch/Sketch3dPalette.hpp app/sketch/Sketch3dModel.cpp
# check-all sources: app/sketch/Sketch3dEdit.cpp app/sketch/Sketch3dController.cpp app/sketch/Sketch3dEditing.cpp
# check-all sources: app/MainWindowSketch3d.cpp app/MainWindow.cpp app/framework/TestDriver.cpp tools/cli
# check-all sources: tools/ui-scripts/sketch3d-relations.mitcad-ui tools/ui-scripts/sketch3d-relations.json
# 3D relations applied from the 3D Sketch Palette (mitcad#1661) on macOS,
# through the test driver (tools/ui-scripts/sketch3d-relations.mitcad-ui,
# the same script as the Linux and Windows tests') on the design mitcad-cli
# makes (tools/ui-scripts/sketch3d-relations.json): Parallel and
# Perpendicular of lines in space, Angle of two lines (a driving angle
# listed in degrees), Symmetric of two points about the sketch's YZ plane
# picked in the palette; each one edit, listed in words; Remove on a row
# while a value field has the keyboard removes the row (mitcad#1836).
# Usage: tools/ui-macos-sketch3d-relations-test.sh <app> <mitcad-cli> [output directory]
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [output directory]" >&2
  exit 2
fi
ui_init "$1" "${3:-ui-macos-sketch3d-relations}"
UI_TIMEOUT=300
mkdir -p "$UI_OUT/files"
"$2" run "$here/ui-scripts/sketch3d-relations.json" --save "$UI_OUT/files/sketch3d-relations.mitcad" \
  > "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}
MITCAD_TEST_WORK=$UI_OUT/files MITCAD_TEST_STEP_MS=100 MITCAD_TEST_INPUT=$here/ui-scripts/sketch3d-relations.mitcad-ui \
  ui_run_app sketch3d-relations --ui=qt --no-recovery --no-mcp --open "$UI_OUT/files/sketch3d-relations.mitcad"
ui_expect_log '3D sketch edit parallel: done' 'Parallel of two lines in space was applied'
ui_expect_log '3D sketch edit perpendicular: done' 'Perpendicular of two lines was applied'
ui_expect_log '3D sketch edit angle_relation: done' 'the two lines were held at their angle'
ui_expect_log '[|] shown: Angle 1 on Line 2, Line 4: ' 'the angle is listed in words, in degrees'
ui_expect_log '3D sketch edit symmetric about [{]"frame":"x"[}]: done' 'Symmetric of two points about the YZ plane'
ui_expect_log '[|] shown: Symmetric 1 on Point 9, Point 10' 'the symmetry is listed in words'
ui_expect_log ': Esc took the typed value back' 'a row was removed while a value field kept the keyboard and the value typed'
ui_expect_log 'TestDriver: done' 'the relations were applied from the palette'
ui_expect_no_log '^TestDriver: FAIL' 'no failed driver assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
