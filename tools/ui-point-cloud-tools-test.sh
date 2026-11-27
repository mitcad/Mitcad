#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/PointCloudTools.cpp app/commands/PointCloudTools.hpp app/MainWindowPointClouds.cpp
# check-all sources: app/browser/BrowserController.cpp app/browser/DocumentHost.hpp
# check-all sources: core/model/src/point_cloud_tools core/model/src/document/point_cloud_tools.rs
# check-all sources: core/model-api/src/point_clouds.rs
# check-all sources: tools/ui-scripts/point-cloud-tools.mitcad-ui tools/ui-scripts/point-cloud-tools.json tools/cli
# Point cloud tools (mitcad#2516) through the real UI, the shared input
# script tools/ui-scripts/point-cloud-tools.mitcad-ui (also run by
# tools/ui-windows-point-cloud-tools-test.ps1 and
# tools/ui-macos-point-cloud-tools-test.sh) on the design mitcad-cli makes
# of tools/ui-scripts/point-cloud-tools.json: a block and a scan of its faces
# moved by (0.6, -0.4, 0.3) mm with 3 outliers. Filter Point Cloud takes the
# outliers out into Scan Filtered; Align Point Cloud, from the cloud's row
# in the Browser, states the remaining deviation and moves it onto the
# block; Reconstruct Mesh adds a closed mesh body of it; saved (checked in
# the file here). The view's pictures stay in the work folder
# (build/ui-point-cloud-tools, overwritten by the next run; the aligned one
# is copied to the path given).
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-point-cloud-tools-test.sh [screenshot.png]
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
shot=${1:-}
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
work=$root/build/ui-point-cloud-tools
mkdir -p "$work"
trap 'ui_cleanup' EXIT
file=$work/point-cloud-tools.mitcad
"$cli" run "$here/ui-scripts/point-cloud-tools.json" --save "$file" > "$work/cli.log" 2>&1 ||
  { cat "$work/cli.log"; echo "FAIL: mitcad-cli: the block and its scan"; exit 1; }
export MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/point-cloud-tools.mitcad-ui
export MITCAD_TEST_WORK=$work
ui_start_display
ui_start_app --ui=qt --no-mcp --no-native-dialogs --open "$file"
for _ in $(seq 1 3000); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail "point cloud tools driver timed out"; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
UI_RUNNER=""
ui_expect_log 'Filtered point cloud Scan: outliers statistical' 'the outliers filtered out'
ui_expect_log 'Point clouds drawn: Scan Filtered 110 points' 'the filtered cloud drawn'
ui_expect_log 'Inspect Align Point Cloud: Scan Filtered to Body1: RMS 0.000 mm' 'the remaining deviation stated'
ui_expect_log 'Aligned point cloud Scan Filtered to Body1' 'the cloud aligned'
ui_expect_log 'Reconstructed a mesh of point cloud Scan Filtered' 'the mesh built'
ui_expect_log 'TestDriver: done' 'the point cloud tools through the UI'
grep -q '"Scan Filtered"' "$file" || ui_fail "no filtered cloud in the saved file"
grep -q '"source": "Scan Filtered"' "$file" || ui_fail "no mesh of the cloud in the saved file"
grep -q '"Scan Filtered Mesh"' "$file" || ui_fail "the mesh body's name is not in the saved file"
echo "ok   the filtered cloud and its mesh are in the saved file"
[ -s "$work/point-cloud-tools-aligned.png" ] || ui_fail "no picture point-cloud-tools-aligned.png"
echo "ok   pictures: $work/point-cloud-tools-aligned.png, $work/point-cloud-tools-mesh.png"
[ -n "$shot" ] && cp "$work/point-cloud-tools-aligned.png" "$shot"
ui_finish 'UI point cloud tools test'
