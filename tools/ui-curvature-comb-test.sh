#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/CurvatureComb.cpp app/commands/CurvatureComb.hpp
# check-all sources: app/commands/CurvatureCombCommand.cpp app/commands/CurvatureCombCommand.hpp
# check-all sources: app/sketch/SketchController.cpp app/sketch/SketchOverlay.cpp app/sketch/SketchPalette.cpp
# check-all sources: core/model/src/curvature_comb.rs core/model-api/src/curvature_comb.rs
# check-all sources: tools/ui-scripts/curvature-comb.mitcad-ui tools/ui-scripts/curvature-comb.json tools/cli
# Curvature Comb (mitcad#2520) through the real UI, the shared input script
# tools/ui-scripts/curvature-comb.mitcad-ui (also run by
# tools/ui-windows-curvature-comb-test.ps1 and
# tools/ui-macos-curvature-comb-test.sh) on the design mitcad-cli makes of
# tools/ui-scripts/curvature-comb.json. Inspect > Curvature Comb combs a
# cylinder's top edge (no inflection, radius 10 mm); in Sketch2 the
# palette's Curvature comb shows an S-shaped spline's comb with its one
# inflection at (55, 0), read again while its end point is dragged. The
# view's pictures stay in the work folder (build/ui-curvature-comb,
# overwritten by the next run; a picture is copied to the path given).
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-curvature-comb-test.sh [screenshot.png]
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
shot=${1:-}
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
work=$root/build/ui-curvature-comb
mkdir -p "$work"
trap 'ui_cleanup' EXIT
file=$work/curvature-comb.mitcad
"$cli" run "$here/ui-scripts/curvature-comb.json" --save "$file" > "$work/cli.log" 2>&1 ||
  { cat "$work/cli.log"; echo "FAIL: mitcad-cli: the design"; exit 1; }
export MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/curvature-comb.mitcad-ui
export MITCAD_TEST_WORK=$work
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$file"
for _ in $(seq 1 1500); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail "curvature comb driver timed out"; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
UI_RUNNER=""
ui_expect_log 'Inspect Curvature Comb: 1 curve(s), 24 teeth each, no inflection, minimum radius 10 mm' 'an edge combed'
ui_expect_log 'Sketch curvature comb: c' 'a sketch curve combed in sketch mode'
ui_expect_log 'Sketch curvature comb (dragging): c' 'the comb read again while dragged'
ui_expect_log 'TestDriver: done' 'Curvature Comb through the UI'
for picture in curvature-comb-edge.png curvature-comb-sketch.png curvature-comb-dragged.png; do
  [ -s "$work/$picture" ] || ui_fail "no picture $picture"
done
echo "ok   pictures of the combs: $work/curvature-comb-edge.png, $work/curvature-comb-sketch.png"
[ -n "$shot" ] && cp "$work/curvature-comb-sketch.png" "$shot"
ui_finish 'UI curvature comb test'
