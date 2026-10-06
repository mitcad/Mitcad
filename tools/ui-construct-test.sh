#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# The CONSTRUCT group (U4) through the real UI, on the demo block (60 x 40
# x 20 mm, Body1 F2.b0): construction planes, axes and points picked from
# the origin and the block, with the geometry the model gives them
# (tools/cli/tests/construction.json); an offset plane's arrow dragged in
# the view; the plane edited from the timeline and measured from XY; a
# midplane between construction planes.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-construct-test.sh [screenshot.png]

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}
TOP='F2:end(r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]})'
RIGHT='F2:side(c2[c1,c3])'

ui_start_display
ui_start_app --demo
ui_step "fit (F6)"                         ui_key F6

echo "--- Plane at Angle: about X, 30 degrees from XY"
ui_step "plane at angle (search)"          ui_command "Plane at Angle"
ui_expect_log "Datum xy at" "origin planes shown"
ui_step "pick the X axis"                  ui_click_logged "Datum x"
ui_expect_log "Plane at Angle Line: 1 axis [axis x]" "X axis picked"
ui_step "pick XY as the reference"         ui_click_logged "Datum xy"
ui_expect_log "Plane at Angle Reference Plane: 1 plane [plane xy]" "XY the reference"
ui_expect_log "Manipulator Plane at Angle angle at" "the angle has a ring"
ui_step "type 30"                          ui_type_in "Panel Plane at Angle input angle" "30"
ui_expect_log "Preview Plane at Angle: ok" "previewed"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_log "Added plane Plane1: origin (0, 0, 0), normal (0, -0.5, 0.866025)" "Plane1 turned 30 degrees"

echo "--- Axis Through Two Planes: XZ and YZ"
ui_step "axis through two planes (search)" ui_command "Axis Through Two Planes"
ui_step "pick XZ"                          ui_click_logged "Datum xz"
ui_step "pick YZ"                          ui_click_logged "Datum yz"
ui_expect_log "Preview Axis Through Two Planes: ok" "previewed"
ui_step "OK (Enter)"                       ui_key Return
grep -E "Added axis Axis1: origin \(0, 0, 0\), direction \(0, 0, -?1\)" "$UI_LOG" > /dev/null ||
  ui_fail "Axis1 is not the Z axis: $(grep 'Added axis' "$UI_LOG" | tail -1)"
echo "ok   Axis1 along Z"

echo "--- Point Through Three Planes"
ui_step "point through three planes"       ui_command "Point Through Three Planes"
ui_step "pick XY"                          ui_click_logged "Datum xy"
ui_step "pick YZ"                          ui_click_logged "Datum yz"
ui_step "pick XZ"                          ui_click_logged "Datum xz"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_log "Added point Point1: point (0, 0, 0)" "Point1 at the origin"

echo "--- Axis Through Edge and Point at Vertex on the block"
ui_step "axis through edge (search)"       ui_command "Axis Through Edge"
ui_step "pick the top right edge"          ui_click_pick "edge F2.b0/E{$TOP|$RIGHT}"
ui_expect_log "Axis Through Edge Edge: 1 edge" "edge picked"
ui_step "OK (Enter)"                       ui_key Return
grep -E "Added axis Axis2: origin \(60, [0-9.]+, 20\), direction \(0, -?1, 0\)" "$UI_LOG" > /dev/null ||
  ui_fail "Axis2 is not along the edge: $(grep 'Added axis' "$UI_LOG" | tail -1)"
echo "ok   Axis2 along the edge"
ui_step "point at vertex (search)"         ui_command "Point at Vertex"
ui_step "pick a top vertex"                ui_click_pick "vertex F2.b0/V{F2:end"
ui_step "OK (Enter)"                       ui_key Return
grep -E "Added point Point2: point \((0|60), (0|40), 20\)" "$UI_LOG" > /dev/null ||
  ui_fail "Point2 is not a top corner: $(grep 'Added point' "$UI_LOG" | tail -1)"
echo "ok   Point2 at a top corner"

echo "--- Offset Plane: the top face, its arrow dragged, then 10 mm typed"
ui_step "offset plane (search)"            ui_command "Offset Plane"
ui_step "pick the top face"                ui_click_pick "face F2.b0/$TOP"
ui_expect_log "Offset Plane Plane: 1 face" "top face picked"
ui_expect_log "Manipulator Offset Plane distance at" "the distance has an arrow"
ui_mark
ui_step "drag the arrow up"                ui_drag_from "Manipulator Offset Plane distance" 0 -60
ui_expect_new "Manipulator Offset Plane distance dragged to" "arrow dragged"
dragged=$(grep "Offset Plane Distance: " "$UI_LOG" | tail -1 | sed -n 's/.*= \([-0-9.]*\) mm$/\1/p')
awk -v d="$dragged" 'BEGIN { exit !(d > 10) }' ||
  ui_fail "the drag gave $dragged mm, not more than the 10 mm it began with"
echo "ok   dragged to $dragged mm"
ui_step "type 10"                          ui_type_in "Panel Offset Plane input distance" "10"
ui_expect_log "Offset Plane Distance: 10 = 10 mm" "10 mm typed"
ui_expect_log "Preview Offset Plane: ok" "previewed"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_log "Added plane Plane2: origin (0, 0, 30), normal (0, 0, 1)" "Plane2 10 mm above the top"
[ -n "$SHOT" ] && ui_capture "$SHOT"

echo "--- Plane2 edited from the timeline to 25 mm, measured from XY"
ui_step "double-click Plane2"              ui_double_click_logged "Timeline Plane2"
ui_expect_log "Editing F8 with Offset Plane" "editing Plane2"
ui_expect_log "Offset Plane Distance: 10 mm" "the distance came from the feature"
ui_step "type 25"                          ui_type_in "Panel Offset Plane input distance" "25"
ui_expect_log "Preview Offset Plane: ok" "previewed"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_log "Edited F8" "Plane2 edited"
# Measure picks the origin's planes once its light bulb shows them.
ui_step "show the origin"                  ui_click_logged "Browser eye Root/Origin"
ui_step "measure (I)"                      ui_key i
ui_expect_log "Command Measure started" "measure started"
ui_step "pick Plane2"                      ui_click_logged "Datum F8"
ui_step "pick XY"                          ui_click_logged "Datum xy"
ui_expect_log "distance 45" "Plane2 is 45 mm above XY"
ui_step "close (Enter)"                    ui_key Return
ui_expect_log "Closed Measure" "measure closed"

echo "--- Midplane: between Plane2 and XY"
ui_step "midplane (search)"                ui_command "Midplane"
ui_step "pick Plane2"                      ui_click_logged "Datum F8"
ui_step "pick XY"                          ui_click_logged "Datum xy"
ui_expect_log "Preview Midplane: ok" "previewed"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_log "Added plane Plane3: origin (0, 0, 22.5), normal (0, 0, 1)" "Plane3 halfway"

grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
ui_finish "UI construct test"
