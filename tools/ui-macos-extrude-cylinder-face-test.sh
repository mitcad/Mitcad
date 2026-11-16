#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindow.cpp app/OcctViewer.cpp app/commands/CommandSupport.cpp
# check-all sources: app/commands/ExtrudeCommands.cpp app/sketch/SketchSurface.cpp app/framework/TestDriver.cpp
# check-all sources: core/model/src/features/extrude.rs core/model/src/features/developed.rs tools/cli
# check-all sources: tools/ui-scripts/extrude-cylinder-face.mitcad-ui tools/ui-scripts/extrude-cylinder-face.json
# Extrude of a sketch on a cylinder's side with the automatic operation
# (mitcad#1908, GitHub mitcad/Mitcad#362) on macOS, through the test driver
# (tools/ui-scripts/extrude-cylinder-face.mitcad-ui, the same script as the
# Linux and Windows tests') on the design mitcad-cli makes as reported
# (tools/ui-scripts/extrude-cylinder-face.json: a cylinder of radius 150 in
# a component, a rectangle and a circle sketched on its side): the picked
# profiles are highlighted on the face, the distance arrow starts there,
# the automatic Join builds and OK adds it.
# Usage: tools/ui-macos-extrude-cylinder-face-test.sh <app> <mitcad-cli> [output directory]
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [output directory]" >&2
  exit 2
fi
ui_init "$1" "${3:-ui-macos-extrude-cylinder-face}"
UI_TIMEOUT=300
cli=$2
files=$UI_OUT/files
rm -rf "$files"
mkdir -p "$files"
"$cli" run "$here/ui-scripts/extrude-cylinder-face.json" --save "$files/cylinder.mitcad" > "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}
MITCAD_TEST_WORK=$files MITCAD_TEST_STEP_MS=100 MITCAD_LOG_PICKS=1 \
  MITCAD_TEST_INPUT=$here/ui-scripts/extrude-cylinder-face.mitcad-ui \
  ui_run_app extrude-cylinder-face --ui=qt --no-recovery --no-mcp --open "$files/cylinder.mitcad"
ui_expect_log 'profile F3/r[{]c1[[]c4,c2[]],c2[[]c1,c3[]],c3[[]c2,c4[]],c4[[]c3,c1[]][}] in O1 at 13[01][.][0-9],-66[.][0-9],55[.]0' \
  "the rectangle's highlight on the cylinder's side"
ui_expect_log 'Extrude: automatic Operation = Join' 'the New Body preview turned Join'
ui_expect_log 'Added extrude [(]Join[)]' 'the Join of the profiles on the face added'
ui_expect_log '^TestDriver: done' 'Extrude of a sketch on a cylinder with the automatic Join'
ui_expect_no_log 'TestDriver: FAIL' 'no failed driver assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: extrude of a sketch on a cylindrical face'
