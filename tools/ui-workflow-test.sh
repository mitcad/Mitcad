#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# The basic modelling workflow through the real UI: sketch a rectangle,
# extrude it (the sketch hides; deleting the extrusion shows it again,
# mitcad#7), fillet the edges of a picked face, sketch a circle, undo.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-workflow-test.sh [screenshot.png]

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}

ui_start_display
ui_start_app

# Clicks are given in percent of the 3D view (ui_view_click); the empty
# sketch view is centred on the origin. S opens the command search.
ui_step "create sketch on XY"             ui_create_sketch xy
ui_step "rectangle tool (R)"             ui_key r
ui_step "first corner at the origin"     ui_view_click 50 50
ui_step "opposite corner"                ui_view_click 65 41
ui_expect_log "Added rectangle" "rectangle created"
ui_step "finish sketch (Ctrl+Enter)"     ui_key ctrl+Return
ui_expect_log "Sketches shown: F1" "the sketch's curves are shown"

ui_mark
ui_step "extrude (E)"                    ui_key e
ui_step "type the distance"              xdotool type "20"
ui_step "confirm (Enter)"                ui_key Return
ui_expect_new "Added extrude (New Body), distance 20" "extrusion built"
ui_expect_new "Sketches shown: none" "the used sketch is hidden"

# Deleting the sketch's only consumer shows it again; undo hides it.
ui_mark
ui_step "right-click Extrude1"           ui_click_logged "Timeline Extrude1" 3
ui_step "choose Delete"                  ui_menu_choose "Delete"
ui_expect_new "Deleted Extrude1" "the extrusion deleted"
ui_expect_new "Sketches shown: F1" "the sketch's curves are back"
ui_mark
ui_step "undo the delete (Ctrl+Z)"       ui_key ctrl+z
ui_expect_new "Undo: Delete Extrude1" "the extrusion is back"
ui_expect_new "Sketches shown: none" "and the sketch hidden again"

ui_step "fit (F6)"                       ui_key F6
ui_step "fillet (F)"                     ui_key f
ui_step "pick the top face"              ui_click_pick "face F2.b0/F2:end("
ui_step "confirm (Enter)"                ui_key Return
ui_expect_log "Added fillet on 4 edge(s)" "fillet on the face's edges"
[ -n "$SHOT" ] && ui_capture "$SHOT"

ui_step "create a second sketch on XY"   ui_create_sketch xy
ui_step "circle tool (C)"                ui_key c
ui_step "centre"                         ui_view_click 13 50
ui_step "radius point"                   ui_view_click 18 50
ui_expect_log "Added circle" "circle created"
ui_step "cancel tool (Esc)"              ui_key Escape
ui_step "undo the circle (Ctrl+Z)"       ui_key ctrl+z
ui_step "undo the sketch (Ctrl+Z)"       ui_key ctrl+z
ui_step "undo the fillet (Ctrl+Z)"       ui_key ctrl+z

ui_finish "UI workflow test"
