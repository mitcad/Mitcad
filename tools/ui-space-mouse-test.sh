#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/view/SpaceMouseController.cpp app/view/SpaceMouseController.hpp
# check-all sources: app/view/SpaceMouseNavigation.hpp app/view/SpaceMouseView.cpp app/platform/SpaceMouseDevice.hpp
# check-all sources: app/platform/unix/SpaceMouseSpnav.cpp app/framework/TestDriver.cpp tools/ui-scripts/space-mouse.mitcad-ui
# check-all sources: app/view/NavigationPreferences.hpp app/view/SpaceMouseCamera.hpp app/view/SpaceMouseTiming.hpp
# check-all sources: app/view/ViewController.cpp app/framework/TestDriverNavigation.cpp
# check-all sources: app/platform/unix/SpaceMouseMagellan.cpp app/platform/unix/SpaceMouseMagellan.hpp app/platform/SpaceMouseDriverReports.hpp
# A 3D mouse (mitcad#951) on a private Xvfb display, through the test
# driver's input (tools/ui-scripts/space-mouse.mitcad-ui): no device logs
# "3D mouse: none", a spin turns the view, a push pans it, a pull zooms in
# and the first button runs Fit, in Object mode, each motion's log line
# with its duration, frame rate, longest frame and grid rebuilds
# (mitcad#1023); the 3D Mouse tab of Preferences has its Navigation mode
# and Lock horizon (mitcad#978); Lock Rotation on the second button locks
# and frees the rotation, also in a sketch, where it starts locked
# (mitcad#1910).
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-test-lib.sh"
trap 'ui_cleanup' EXIT
ui_start_display
# OK of the Preferences dialog, as in ui-navigation-reverse-pan-test.sh.
export MITCAD_TEST_PREFERENCES_DONE=Return
MITCAD_TEST_INPUT=$here/ui-scripts/space-mouse.mitcad-ui \
  ui_start_app --ui=qt --no-mcp --demo
for _ in $(seq 1 600); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail "the 3D mouse script timed out"; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'TestDriver: done' "the 3D mouse turned, panned and zoomed the view and its button ran Fit"
ui_expect_log ' fps, longest frame ' "the 3D mouse motion's log line gives its frame rate and longest frame"
ui_expect_log ' grid rebuild(s), turned ' "the 3D mouse motion's log line counts its grid rebuilds"
ui_expect_log 'strongest ry: in Object mode the model turns counter-clockwise' "the 3D mouse axes line names what the spin did to the model"
ui_expect_log 'Status info: 3D mouse rotation locked' "Lock Rotation on a button locked the 3D mouse's rotation"
ui_expect_log '3D mouse: rotation unlocked in the sketch' "Lock Rotation freed the rotation in a sketch"
ui_finish '3D mouse'
