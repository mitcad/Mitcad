#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch/Sketch3dPalette.cpp app/sketch/Sketch3dPalette.hpp app/sketch/Sketch3dModel.cpp
# check-all sources: app/sketch/Sketch3dModel.hpp app/sketch/Sketch3dEdit.cpp app/sketch/Sketch3dEdit.hpp
# check-all sources: app/sketch/Sketch3dController.cpp app/sketch/Sketch3dController.hpp app/sketch/Sketch3dEditing.cpp
# check-all sources: app/MainWindowSketch3d.cpp app/MainWindow.cpp app/framework/TestDriver.cpp
# check-all sources: tools/ui-scripts/sketch3d-relations.mitcad-ui tools/ui-scripts/sketch3d-relations.json
# check-all sources: tools/cli core/sketch core/solver core/model core/model-api
# 3D relations applied from the 3D Sketch Palette (mitcad#1661) through the
# real UI, the shared input script tools/ui-scripts/sketch3d-relations.mitcad-ui
# (also run by tools/ui-windows-sketch3d-relations-test.ps1) on a design
# that mitcad-cli makes (tools/ui-scripts/sketch3d-relations.json): Parallel
# and Perpendicular of lines in space, Angle of two lines (a driving angle
# listed in degrees), Symmetric of two points about the sketch's YZ plane
# picked in the palette; each one edit, listed in words with no id,
# topological name or coordinate; the sketch solves.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sketch3d-relations-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-sketch3d-relations.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/sketch3d-relations.mitcad-ui
"$cli" run "$here/ui-scripts/sketch3d-relations.json" --save "$work/sketch3d-relations.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/sketch3d-relations.mitcad"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail '3D sketch relations driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log '3D sketch edit parallel: done' 'Parallel of two lines in space was applied'
ui_expect_log '3D sketch edit perpendicular: done' 'Perpendicular of two lines was applied'
ui_expect_log '3D sketch edit angle_relation: done' 'the two lines were held at their angle'
ui_expect_log '| shown: Angle 1 on Line 2, Line 4: ' 'the angle is listed in words, in degrees'
ui_expect_log '3D sketch edit symmetric about {"frame":"x"}: done' 'Symmetric of two points about the YZ plane'
ui_expect_log '| shown: Symmetric 1 on Point 9, Point 10' 'the symmetry is listed in words'
ui_expect_log 'TestDriver: done' 'the relations were applied from the palette'
# What a person reads has no id, topological name or coordinate (the
# agents' part before "| shown:" keeps them).
if grep -E '[|] shown: .*([pck][0-9]+([^0-9]|$)|[EV][{]|F[0-9]+[.:]|[0-9][.][0-9]+, -?[0-9])' "$UI_LOG"; then
  ui_fail 'a shown text has an id, a topological name or a coordinate'
fi
echo "ok   no shown text has an id, a topological name or a coordinate"
ui_finish 'UI 3D sketch relations'
