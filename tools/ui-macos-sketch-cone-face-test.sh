#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/MainWindow.cpp app/MainWindowSketch3d.cpp app/MainWindowVisibility.cpp
# check-all sources: app/framework/ModelShapes.cpp app/framework/TestDriver.cpp app/OcctViewer.cpp tools/cli
# check-all sources: tools/ui-scripts/sketch-cone-face.mitcad-ui tools/ui-scripts/sketch-cone-face.json
# A sketch on a cone's side (mitcad#1853) on macOS, through the test driver
# (tools/ui-scripts/sketch-cone-face.mitcad-ui, the same script as the
# Linux and Windows tests') on the design mitcad-cli makes
# (tools/ui-scripts/sketch-cone-face.json, a cone of base radius 20 and
# height 30): Create Sketch picks the cone's side and starts the sketch in
# the face's development, the layout grid not drawn on it; a rectangle
# typed 20 by 12 from its origin keeps its lengths on the face, its profile
# is drawn laid onto the face and its two dimensions' lines are drawn on
# the face; Finish Sketch ends the sketch and the grid is drawn again. The
# picture sketch-cone-face.png stays in <output>/files.
# Usage: tools/ui-macos-sketch-cone-face-test.sh <app> <mitcad-cli> [output directory]
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [output directory]" >&2
  exit 2
fi
ui_init "$1" "${3:-ui-macos-sketch-cone-face}"
UI_TIMEOUT=240
cli=$2
files=$UI_OUT/files
rm -rf "$files"
mkdir -p "$files"
"$cli" run "$here/ui-scripts/sketch-cone-face.json" --save "$files/cone.mitcad" > "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}
MITCAD_TEST_WORK=$files MITCAD_TEST_STEP_MS=100 MITCAD_TEST_INPUT=$here/ui-scripts/sketch-cone-face.mitcad-ui \
  ui_run_app sketch-cone-face --ui=qt --no-recovery --no-mcp --open "$files/cone.mitcad"
ui_expect_log 'Sketch Sketch2 lies on its face in its developed coordinates [(]cone, radius 20[.]000 mm, half angle 33[.]690 deg[)]' \
  "the cone's side picked in Create Sketch takes the sketch"
ui_expect_log 'Layout grid not drawn: the sketch lies on a curved face' 'no layout grid on the curved face'
ui_expect_log 'Added rectangle 20 x 12 mm at ' 'the rectangle typed 20 by 12 on the face'
ui_expect_log 'laid onto its face' "the rectangle's profile drawn on the face"
ui_expect_log '^Sketch dimensions on its face: 2 drawn, .*, 0 straight, ' "the dimensions' lines drawn on the face"
ui_expect_log 'Layout grid drawn on its plane again' 'the grid drawn again after the sketch'
ui_expect_log '^TestDriver: done' 'a sketch on a cone: picked, drawn by typed sizes, measured on the face, finished'
ui_expect_no_log 'TestDriver: FAIL' 'no failed driver assertion'
if [ -s "$files/sketch-cone-face.png" ]; then
  ui_pass 'the picture of the sketch on the cone was saved'
else
  ui_fail 'the picture of the sketch on the cone was not saved'
fi
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: sketch on a conical face'
