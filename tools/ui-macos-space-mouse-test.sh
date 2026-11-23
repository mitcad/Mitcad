#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/view/SpaceMouseController.cpp app/view/SpaceMouseController.hpp
# check-all sources: app/view/SpaceMouseNavigation.hpp app/view/SpaceMouseView.cpp app/platform/SpaceMouseDevice.hpp
# check-all sources: app/platform/macos/SpaceMouseMac.cpp app/framework/TestDriver.cpp tools/ui-scripts/space-mouse.mitcad-ui
# check-all sources: app/view/NavigationPreferences.hpp app/view/SpaceMouseCamera.hpp app/view/SpaceMouseTiming.hpp
# check-all sources: app/view/ViewController.cpp app/framework/TestDriverNavigation.cpp
# check-all sources: app/platform/macos/SpaceMouse3DxWare.cpp app/platform/macos/SpaceMouse3DxWare.hpp
# check-all sources: app/framework/TestDriverViewCamera.cpp app/WindowManager.cpp app/DocumentFrame.cpp
# A 3D mouse (mitcad#951) in the macOS VM, through the test driver's input
# (tools/ui-scripts/space-mouse.mitcad-ui, the same script as the Linux and
# Windows tests'): no device logs "3D mouse: none" (IOKit HID finds none
# in the VM), a spin turns the view, a push pans it, a pull zooms in and
# the first button runs Fit, in Object mode; the 3D Mouse tab of Settings
# has its Navigation mode and Lock horizon (mitcad#978); Lock Rotation on
# the second button locks and frees the rotation, also in a sketch, where
# it starts locked (mitcad#1910); a second design shares the process's one
# connection, the input turning the active design's view only, the first
# one's again after the second closes (mitcad#2297).
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" != 2 ]; then
  echo 'usage: ui-macos-space-mouse-test.sh <app> <output>' >&2
  exit 2
fi
ui_init "$1" "$2"
UI_TIMEOUT=300
export MITCAD_SETTINGS_DIR=$UI_ROOT/settings
mkdir -p "$MITCAD_SETTINGS_DIR"
# Closes the Settings window, as in ui-macos-navigation-test.sh.
export MITCAD_TEST_PREFERENCES_DONE=ctrl+w
MITCAD_TEST_INPUT=$here/ui-scripts/space-mouse.mitcad-ui MITCAD_TEST_STEP_MS=70 \
  ui_run_app space-mouse --ui=qt --no-recovery --no-mcp --demo
ui_expect_log '^TestDriver: done' 'the 3D mouse turned, panned and zoomed the view and its button ran Fit'
ui_expect_log 'strongest ry: in Object mode the model turns counter-clockwise' \
  'the 3D mouse axes line names what the spin did to the model'
ui_expect_log 'Status info: 3D mouse rotation locked' "Lock Rotation on a button locked the 3D mouse's rotation"
ui_expect_log '3D mouse: rotation unlocked in the sketch' 'Lock Rotation freed the rotation in a sketch'
ui_expect_no_log 'TestDriver: FAIL' 'no failed 3D mouse assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: space-mouse'
