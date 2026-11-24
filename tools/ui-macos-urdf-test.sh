#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/files app/framework/TestDriver.cpp tools/cli
# check-all sources: core/model/src/exchange core/model/src/exchange.rs core/model-api/src/exchange.rs
# URDF export for ROS 2 (mitcad#2100) on macOS in Qt, as
# tools/ui-urdf-test.sh on Linux: the shared input script
# tools/ui-scripts/urdf.mitcad-ui on a mechanism of three components
# (tools/ui-scripts/urdf.json): File > Export to robot.urdf writes the
# package folder robot/ beside it: urdf/robot.urdf (four links, three
# joints), a binary STL per body in meshes/, package.xml and CMakeLists.txt.
# Usage: tools/ui-macos-urdf-test.sh <app> <mitcad-cli> <output>
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=tools/ui-macos-lib.sh
. "$here/ui-macos-lib.sh"
if [ "$#" != 3 ]; then
  echo 'usage: ui-macos-urdf-test.sh <app> <cli> <output>' >&2
  exit 2
fi
ui_init "$1" "$3"
cli=$2
UI_TIMEOUT=240
export MITCAD_TEST_WORK=$UI_OUT/files
rm -rf "$MITCAD_TEST_WORK"
mkdir -p "$MITCAD_TEST_WORK"
"$cli" run "$here/ui-scripts/urdf.json" --save "$MITCAD_TEST_WORK/robot.mitcad" > "$UI_OUT/setup.log" 2>&1 ||
  { cat "$UI_OUT/setup.log" >&2; echo 'FAIL: mitcad-cli: the design' >&2; exit 1; }

MITCAD_TEST_INPUT=$here/ui-scripts/urdf.mitcad-ui MITCAD_TEST_STEP_MS=100 \
  ui_run_app urdf --ui=qt --no-recovery --no-mcp --no-native-dialogs --open "$MITCAD_TEST_WORK/robot.mitcad"
ui_expect_log '^TestDriver: done' 'File > Export writes the URDF package folder'
ui_expect_no_log 'TestDriver: FAIL' 'no failed URDF export step'
[ "$UI_FAILURES" = 0 ] || exit 1
urdf=$MITCAD_TEST_WORK/robot/urdf/robot.urdf
[ "$(grep -c '<link name=' "$urdf")" = 4 ] || { echo 'FAIL: robot.urdf has not 4 links' >&2; exit 1; }
[ "$(grep -c '<joint name=' "$urdf")" = 3 ] || { echo 'FAIL: robot.urdf has not 3 joints' >&2; exit 1; }
[ "$(find "$MITCAD_TEST_WORK/robot/meshes" -name '*.stl' | wc -l | tr -d ' ')" = 3 ] ||
  { echo 'FAIL: the package has not 3 meshes' >&2; exit 1; }
grep -q '<name>robot</name>' "$MITCAD_TEST_WORK/robot/package.xml" ||
  { echo 'FAIL: package.xml does not name the package robot' >&2; exit 1; }
echo 'ok   robot/ holds robot.urdf (4 links, 3 joints), 3 meshes, package.xml and CMakeLists.txt'
echo 'PASS: Qt URDF export'
