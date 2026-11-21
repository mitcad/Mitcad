#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/OcctViewer.cpp app/OcctViewer.hpp app/view/ViewController.cpp app/view/ViewController.hpp
# check-all sources: app/framework/FloatingLayout.cpp app/framework/TestDriver.cpp
# check-all sources: tools/ui-scripts/cube-size.mitcad-ui
# The orientation cube's size (mitcad#2071, GitHub mitcad/Mitcad#447)
# through the real UI, the shared input script
# tools/ui-scripts/cube-size.mitcad-ui (also run by
# app.ui-windows-cube-size) on a new design, Floating: 75 % and 150 % from
# the cube's right-click menu and from Preferences > Display; the cube, its
# Home button and its arrows follow the size, no two buttons overlap at
# 75 %, a click on the cube's top face still turns the view to Top, and the
# Command card on the upper right moves below the larger cube at once.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-cube-size-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-test-lib.sh"
trap 'ui_cleanup' EXIT
export MITCAD_TEST_STEP_MS=50 MITCAD_LOG_ORIENTATION_CUBE=1
export MITCAD_TEST_INPUT=$here/ui-scripts/cube-size.mitcad-ui
ui_start_display
ui_start_app --ui=qt --no-mcp --chrome=floating
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'cube size driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Orientation cube corner 75 %: area 143, offset 75' 'the corner at 75 %'
ui_expect_log 'Orientation cube corner 150 %: area 285, offset 150' 'the corner at 150 %'
ui_expect_log 'TestDriver: clear of the cube: commandDock' 'the Command card below the cube'
ui_expect_log 'Preferences: orientation cube size 150 %' 'Preferences saved the size'
ui_expect_log 'TestDriver: done' 'the orientation cube size'
ui_finish 'UI cube size'
