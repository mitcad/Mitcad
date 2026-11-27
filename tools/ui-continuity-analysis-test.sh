#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/ContinuityAnalysis.cpp app/commands/ContinuityAnalysis.hpp app/commands/InspectCommands.cpp
# check-all sources: app/OcctViewer.cpp app/framework/CommandSession.cpp app/framework/Theme.cpp
# check-all sources: core/model-api/src/continuity.rs core/ffi/src/kernel/edge_continuity.rs core/cpp/bridge/edge_continuity.cpp
# check-all sources: geometry/src/edge_continuity.cpp geometry/include/mitcad/geometry/edge_continuity.hpp
# check-all sources: tools/ui-scripts/continuity-analysis.mitcad-ui tools/ui-scripts/continuity-analysis.json tools/cli
# Continuity Analysis (mitcad#2521) through the real UI, the shared input
# script tools/ui-scripts/continuity-analysis.mitcad-ui (also run by
# tools/ui-windows-continuity-analysis-test.ps1 and
# tools/ui-macos-continuity-analysis-test.sh) on the rounded box mitcad-cli
# makes of tools/ui-scripts/continuity-analysis.json. Inspect > Continuity
# Analysis with the tangent rounding's face picked gives its long edges G1
# and its ends G0; with the body picked in the browser the curvature
# continuous rounding's edges are G2; a curvature tolerance of 150 % makes
# the tangent rounding's G2 too; Escape keeps nothing. The view's pictures
# stay in the work folder (build/ui-continuity-analysis, overwritten by the
# next run; a picture is copied to the path given).
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-continuity-analysis-test.sh [screenshot.png]
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
shot=${1:-}
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
work=$root/build/ui-continuity-analysis
mkdir -p "$work"
trap 'ui_cleanup' EXIT
file=$work/continuity-part.mitcad
"$cli" run "$here/ui-scripts/continuity-analysis.json" --save "$file" > "$work/cli.log" 2>&1 ||
  { cat "$work/cli.log"; echo "FAIL: mitcad-cli: the rounded box"; exit 1; }
export MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/continuity-analysis.mitcad-ui
export MITCAD_TEST_WORK=$work
export MITCAD_LOG_PICKS=1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$file"
for _ in $(seq 1 1500); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail "continuity analysis driver timed out"; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
UI_RUNNER=""
ui_expect_log 'Inspect Continuity Analysis: 4 edges: g2 0, g1 2, g0 2, none 0, unmeasured 0' 'the tangent rounding: G1 along, G0 at its ends'
ui_expect_log 'Preview colored edges: ContinuityG1 2, ContinuityG0 2' 'its edges drawn by class'
ui_expect_log 'Command Continuity Analysis cancelled' 'closed, nothing kept'
ui_expect_log 'TestDriver: done' 'Continuity Analysis through the UI'
for picture in continuity-face.png continuity-body.png; do
  [ -s "$work/$picture" ] || ui_fail "no picture $picture"
done
echo "ok   pictures of the edges by class: $work/continuity-face.png, $work/continuity-body.png"
[ -n "$shot" ] && cp "$work/continuity-body.png" "$shot"
ui_finish 'UI continuity analysis test'
