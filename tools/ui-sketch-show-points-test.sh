#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/OcctViewer.cpp app/OcctViewer.hpp app/sketch/SketchOverlay.cpp
# check-all sources: app/sketch/SketchController.cpp app/sketch/SketchController.hpp app/sketch/SketchPalette.cpp
# check-all sources: app/framework/TestDriver.cpp app/framework/ModelShapes.cpp core/sketch tools/cli
# check-all sources: tools/ui-scripts/sketch-show-points.mitcad-ui tools/ui-scripts/sketch-show-points.json
# The Sketch Palette's Show points (mitcad#1705, GitHub mitcad/Mitcad#262;
# mitcad#1764, GitHub mitcad/Mitcad#290) through the real UI, the shared
# input script tools/ui-scripts/sketch-show-points.mitcad-ui (also run by
# tools/ui-windows-sketch-show-points-test.ps1) on a design that mitcad-cli
# makes (tools/ui-scripts/sketch-show-points.json): on (the default), every
# point's marker is drawn; unchecked, none is, while the end point under
# the pointer still shows as the hover, a drag moves it and a click selects
# a point and draws its ball; finished with it off, the sketch's points and
# another sketch's stay without markers; the next edit of the sketch opens
# with it off again, and checked again in the other sketch every marker is
# drawn, also after that sketch is finished.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sketch-show-points-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-sketch-show-points.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/sketch-show-points.mitcad-ui
"$cli" run "$here/ui-scripts/sketch-show-points.json" --save "$work/points.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/points.mitcad"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'sketch show points driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Sketch Sketch1 opens with Show points on' 'on (the default): Sketch1 opens with its points drawn'
ui_expect_log 'Show points off' 'the palette hides the points'
ui_expect_log '0 markers over them' 'a hidden end point shows under the pointer'
ui_expect_log 'TestDriver: sketch F1 point markers: 0 drawn,' 'a sketch finished with it off keeps its points hidden'
ui_expect_log 'TestDriver: sketch F2 point markers: 0 drawn,' 'another shown sketch keeps its points hidden'
ui_expect_log 'Sketch Sketch1 opens with Show points off' 'off is remembered for the next edit'
ui_expect_log 'Sketch Sketch2 opens with Show points off' 'another sketch opens with it off'
ui_expect_log 'Show points on' 'the palette draws the points again'
ui_expect_log 'TestDriver: done' 'Show points in the Sketch Palette'
ui_finish 'UI sketch show points'
