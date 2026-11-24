#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/files app/framework/TestDriver.cpp tools/cli
# check-all sources: core/model/src/exchange core/model/src/exchange.rs core/model-api/src/exchange.rs
# check-all sources: tools/ui-scripts/urdf.mitcad-ui tools/ui-scripts/urdf.json
# URDF export for ROS 2 (mitcad#2100) through the real UI, the shared input
# script tools/ui-scripts/urdf.mitcad-ui (also run by
# tools/ui-windows-urdf-test.ps1 and tools/ui-macos-urdf-test.sh), on a
# mechanism of three components (tools/ui-scripts/urdf.json): File > Export
# to robot.urdf writes the package folder robot/ beside it: urdf/robot.urdf
# (a robot of four links and three joints), a binary STL per body in
# meshes/, package.xml and CMakeLists.txt.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-urdf-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-urdf.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100

"$cli" run "$here/ui-scripts/urdf.json" --save "$work/robot.mitcad" > "$work/cli.log" 2>&1 ||
  { cat "$work/cli.log"; ui_fail "mitcad-cli: the design"; exit 1; }
ui_start_display

MITCAD_TEST_INPUT=$here/ui-scripts/urdf.mitcad-ui \
  ui_start_app --ui=qt --no-mcp --no-native-dialogs --open "$work/robot.mitcad"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail "export: the driver timed out"; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'TestDriver: done' 'File > Export writes the URDF package folder'
urdf=$work/robot/urdf/robot.urdf
[ "$(grep -c '<link name=' "$urdf")" = 4 ] || ui_fail "robot.urdf has not 4 links"
[ "$(grep -c '<joint name=' "$urdf")" = 3 ] || ui_fail "robot.urdf has not 3 joints"
[ "$(find "$work/robot/meshes" -name '*.stl' | wc -l)" = 3 ] || ui_fail "the package has not 3 meshes"
grep -q '<name>robot</name>' "$work/robot/package.xml" || ui_fail "package.xml does not name the package robot"
echo "ok   robot/ holds robot.urdf (4 links, 3 joints), 3 meshes, package.xml and CMakeLists.txt"
ui_finish 'UI URDF export'
