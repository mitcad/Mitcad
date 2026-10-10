#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands
# Chamfer and Revolve, the commands added through the command framework,
# end to end through the real UI, with the volumes the model reports:
#   - Chamfer of the demo block's top face (all four edges), 2 mm given as
#     the expression d3/10: the top 2 mm become a frustum.
#   - Revolve of a sketched rectangle: the axis input refuses the profile,
#     takes a sketch line, then the origin's Y axis instead; a quarter turn.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-feature-test.sh [screenshot.png]

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}

# expect_volume "log regex with one number" expected description
expect_volume() {
  local volume
  volume=$(sed -n "s/.*$1.*/\\1/p" "$UI_LOG" | tail -1)
  [ -n "$volume" ] || ui_fail "$3: no volume logged"
  awk -v v="$volume" -v e="$2" 'BEGIN { exit !((v - e) ^ 2 < (1e-6 * e) ^ 2 + 1e-4) }' ||
    ui_fail "$3: volume $volume mm3, expected $2"
  echo "ok   $3: $volume mm3"
}

ui_start_display

echo "--- Chamfer"
ui_start_app --demo
ui_step "fit (F6)"                       ui_key F6
ui_step "pick the top face"              ui_view_click 50 50
ui_expect_log "Selected: 1 face [face F2:end(" "top face selected"
ui_step "chamfer (search)"               ui_command "Chamfer"
ui_expect_log "Chamfer Edges: 1 face" "the face went to the edges input"
ui_step "type d3/10 (2 mm)"              xdotool type --delay 40 "d3/10"
ui_expect_log "Chamfer Distance: d3/10 = 2 mm" "distance from an expression"
ui_expect_log "Preview Chamfer: ok" "chamfer previewed"
ui_step "OK (Enter)"                     ui_key Return
ui_expect_log "Added chamfer (Equal Distance) on 4 edge(s)" "chamfer added"
# 48000 - (2 (60 + 40) d^2 - 4/3 d^3) for d = 2: the top is a frustum.
expect_volume "Body Body1 (F2.b0): volume 48000.000 -> \([0-9.]*\) mm3" 47610.6666667 \
  "chamfered block"
ui_stop_app

echo "--- Revolve"
ui_start_app
ui_step "create sketch on XY"             ui_create_sketch xy
ui_step "rectangle tool (R)"             ui_key r
ui_step "first corner, right of Y"       ui_view_click 56 50
ui_step "opposite corner"                ui_view_click 66 40
ui_expect_log "Added rectangle" "rectangle created"
read -r width height x <<< "$(sed -n \
  's/.*Added rectangle \([0-9.]*\) x \([0-9.]*\) mm at (\([-0-9.]*\), [-0-9.]*).*/\1 \2 \3/p' \
  "$UI_LOG" | tail -1)"
awk -v x="$x" 'BEGIN { exit !(x > 0) }' || ui_fail "the rectangle starts at x = $x, not right of Y"
echo "ok   rectangle $width x $height mm from x = $x"
ui_step "finish sketch (Ctrl+Enter)"     ui_key ctrl+Return
ui_step "revolve (search)"               ui_command "Revolve"
ui_expect_log "Revolve Profiles: 1 profile" "the profile was taken"
ui_expect_log "Panel Revolve input axis at" "panel generated"
ui_expect_log "Datum y at" "origin axes shown for the axis input"
ui_expect_log "Sketch entity F1/c4 at" "sketch lines can be picked"

# The middle of the profile: between the middles of its bottom and top lines.
read -r bx by <<< "$(ui_logged_at "Sketch entity F1/c1")"
read -r tx ty <<< "$(ui_logged_at "Sketch entity F1/c3")"
ui_step "click inside the profile"       xdotool mousemove $(((bx + tx) / 2)) $(((by + ty) / 2)) click 1
grep -q "Revolve Axis: 1" "$UI_LOG" && ui_fail "the axis input took a profile or a plane"
echo "ok   the axis input refuses the profile"
ui_step "pick the left sketch line"      ui_click_logged "Sketch entity F1/c4"
ui_expect_log "Revolve Axis: 1 sketch curve [sketch curve c4 of F1]" "sketch line as the axis"
ui_expect_log "Preview Revolve: ok" "revolve about the line previewed"
ui_step "pick the Y axis"                ui_click_logged "Datum y"
ui_expect_log "Revolve Axis: 1 axis [axis y]" "the Y axis replaced the line"
ui_step "open the extent list"           ui_click_logged "Panel Revolve input type"
ui_step "choose Angle"                   ui_key Down Return
ui_expect_log "Revolve: Extent = Angle" "partial revolve chosen"
ui_expect_log "Revolve Angle: 90 deg = 90 deg" "default angle"
[ -n "$SHOT" ] && ui_capture "$SHOT"
ui_step "OK (Enter)"                     ui_key Return
ui_expect_log "Added revolve (New Body), angle 90 deg" "revolve added"
# A quarter of the ring the rectangle sweeps about Y.
expected=$(awk -v w="$width" -v h="$height" -v x="$x" \
  'BEGIN { printf "%.6f", 3.14159265358979 * ((x + w) ^ 2 - x ^ 2) * h / 4 }')
expect_volume "New body Body1 (F2.b0): volume \([0-9.]*\) mm3" "$expected" "quarter ring"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"

ui_finish "UI feature test"
