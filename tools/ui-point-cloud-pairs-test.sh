#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/PointCloudTools.cpp app/commands/PointCloudTools.hpp app/MainWindowPointClouds.cpp
# check-all sources: app/browser/BrowserController.cpp app/framework/Command.hpp app/framework/CommandSession.cpp
# check-all sources: app/framework/CommandSessionHover.cpp app/framework/ManipulatorOverlay.cpp
# check-all sources: app/view/PointClouds.cpp app/view/PointClouds.hpp
# check-all sources: core/model/src/point_cloud_tools core/model/src/document/point_cloud_tools.rs
# check-all sources: core/model-api/src/point_clouds.rs
# check-all sources: tools/ui-scripts/point-cloud-pairs.mitcad-ui tools/ui-scripts/point-cloud-pairs.json tools/cli
# Point pairs and the crop box of the point cloud tools (mitcad#2565)
# through the real UI, the shared input script
# tools/ui-scripts/point-cloud-pairs.mitcad-ui (also run by
# tools/ui-windows-point-cloud-pairs-test.ps1 and
# tools/ui-macos-point-cloud-pairs-test.sh) on the design mitcad-cli makes of
# tools/ui-scripts/point-cloud-pairs.json: a block and a scan of its faces
# turned 30 degrees and moved 20 mm. Align Point Cloud by three point pairs
# clicked in the view (one point taken back by Backspace) and the best fit
# moves the scan back onto the block within 0.01 mm RMS; Filter Point Cloud
# with a crop box, its handle dragged and its values typed, keeps the 140
# points inside it. The view's pictures stay in the work folder
# (build/ui-point-cloud-pairs, overwritten by the next run; the aligned one
# is copied to the path given).
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-point-cloud-pairs-test.sh [screenshot.png]
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
shot=${1:-}
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
work=$root/build/ui-point-cloud-pairs
mkdir -p "$work"
trap 'ui_cleanup' EXIT
file=$work/point-cloud-pairs.mitcad
"$cli" run "$here/ui-scripts/point-cloud-pairs.json" --save "$file" > "$work/cli.log" 2>&1 ||
  { cat "$work/cli.log"; echo "FAIL: mitcad-cli: the block and its scan"; exit 1; }
export MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/point-cloud-pairs.mitcad-ui
export MITCAD_TEST_WORK=$work
ui_start_display
ui_start_app --ui=qt --no-mcp --no-native-dialogs --open "$file"
for _ in $(seq 1 3000); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail "point cloud pairs driver timed out"; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
UI_RUNNER=""
ui_expect_log 'Align Point Cloud points: 6' 'three point pairs picked in the view'
ui_expect_log 'Align Point Cloud: Backspace taken' 'a picked point taken back'
ui_expect_log 'Inspect Align Point Cloud: Scan to Body1: RMS 0.00' 'the remaining deviation stated'
ui_expect_log 'Aligned point cloud Scan to Body1: RMS 0.00' 'the cloud aligned within 0.01 mm'
ui_expect_log 'cropped by a box from -1.000,-1.000,-1.000 to 9.000,21.000,11.000, inside kept' 'the box crop'
ui_expect_log 'Point clouds drawn: Scan Filtered 140 points' 'the points inside the box kept'
ui_expect_log 'TestDriver: done' 'point pairs and the crop box through the UI'
[ -s "$work/point-cloud-pairs-aligned.png" ] || ui_fail "no picture point-cloud-pairs-aligned.png"
echo "ok   pictures: $work/point-cloud-pairs-aligned.png, $work/point-cloud-pairs-box.png"
[ -n "$shot" ] && cp "$work/point-cloud-pairs-aligned.png" "$shot"
ui_finish 'UI point cloud pairs test'
