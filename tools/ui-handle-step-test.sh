#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/HandleStep.cpp app/framework/HandleStep.hpp app/framework/ManipulatorOverlay.cpp
# check-all sources: app/framework/CommandSession.cpp app/commands/FilletCommands.cpp app/commands/ExtrudeCommands.cpp
# check-all sources: app/commands/PrimitiveCommands.cpp app/commands/PlaceInputs.cpp app/commands/CommandSupport.cpp
# check-all sources: tools/cli
# The step an arrow handle's drag rounds its value to follows the size of
# the geometry as well as the zoom (mitcad#1431), through the real UI. Each
# drag logs "Manipulator <command> <input> drag step <step> (zoom step
# <step>)" when it starts; the value it ends at is a whole number of steps.
#   - A 10 mm cube framed in the view: Fillet's Radius and Chamfer's
#     Distance on its top front edge, Extrude's Distance on its top face
#     and a Box's Height step by 0.1 mm or finer. The Box's Distance 1, a
#     Place distance, steps by the zoom's step alone, as a click on the
#     face rounds it.
#   - A 200 mm block framed the same way: Fillet's and Chamfer's arrows on
#     its top front edge keep a coarse step, the zoom's or 2 mm (a
#     hundredth of the block) where the zoom's is coarser.
#   - Both in one view, the cube beside the block: the cube's edge steps by
#     0.1 mm, finer than the zoom's step, the block's as above.
# Each design is seen from its Home view (from the front, a little from the
# right and above), fitted.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-handle-step-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-handle-step.XXXXXX)
trap 'ui_cleanup; rm -rf "$WORK"' EXIT

# The top front edge (the -Y side and the top) of the box feature Fn.
top_front() { echo "E{F$1:side0|F$1:top}"; }

cat > "$WORK/cube.json" << EOF
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, 0],
    "length": 10, "width": 10, "height": 10, "operation": "new_body"}},
  {"cmd": "add_named_view", "name": "Home", "eye": [25, -75, 45], "target": [5, 5, 5], "up": [0, 0, 1],
    "height": 20}
]
EOF
cat > "$WORK/block.json" << EOF
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, 0],
    "length": 200, "width": 200, "height": 200, "operation": "new_body"}},
  {"cmd": "add_named_view", "name": "Home", "eye": [300, -700, 500], "target": [100, 100, 100], "up": [0, 0, 1],
    "height": 350}
]
EOF
cat > "$WORK/both.json" << EOF
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, 0],
    "length": 200, "width": 200, "height": 200, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [230, 0],
    "length": 10, "width": 10, "height": 10, "operation": "new_body"}},
  {"cmd": "add_named_view", "name": "Home", "eye": [320, -700, 500], "target": [120, 100, 100], "up": [0, 0, 1],
    "height": 380}
]
EOF
for design in cube block both; do
  "$CLI" run "$WORK/$design.json" --save "$WORK/$design.mitcad" > "$WORK/cli-$design.log" 2>&1 ||
    { cat "$WORK/cli-$design.log"; ui_fail "mitcad-cli ($design)"; }
done

# screen dx dy dz: the way the view shows the direction (dx, dy, dz), from
# the newest camera line: "x y", x to the right and y down on the screen
# (unit).
screen() {
  local camera
  camera=$(grep -o 'Camera direction [-0-9.]* [-0-9.]* [-0-9.]* up [-0-9.]* [-0-9.]* [-0-9.]*' "$UI_LOG" | tail -1)
  [ -n "$camera" ] || ui_fail "no camera logged"
  echo "$camera" | awk -v x="$1" -v y="$2" -v z="$3" '{
    a = $3; b = $4; c = $5; p = $7; q = $8; s = $9
    n = sqrt(a * a + b * b + c * c); a /= n; b /= n; c /= n
    k = p * a + q * b + s * c; p -= k * a; q -= k * b; s -= k * c
    n = sqrt(p * p + q * q + s * s); p /= n; q /= n; s /= n
    rx = b * s - c * q; ry = c * p - a * s; rz = a * q - b * p
    n = sqrt(x * x + y * y + z * z); x /= n; y /= n; z /= n
    u = x * rx + y * ry + z * rz; v = -(x * p + y * q + z * s); l = sqrt(u * u + v * v)
    if (l < 1e-6) { exit 1 }
    printf "%.6f %.6f\n", u / l, v / l }' || ui_fail "($1, $2, $3) points at the eye"
}

# drag_along command key pixels: drags the input's arrow along the way the
# view shows it by `pixels`. The cursor first goes 6 pixels square to the
# arrow, so that the drag starts however short the way along it is, and
# comes back onto it on the way to the end (as in
# tools/ui-fillet-handle-test.sh).
drag_along() {
  local way ux uy fx fy dx dy sx sy i
  way=$(grep -F "Arrow direction $1 $2: " "$UI_LOG" | tail -1 | sed 's/.*: //' | tr ',' ' ')
  [ -n "$way" ] || ui_fail "no direction logged for the arrow of $1 $2"
  # shellcheck disable=SC2086 # three numbers
  read -r ux uy <<< "$(screen $way)"
  read -r dx dy sx sy <<< "$(awk -v ux="$ux" -v uy="$uy" -v p="$3" '
    function near(v) { return v < 0 ? int(v - 0.5) : int(v + 0.5) }
    BEGIN { printf "%d %d %d %d", near(ux * p), near(uy * p), near(-uy * 6), near(ux * 6) }')"
  read -r fx fy <<< "$(ui_logged_at "Manipulator $1 $2")"
  ui_apart "$fx" "$fy"
  xdotool mousemove "$fx" "$fy" mousedown 1
  sleep 0.08
  xdotool mousemove $((fx + sx)) $((fy + sy))
  for i in 1 2 3 4 5 6 7 8; do
    sleep 0.08
    xdotool mousemove $((fx + sx * (8 - i) / 8 + dx * i / 8)) $((fy + sy * (8 - i) / 8 + dy * i / 8))
  done
  ui_sync
  xdotool mouseup 1
}

# stepped command key description: drags the input's arrow and prints the
# step and the zoom's step its drag logged, "step zoom"; the value it was
# dragged to is a whole number of steps.
STEP=""
ZOOM=""
stepped() {
  local line value
  ui_mark
  ui_step "drag $1's $2"                   drag_along "$1" "$2" 2
  ui_expect_new "Manipulator $1 $2 drag step " "$3: the drag's step logged"
  ui_expect_new "Manipulator $1 $2 dragged to " "$3: dragged"
  line=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -F "Manipulator $1 $2 drag step " | tail -1)
  read -r STEP ZOOM <<< "$(sed -n 's/.* drag step \([-0-9.e+]*\) (zoom step \([-0-9.e+]*\))$/\1 \2/p' <<< "$line")"
  [ -n "$STEP" ] && [ -n "$ZOOM" ] || ui_fail "$3: no step in '$line'"
  value=$(grep -F "Manipulator $1 $2 dragged to " "$UI_LOG" | tail -1 | sed 's/.* dragged to //')
  awk -v v="$value" -v s="$STEP" 'BEGIN { n = v / s; r = n < 0 ? int(n - 0.5) : int(n + 0.5); exit !((n - r) ^ 2 < 1e-12) }' ||
    ui_fail "$3: dragged to $value, not a whole number of steps of $STEP"
  echo "ok   $3: step $STEP (the zoom's $ZOOM), dragged to $value"
}

# fine description: the step is 0.1 mm or finer and not coarser than the
# zoom's.
fine() {
  awk -v s="$STEP" -v z="$ZOOM" 'BEGIN { exit !(s > 0 && s <= 0.1 * (1 + 1e-9) && s <= z * (1 + 1e-9)) }' ||
    ui_fail "$1: step $STEP, the zoom's $ZOOM: not 0.1 mm or finer"
  echo "ok   $1: steps by $STEP mm"
}

# finer description: the step is finer than the zoom's.
finer() {
  awk -v s="$STEP" -v z="$ZOOM" 'BEGIN { exit !(s < z * (1 - 1e-9)) }' ||
    ui_fail "$1: step $STEP is not finer than the zoom's $ZOOM"
  echo "ok   $1: finer than the zoom's step"
}

# coarse description: the step of the 200 mm block, the zoom's or 2 mm
# where the zoom's is coarser.
coarse() {
  awk -v s="$STEP" -v z="$ZOOM" 'BEGIN { e = z < 2 ? z : 2; exit !((s - e) ^ 2 < (1e-9 * e) ^ 2) }' ||
    ui_fail "$1: step $STEP, the zoom's $ZOOM: not the zoom's step or 2 mm"
  echo "ok   $1: steps by $STEP mm"
}

# zoomed description: the step is the zoom's own.
zoomed() {
  awk -v s="$STEP" -v z="$ZOOM" 'BEGIN { exit !((s - z) ^ 2 < (1e-9 * z) ^ 2) }' ||
    ui_fail "$1: step $STEP is not the zoom's $ZOOM"
  echo "ok   $1: steps by the zoom's $STEP mm"
}

# cancel command: Esc ends the command.
cancel() {
  ui_focus_main
  ui_mark
  ui_step "cancel (Esc)"                   ui_key Escape
  ui_expect_new "Command $1 cancelled" "$1 cancelled"
  ui_wait_idle
}

# open_design design: the design from its Home view, fitted.
open_design() {
  ui_start_app --open "$WORK/$1.mitcad"
  ui_step "the Home view (search)"         ui_command "Home"
  ui_expect_log "Camera direction -0.218 0.873 -0.436" "seen from the front, a little from the right and above"
  ui_step "fit (F6)"                       ui_key F6
  ui_sync
  ui_wait_idle
}

# edge_handle command key body feature: the command on the top front edge
# of the box feature `feature` (body F<body>.b0), its arrow shown.
edge_handle() {
  ui_mark
  ui_step "pick the top front edge"        ui_click_pick "edge F$3.b0/$(top_front "$4")"
  ui_expect_new "$1 Edges: 1 edge" "the edge picked"
  ui_expect_new "Manipulator $1 $2 at" "$1 has an arrow"
  ui_wait_idle
  ui_sync
}

ui_start_display

echo "=== A 10 mm cube framed"
open_design cube
echo "--- Fillet"
ui_step "fillet (F)"                       ui_key f
edge_handle Fillet sets.0.radius 1 1
stepped Fillet sets.0.radius "the cube's fillet"
fine "the cube's fillet"
cancel Fillet
echo "--- Chamfer"
ui_step "chamfer (search)"                 ui_command "Chamfer"
edge_handle Chamfer sets.0.distance 1 1
stepped Chamfer sets.0.distance "the cube's chamfer"
fine "the cube's chamfer"
cancel Chamfer
echo "--- Extrude its top face"
ui_step "extrude (E)"                      ui_key e
ui_mark
ui_step "pick the top face"                ui_click_pick "face F1.b0/F1:top"
ui_expect_new "Manipulator Extrude distance at" "Extrude has an arrow"
# The default distance (10 mm, the cube's height) puts the arrow above the
# framed view; 1 mm brings it back over the cube.
ui_mark
ui_step "distance 1"                       ui_type_in "Panel Extrude input distance" "1"
ui_expect_new "Manipulator Extrude distance at" "the arrow over the cube"
ui_wait_idle
ui_sync
stepped Extrude distance "the top face's extrusion"
fine "the top face's extrusion"
cancel Extrude
echo "--- A Box on the XY plane"
ui_step "box (search)"                     ui_command "Box"
ui_place_on_xy Box
# The default box (30 by 20 by 10 mm) reaches under the command panel in
# the view framed on the cube; a 4 mm box keeps its arrows in the view.
ui_mark
ui_step "length 4"                         ui_type_in "Panel Box input length" "4"
ui_step "width 4"                          ui_type_in "Panel Box input width" "4"
ui_step "height 4"                         ui_type_in "Panel Box input height" "4"
ui_expect_new "Manipulator Box height at" "Height has an arrow"
ui_wait_idle
ui_sync
stepped Box height "the box's height"
fine "the box's height"
stepped Box at_offset1 "the box's Distance 1"
zoomed "the box's Distance 1, as a click rounds it"
cancel Box
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
ui_stop_app

echo "=== A 200 mm block framed"
open_design block
echo "--- Fillet"
ui_step "fillet (F)"                       ui_key f
edge_handle Fillet sets.0.radius 1 1
stepped Fillet sets.0.radius "the block's fillet"
coarse "the block's fillet"
cancel Fillet
echo "--- Chamfer"
ui_step "chamfer (search)"                 ui_command "Chamfer"
edge_handle Chamfer sets.0.distance 1 1
stepped Chamfer sets.0.distance "the block's chamfer"
coarse "the block's chamfer"
cancel Chamfer
ui_stop_app

echo "=== The cube beside the block, both framed"
open_design both
ui_step "fillet (F)"                       ui_key f
edge_handle Fillet sets.0.radius 2 2
stepped Fillet sets.0.radius "the cube's fillet beside the block"
fine "the cube's fillet beside the block"
finer "the cube's fillet beside the block"
cancel Fillet
ui_step "fillet (F)"                       ui_key f
edge_handle Fillet sets.0.radius 1 1
stepped Fillet sets.0.radius "the block's fillet beside the cube"
coarse "the block's fillet beside the cube"
cancel Fillet
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
ui_finish "UI handle step test"
