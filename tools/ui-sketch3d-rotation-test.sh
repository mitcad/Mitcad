#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch/Sketch3dTools.cpp app/sketch/Sketch3dDraw.cpp app/sketch/Sketch3dDraw.hpp
# check-all sources: app/sketch/Sketch3dShapes.cpp core/sketch core/solver core/model-api
# check-all sources: app/sketch/Sketch3dSnaps.cpp app/sketch/Sketch3dController.cpp app/sketch/Sketch3dController.hpp
# check-all sources: app/sketch/Sketch3dPalette.cpp app/sketch/Sketch3dPalette.hpp app/MainWindowSketch3d.cpp
# check-all sources: app/framework/TestDriver.cpp tools/ui-scripts/sketch3d-rotation.mitcad-ui
# check-all sources: tools/ui-scripts/sketch3d-rotation.json tools/cli
# The free rotation of a 3D sketch's placement plane (mitcad#1661) through
# the real UI, the shared input script
# tools/ui-scripts/sketch3d-rotation.mitcad-ui (also run by
# tools/ui-windows-sketch3d-rotation-test.ps1) on a design that mitcad-cli
# makes (tools/ui-scripts/sketch3d-rotation.json): a route drawn with the
# rotated plane. Rotate X 90 typed and Axis and Length 30 along the rotated Y put
# the end at (40, 0, 30); the Z handle dragged turns the plane by whole
# degrees and the next end stays in the sketch's XZ plane; a circle
# drawn on the rotated plane carries its frame (mitcad#1661); Reset, Show
# Handles and Show Plane; the sketch's stored frame is the same when it
# is opened again, and the plane is its XY.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sketch3d-rotation-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-sketch3d-rotation.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/sketch3d-rotation.mitcad-ui
"$cli" run "$here/ui-scripts/sketch3d-rotation.json" --save "$work/sketch3d-rotation.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/sketch3d-rotation.mitcad"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail '3D sketch rotation driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log '3D sketch rotation handle X at ' 'the rotation handles were logged where they can be grabbed'
ui_expect_log '3D sketch placement: along the rotated Y' 'the placement followed the rotated plane'
ui_expect_log '"end":[40,0,30]' 'the end along the rotated Y with Axis and Length typed is (40, 0, 30)'
ui_expect_log 'about Z by its handle' 'the Z handle'"'"'s drag rotated the plane'
ui_expect_log '"plane":{"normal":' 'the circle on the rotated plane carried its frame'
ui_expect_log '3D sketch plane reset: on the XY plane' 'Reset gave the sketch'"'"'s XY'
ui_expect_log '3D sketch manipulator: rotation handles hidden' 'Show Handles hid the handles'
ui_expect_log '3D sketch manipulator: plane hidden' 'Show Plane hid the plane'
ui_expect_log 'TestDriver: done' 'a route was drawn with the rotated plane'
ui_finish 'UI 3D sketch plane rotation'
