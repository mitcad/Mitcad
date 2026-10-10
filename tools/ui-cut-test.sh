#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands app/sketch
# Cuts a hole through a block with the extrude Cut operation through the real
# UI: sketch and extrude a block, sketch a circle over it, extrude the circle
# with Operation = Cut, check the removed volume, undo.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-cut-test.sh [screenshot.png]

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}

ui_start_display
ui_start_app

ui_step "create sketch on XY"             ui_create_sketch xy
ui_step "rectangle tool (R)"             ui_key r
ui_step "first corner"                   ui_view_click 50 50
ui_step "opposite corner"                ui_view_click 65 41
ui_expect_log "Added rectangle" "rectangle created"
ui_step "finish sketch (Ctrl+Enter)"     ui_key ctrl+Return
ui_step "extrude (E)"                    ui_key e
ui_step "type the distance"              xdotool type "20"
ui_step "confirm (Enter)"                ui_key Return
ui_expect_log "Added extrude (New Body), distance 20" "block extruded"

# Fit (F6) centres the block in the sketch's top view, in the middle of the
# 3D view (clicks in percent of it: ui_view_click).
ui_step "create a second sketch on XY"   ui_create_sketch xy
ui_step "fit the block (F6)"             ui_key F6
ui_step "circle tool (C)"                ui_key c
ui_step "centre over the block"          ui_view_click 50 50
ui_step "radius point"                   ui_view_click 56 50
ui_expect_log "Added circle" "circle created"
ui_step "finish sketch (Ctrl+Enter)"     ui_key ctrl+Return

# Extrude starts with the newest unused profile: the circle. The tool goes
# 30 mm up from the sketch plane, through the 20 mm block.
ui_step "extrude (E)"                    ui_key e
ui_step "type the distance"              xdotool type "30"
# The Operation box of the command panel, where the app logged it.
ui_step "open the operation list"        ui_click_logged "Panel Extrude input operation"
ui_step "choose Cut"                     ui_key Down Down Return
ui_expect_log "Extrude: Operation = Cut" "Cut operation chosen"
if [ -n "$SHOT" ]; then
  ui_capture "$SHOT"
fi
ui_step "confirm (Enter)"                ui_key Return
ui_expect_log "Added extrude (Cut), distance 30" "cut extrude added"
ui_expect_log "Body Body1 (F2.b0): volume" "body volume reported"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"

# The block must lose a cylinder of the circle's diameter and its height.
diameter=$(sed -n 's/.*Added circle, diameter \([0-9.]*\) mm.*/\1/p' "$UI_LOG" | tail -1)
volumes=$(sed -n 's/.*Body Body1 (F2.b0): volume \([0-9.]*\) -> \([0-9.]*\) mm3.*/\1 \2/p' \
  "$UI_LOG" | tail -1)
read -r before after <<< "$volumes"
awk -v d="$diameter" -v b="$before" -v a="$after" \
  'BEGIN { hole = 3.14159265358979 * d * d / 4 * 20; exit !(d > 0 && (b - a - hole) ^ 2 < 0.01) }' ||
  ui_fail "expected a hole of diameter $diameter mm through 20 mm, volume went $before -> $after"
echo "ok   hole of diameter $diameter mm: body volume $before -> $after mm3"

ui_step "undo the cut (Ctrl+Z)"          ui_key ctrl+z
ui_step "undo the circle (Ctrl+Z)"       ui_key ctrl+z
ui_step "undo the sketch (Ctrl+Z)"       ui_key ctrl+z

ui_finish "UI cut test"
