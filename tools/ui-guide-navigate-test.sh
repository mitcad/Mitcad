#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/OcctViewer.cpp app/OcctViewer.hpp app/view/Navigation.hpp
# check-all sources: app/tutorials/GuideStep.cpp app/tutorials/GuideStep.hpp app/tutorials/TutorialGuide.cpp
# check-all sources: app/tutorials/TutorialGuide.hpp app/tutorials/MainWindowGuide.cpp
# check-all sources: app/framework/TestDriver.cpp app/framework/TestDriverNavigation.cpp
# check-all sources: app/framework/TestDriverCommands.cpp tools/ui-scripts/guide-navigate.mitcad-ui
# A tutorial's guide step done by a navigation of the view (mitcad#2369,
# GitHub mitcad/Mitcad#631) through the real view, the input script
# tools/ui-scripts/guide-navigate.mitcad-ui on the demo block: an orbit step
# stays open on a wheel zoom, a pan and a press without a move and is done
# once by an orbit drag; after the navigation scheme changes, the same step
# is done by the new scheme's orbit gesture; the wheel does a zoom step, an
# arrow key an orbit step, and an Alt + left orbit is free during a step
# aimed at a command.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-guide-navigate-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-test-lib.sh"
trap 'ui_cleanup' EXIT
export MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/guide-navigate.mitcad-ui
ui_start_display
ui_start_app --ui=qt --no-mcp --demo
for _ in $(seq 1 600); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'guide navigate driver timed out'; exit 1; }
wait "$UI_RUNNER" || { grep -E 'TestDriver|Guide: ' "$UI_LOG" | tail -20 >&2; ui_fail 'guide navigate driver failed'; exit 1; }
ui_expect_log 'Guide: step orbit done: the view orbited' 'an orbit drag completes the orbit step'
ui_expect_log 'Guide: step orbit2 done: the view orbited' "another scheme's orbit gesture completes it"
ui_expect_log 'Guide: step turn done: the view orbited' 'an arrow key turn is an orbit'
ui_expect_log 'TestDriver: done' 'guide navigate steps completed'
ui_finish 'UI guide step done by a navigation of the view'
