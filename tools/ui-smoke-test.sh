#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Drives the view of the demo block with mouse and keyboard input and fails if
# the application crashes or OCCT reports a failed view operation.
#
# Runs headless on Xvfb (see ui-test-lib.sh). Usage: tools/ui-smoke-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

ui_start_display
ui_start_app --demo

ui_step "hover over body"             xdotool mousemove $(ui_at 50 50)
ui_step "click face"                  ui_click 50 45
ui_step "click edge area"             ui_click 45 60
ui_step "click empty space"           ui_click 70 15
ui_step "left drag across body"       ui_drag 1 35 30 70 70
ui_step "left drag starting on body"  ui_drag 1 50 50 75 85
ui_step "middle drag (pan)"           ui_drag 2 50 50 60 60
ui_step "shift + middle drag (orbit)" ui_drag 2 50 50 65 40 shift
ui_step "wheel zoom in"               xdotool mousemove $(ui_at 50 50) click --repeat 3 4
ui_step "wheel zoom out"              xdotool click --repeat 3 5
ui_step "click orientation cube"             ui_view_click 12 83
ui_sync # the orientation cube's animation ends, the camera rests
ui_step "fit (F6)"                    ui_key F6
ui_step "orbit after cube"            ui_drag 2 50 50 40 60 shift

ui_finish "UI smoke test"
