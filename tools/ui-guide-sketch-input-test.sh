#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/tutorials/GuideStep.cpp app/tutorials/GuideStep.hpp app/tutorials/TutorialGuide.cpp
# check-all sources: app/tutorials/TutorialGuide.hpp app/tutorials/MainWindowGuide.cpp app/MainWindow.cpp
# check-all sources: app/sketch/SketchController.cpp app/sketch/SketchController.hpp app/sketch/SketchSnap.cpp
# check-all sources: app/sketch/SketchSnap.hpp app/sketch/DrawTools.cpp app/framework/TestDriver.cpp
# check-all sources: app/framework/TestDriverCommands.cpp app/framework/TestDriverSketch.cpp
# check-all sources: tools/ui-scripts/guide-sketch-input.mitcad-ui
# A tutorial's guide steps on the learner's sketch input one interaction at
# a time (mitcad#2440, GitHub mitcad/Mitcad#628) through the real UI, the
# shared input script tools/ui-scripts/guide-sketch-input.mitcad-ui (also
# run by tools/ui-windows-guide-sketch-input-test.ps1 and
# tools/ui-macos-guide-sketch-input-test.sh) in an empty design: a plane
# step holds back a sketch on XZ and is done by one on XY; a point step
# holds back the 2-Point Rectangle's first corner off the origin and is
# done by the one at the origin; a value step holds back the opposite
# corner without the width typed and Enter with another width, and is done
# by the width typed.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-guide-sketch-input-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-test-lib.sh"
trap 'ui_cleanup' EXIT
export MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/guide-sketch-input.mitcad-ui
ui_start_display
ui_start_app --ui=qt --no-mcp
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'guide sketch input driver timed out'; exit 1; }
wait "$UI_RUNNER" || { grep -E 'TestDriver|Guide: ' "$UI_LOG" | tail -20 >&2; ui_fail 'guide sketch input driver failed'; exit 1; }
ui_expect_log 'Guide: step plane plane xz, not its XY' 'a sketch on another plane is held back'
ui_expect_log 'Guide: step plane done: a sketch on XY' 'a sketch on the plane completes the plane step'
ui_expect_log 'Guide: step corner done: a point at origin' 'the first corner at the origin completes the point step'
ui_expect_log 'Guide: step width value 25 in width, not its 40' 'Enter with another width is held back'
ui_expect_log 'Guide: step width done: 40 typed into width' 'the width typed completes the value step'
ui_expect_log 'TestDriver: done' 'guide sketch input steps completed'
ui_finish 'UI guide steps on sketch input'
