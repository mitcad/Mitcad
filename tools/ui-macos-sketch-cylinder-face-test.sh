#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/MainWindow.cpp app/MainWindowSketch3d.cpp app/MainWindowVisibility.cpp
# check-all sources: app/framework/ModelShapes.cpp app/framework/TestDriver.cpp tools/cli
# check-all sources: tools/ui-scripts/sketch-cylinder-face.mitcad-ui tools/ui-scripts/sketch-cylinder-face.json
# A sketch on a cylinder's side (mitcad#1768) on macOS, through the test
# driver (tools/ui-scripts/sketch-cylinder-face.mitcad-ui, the same script
# as the Linux and Windows tests') on the design mitcad-cli makes
# (tools/ui-scripts/sketch-cylinder-face.json, a cylinder of radius 20):
# Create Sketch picks the cylinder's side and starts the sketch in the
# face's developed coordinates; a rectangle typed 30 by 20 from its origin
# is 30 mm around the face and 20 mm along it, its profile is drawn laid
# onto the face, and Finish Sketch ends the sketch.
# Usage: tools/ui-macos-sketch-cylinder-face-test.sh <app> <mitcad-cli> [output directory]
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [output directory]" >&2
  exit 2
fi
ui_init "$1" "${3:-ui-macos-sketch-cylinder-face}"
UI_TIMEOUT=240
cli=$2
files=$UI_OUT/files
rm -rf "$files"
mkdir -p "$files"
"$cli" run "$here/ui-scripts/sketch-cylinder-face.json" --save "$files/cylinder.mitcad" > "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}
MITCAD_TEST_WORK=$files MITCAD_TEST_STEP_MS=100 MITCAD_TEST_INPUT=$here/ui-scripts/sketch-cylinder-face.mitcad-ui \
  ui_run_app sketch-cylinder-face --ui=qt --no-recovery --no-mcp --open "$files/cylinder.mitcad"
ui_expect_log 'Sketch Sketch2 lies on its face in its developed coordinates [(]cylinder, radius 20[.]000 mm[)]' \
  "the cylinder's side picked in Create Sketch takes the sketch"
ui_expect_log 'Added rectangle 30 x 20 mm at ' 'the rectangle typed 30 by 20 on the face'
ui_expect_log 'laid onto its face' "the rectangle's profile drawn on the face"
ui_expect_log '^TestDriver: done' 'a sketch on a cylinder: picked, drawn by typed sizes, measured on the face, finished'
ui_expect_no_log 'TestDriver: FAIL' 'no failed driver assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: sketch on a cylindrical face'
