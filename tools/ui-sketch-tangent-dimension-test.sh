#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch/EditTools.cpp app/sketch/SketchTool.hpp app/sketch/SketchOverlay.cpp
# check-all sources: app/sketch/SketchController.cpp app/sketch/SketchController.hpp app/MainWindow.cpp
# check-all sources: app/framework/TestDriver.cpp core/sketch core/solver tools/cli
# check-all sources: tools/ui-scripts/sketch-tangent-dimension.mitcad-ui tools/ui-scripts/sketch-tangent-dimension.json
# Sketch Dimension's right-click menu (mitcad#1540, GitHub #203), through
# the real UI on a design that mitcad-cli makes
# (tools/ui-scripts/sketch-tangent-dimension.json: a circle of diameter 20
# about the fixed origin, a free circle of radius 5 about (30, 0)): a
# right-click on the first circle offers Center and Tangent for it and
# Driving and Driven; Tangent picks it by its tangent side, the second
# circle is picked by its centre, and 18 typed adds a
# tangent_point_distance that moves the free centre to 28; then both
# circles by their tangent sides and Driven add a driven
# tangent_circle_distance that measures 13. The steps are
# tools/ui-scripts/sketch-tangent-dimension.mitcad-ui, which
# tools/ui-windows-sketch-tangent-dimension-test.ps1 runs on Windows.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sketch-tangent-dimension-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-sketch-tangent-dimension.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/sketch-tangent-dimension.mitcad-ui
"$cli" run "$here/ui-scripts/sketch-tangent-dimension.json" --save "$work/circles.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/circles.mitcad"
for _ in $(seq 1 600); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'tangent dimension driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Context menu: Center | Tangent | Driving (current) | Driven | Cancel' \
  'the right-click on a circle offers Center, Tangent, Driving and Driven'
ui_expect_log 'Sketch Dimension preview: tangent_point_distance 20.00' 'the preview from the tangent side to the centre'
grep -Eq 'Added dimension k[0-9]+ tangent_point_distance d[0-9]+ = 18 mm$' "$UI_LOG" ||
  { ui_fail 'no tangent_point_distance added with the value 18'; exit 1; }
echo 'ok   a tangent-to-center dimension added with the value 18'
ui_expect_log 'Sketch Dimension preview: tangent_circle_distance (13.00)' 'the driven preview in parentheses'
ui_expect_log 'tangent_circle_distance = 13.00' 'a driven tangent-to-tangent dimension measuring 13'
ui_expect_log 'TestDriver: done' 'tangent and center picks from the right-click menu'
ui_finish 'UI sketch dimension tangent and center picks'
