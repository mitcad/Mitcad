#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/PointCloudDeviation.cpp app/commands/PointCloudDeviation.hpp app/commands/InspectCommands.cpp
# check-all sources: app/commands/PrintAnalysis.cpp app/MainWindowPrintAnalysis.cpp app/MainWindowPointClouds.cpp
# check-all sources: app/view/PointClouds.cpp app/view/PointCloudView.cpp app/view/PrintOverlay.cpp app/browser
# check-all sources: core/model/src/document/point_clouds.rs core/model/src/point_cloud_files.rs core/model-api/src/point_clouds.rs
# check-all sources: tools/ui-scripts/point-cloud-deviation.mitcad-ui tools/ui-scripts/point-cloud-deviation.json tools/cli
# check-all sources: app/framework/TestDriverOrbitCenter.cpp
# Point clouds and Point Cloud Deviation (mitcad#2240) through the real UI,
# the shared input script tools/ui-scripts/point-cloud-deviation.mitcad-ui
# (also run by tools/ui-windows-point-cloud-deviation-test.ps1 and
# tools/ui-macos-point-cloud-deviation-test.sh) on the block mitcad-cli
# makes of tools/ui-scripts/point-cloud-deviation.json and the scan written
# here beside it (16 points 0.5 mm above the block's top, 16 points 0.3 mm
# above its bottom, inside the material). Insert > Point Cloud reads the
# scan, chosen with Choose File... (mitcad#2474: no message left, OK
# available at once); Inspect > Point Cloud Deviation gives the statistics and colours the
# points, a point's tooltip its distance; a wider tolerance takes in the
# inner points; the top face only, then 11 of the 32 points measured, the
# others dimmed (mitcad#2265); OK keeps it as Deviation1, saved (checked in
# the file here). The view's picture stays in
# the work folder (build/ui-point-cloud-deviation, overwritten by the next
# run; it is copied to the path given).
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-point-cloud-deviation-test.sh [screenshot.png]
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
shot=${1:-}
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
work=$root/build/ui-point-cloud-deviation
mkdir -p "$work"
trap 'ui_cleanup' EXIT
file=$work/deviation-part.mitcad
"$cli" run "$here/ui-scripts/point-cloud-deviation.json" --save "$file" > "$work/cli.log" 2>&1 ||
  { cat "$work/cli.log"; echo "FAIL: mitcad-cli: the block"; exit 1; }
# The scan: on a 4 x 4 grid, 0.5 mm above the top face and 0.3 mm above the
# bottom face.
: > "$work/scan.xyz"
for x in 4 8 12 16; do
  for y in 4 8 12 16; do
    printf '%s %s 10.5\n%s %s 0.3\n' "$x" "$y" "$x" "$y" >> "$work/scan.xyz"
  done
done
export MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/point-cloud-deviation.mitcad-ui
export MITCAD_TEST_WORK=$work
ui_start_display
ui_start_app --ui=qt --no-mcp --no-native-dialogs --open "$file"
for _ in $(seq 1 1500); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail "point cloud deviation driver timed out"; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
UI_RUNNER=""
ui_expect_log 'Panel Point Cloud message: error gone: Choose a point cloud file.' 'the file chosen in the dialog clears the message'
ui_expect_log 'Point clouds drawn: PointCloud1 32 points' 'the scan inserted and drawn'
ui_expect_log '32 of 32 points, signed, mean 0.100 mm, mean absolute 0.400 mm' 'the statistics'
ui_expect_log 'deviation 32 point(s)' 'the colour map drawn'
ui_expect_log 'Deviation hover: Distance 9.700 mm (inside)' 'the tooltip at a point inside'
ui_expect_log 'Point clouds drawn: PointCloud1 21 points not measured' 'the points not measured drawn dimmed'
ui_expect_log 'Kept Point Cloud Deviation as Deviation1' 'kept in the document'
ui_expect_log 'TestDriver: done' 'Point Cloud Deviation through the UI'
grep -q '"point_clouds"' "$file" || ui_fail "no point cloud in the saved file"
grep -q '"type": "point_cloud_deviation"' "$file" || ui_fail "no deviation in the saved file"
grep -q '"faces"' "$file" || ui_fail "the deviation's face is not in the saved file"
echo "ok   the cloud and the analysis with its face are in the saved file"
[ -s "$work/point-cloud-deviation.png" ] || ui_fail "no picture point-cloud-deviation.png"
echo "ok   picture of the coloured points: $work/point-cloud-deviation.png"
[ -n "$shot" ] && cp "$work/point-cloud-deviation.png" "$shot"
ui_finish 'UI point cloud deviation test'
