#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch/SketchController.cpp app/sketch/SketchController.hpp app/sketch/SketchPreferences.cpp
# check-all sources: app/sketch/EditTools.cpp app/OcctViewer.cpp app/OcctViewer.hpp app/MainWindow.cpp
# check-all sources: app/framework/TestDriver.cpp app/framework/TestDriverOrbitCenter.cpp core/sketch core/solver tools/cli
# check-all sources: tools/ui-scripts/sketch-scale-view.mitcad-ui tools/ui-scripts/sketch-scale-view.json
# The view follows a sketch its first dimension scales (mitcad#1948,
# GitHub #400), through the real UI on a design that mitcad-cli makes
# (tools/ui-scripts/sketch-scale-view.json: three free circles 600 mm and
# more from the sketch origin): a diameter of 50 on one circle scales the
# sketch about its origin by 1.25, and the circle's centre stays at its
# place in the window within a pixel, at the same zoom; undone, a diameter
# of 50 on another circle scales it by 0.8 the same way. The steps are
# tools/ui-scripts/sketch-scale-view.mitcad-ui, which
# tools/ui-windows-sketch-scale-view-test.ps1 runs on Windows and
# tools/ui-macos-sketch-scale-view-test.sh on macOS.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sketch-scale-view-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-sketch-scale-view.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/sketch-scale-view.mitcad-ui
"$cli" run "$here/ui-scripts/sketch-scale-view.json" --save "$work/circles.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/circles.mitcad"
for _ in $(seq 1 600); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'sketch scale view driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Scaled the sketch by 1.25 to its first dimension' 'the first diameter scaled the sketch up by 1.25'
ui_expect_log 'Scaled the sketch by 0.8 to its first dimension' 'the first diameter scaled the sketch down by 0.8'
ui_expect_log 'Sketch view moved with the scaled sketch by 150.000 mm' 'the view followed the circle scaled up'
ui_expect_log 'Sketch view moved with the scaled sketch by 144.000 mm' 'the view followed the circle scaled down'
ui_expect_log 'TestDriver: done' 'the dimensioned circle kept its place in the window'
ui_finish 'UI sketch view follows the first-dimension scale'
