#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/SurfaceAnalysis.cpp app/commands/SurfaceAnalysis.hpp app/commands/InspectCommands.cpp
# check-all sources: app/commands/PrintAnalysis.cpp app/commands/PrintAnalysis.hpp app/MainWindowPrintAnalysis.cpp
# check-all sources: app/view/PrintOverlay.cpp app/view/PrintOverlay.hpp app/browser app/framework/Theme.cpp
# check-all sources: core/model/src/document/surface_analyses.rs core/model/src/surface_analysis.rs
# check-all sources: core/model-api/src/surface_quality.rs core/cpp/bridge/surface_quality.cpp
# check-all sources: tools/ui-scripts/surface-analysis.mitcad-ui tools/ui-scripts/surface-analysis.json tools/cli
# Surface quality analyses (mitcad#2517) through the real UI, the shared
# input script tools/ui-scripts/surface-analysis.mitcad-ui (also run by
# tools/ui-windows-surface-analysis-test.ps1 and
# tools/ui-macos-surface-analysis-test.sh) on the cylinder mitcad-cli makes
# of tools/ui-scripts/surface-analysis.json. Inspect > Zebra draws the
# stripes (logged counts), Curvature Map the side's 0.1/mm, Isocurve
# Analysis 12 curves colored by curvature, Environment Map the studio
# picture reflected; each is kept, Zebra1 shown again and edited, saved
# (checked in the file here). The view's pictures stay in the work folder
# (build/ui-surface-analysis, overwritten by the next run; the zebra's is
# copied to the path given).
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-surface-analysis-test.sh [screenshot.png]
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
shot=${1:-}
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
work=$root/build/ui-surface-analysis
mkdir -p "$work"
trap 'ui_cleanup' EXIT
file=$work/surface-part.mitcad
"$cli" run "$here/ui-scripts/surface-analysis.json" --save "$file" > "$work/cli.log" 2>&1 ||
  { cat "$work/cli.log"; echo "FAIL: mitcad-cli: the cylinder"; exit 1; }
export MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/surface-analysis.mitcad-ui
export MITCAD_TEST_WORK=$work
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$file"
for _ in $(seq 1 1500); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail "surface analysis driver timed out"; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
UI_RUNNER=""
ui_expect_log 'Inspect Zebra: direction (0, 0, 1), 3 stripes, width 0.3: Body1 dark' 'the zebra stripes'
ui_expect_log 'Kept Zebra as Zebra1' 'the zebra kept'
ui_expect_log 'Inspect Curvature Map: maximum 1/mm, limit 0.1 (automatic): Body1 min 0, max 0.1' 'the side 0.1/mm'
ui_expect_log 'Inspect Isocurve Analysis: 2 per face, curvature colored: Body1 12 curves' 'the iso curves'
ui_expect_log 'Inspect Environment Map: studio picture: Body1' 'the environment reflected'
ui_expect_log 'Kept Environment Map as Environment1' 'the environment map kept'
ui_expect_log 'TestDriver: done' 'the surface quality analyses through the UI'
for type in zebra curvature isocurve environment; do
  grep -q "\"type\": \"$type\"" "$file" || ui_fail "no $type analysis in the saved file"
done
echo "ok   the four analyses are in the saved file"
for picture in surface-zebra.png surface-curvature.png surface-isocurves.png surface-environment.png; do
  [ -s "$work/$picture" ] || ui_fail "no picture $picture"
done
echo "ok   pictures: $work/surface-zebra.png, surface-curvature.png, surface-isocurves.png, surface-environment.png"
[ -n "$shot" ] && cp "$work/surface-zebra.png" "$shot"
ui_finish 'UI surface analysis test'
