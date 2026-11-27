#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/PointCloudTools.cpp app/commands/MeshShapeCommands.cpp
# check-all sources: app/commands/SketchNormal.cpp app/commands/SketchNormal.hpp app/OcctViewer.cpp
# check-all sources: core/model/src/document/point_cloud_tools.rs core/model/src/features/mesh_shape_tools.rs
# check-all sources: tools/ui-scripts/point-cloud-pairs-placed.mitcad-ui tools/ui-scripts/point-cloud-pairs-placed.json tools/cli
# Point pairs on bodies of a placed component (mitcad#2566) through the
# real UI, the shared input script
# tools/ui-scripts/point-cloud-pairs-placed.mitcad-ui (also run by
# tools/ui-windows-point-cloud-pairs-placed-test.ps1 and
# tools/ui-macos-point-cloud-pairs-placed-test.sh) on the design mitcad-cli
# makes of tools/ui-scripts/point-cloud-pairs-placed.json in the work
# folder: a block and a mesh block in a component whose occurrence is
# turned 90 degrees and moved, and a scan of the block's faces in the root.
# Align Point Cloud by three point pairs clicked on the placed block takes
# them into the design and moves the scan onto the block as the view shows
# it within 0.01 mm RMS; Align Mesh by three pairs keeps them in the
# component's coordinates and moves the mesh onto the block within 0.01 mm
# RMS. The view's pictures stay in the work folder
# (build/ui-point-cloud-pairs-placed, overwritten by the next run; the
# aligned cloud's is copied to the path given).
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-point-cloud-pairs-placed-test.sh [screenshot.png]
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
shot=${1:-}
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
work=$root/build/ui-point-cloud-pairs-placed
mkdir -p "$work"
trap 'ui_cleanup' EXIT
file=$work/point-cloud-pairs-placed.mitcad
(cd "$work" && "$cli" run "$here/ui-scripts/point-cloud-pairs-placed.json" --save "$file") > "$work/cli.log" 2>&1 ||
  { cat "$work/cli.log"; echo "FAIL: mitcad-cli: the placed blocks and the scan"; exit 1; }
export MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/point-cloud-pairs-placed.mitcad-ui
export MITCAD_TEST_WORK=$work
ui_start_display
ui_start_app --ui=qt --no-mcp --no-native-dialogs --open "$file"
for _ in $(seq 1 3000); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail "point cloud pairs placed driver timed out"; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
UI_RUNNER=""
ui_expect_log 'Align Point Cloud target point: F1.b0 at ' 'a target point taken into the design'
ui_expect_log 'Align Point Cloud points: 6' 'three point pairs picked on the placed block'
ui_expect_log 'Aligned point cloud Scan to Body1: RMS 0.00' 'the cloud aligned within 0.01 mm'
ui_expect_log 'Align Mesh points: 6' 'three point pairs picked on the placed mesh and block'
ui_expect_log 'Added mesh align of F4.b0 onto F1.b0, 3 point pairs, best fit' 'the mesh aligned'
ui_expect_log 'TestDriver: done' 'point pairs on a placed component through the UI'
[ -s "$work/point-cloud-pairs-placed-aligned.png" ] || ui_fail "no picture point-cloud-pairs-placed-aligned.png"
echo "ok   pictures: $work/point-cloud-pairs-placed-aligned.png, $work/point-cloud-pairs-placed-mesh.png"
[ -n "$shot" ] && cp "$work/point-cloud-pairs-placed-aligned.png" "$shot"
ui_finish 'UI point cloud pairs placed test'
