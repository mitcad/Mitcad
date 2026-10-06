#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Sketch mode (U2) through the real UI, as a user works:
#   - Create Sketch asks for the plane; the view looks at it.
#   - A closed profile drawn with the line tool, its horizontal and
#     vertical constraints inferred while drawing; dimensions placed with
#     the sketch dimension tool, one an expression, until the sketch is
#     fully constrained (0 DOF); an over-constraining dimension becomes
#     driven, an over-constraining constraint is refused.
#   - Drag with the solver: a fully constrained corner stays, a free line
#     end follows. Trim.
#   - Extrude the profile and check the volume the model reports; edit the
#     sketch from the timeline, change a dimension by double-click, check the
#     body follows, undo the edit as one step.
#   - Each other tool on a sample in a second sketch: arc, polygon, slot,
#     ellipse, spline, point, text, offset, mirror, patterns, fillet,
#     chamfer, a constraint, construction, delete, project.
#   - The tools' pointer (mitcad#3): a bitmap cursor of Mitcad's own with
#     the hot spot in the middle of its cross; the arrow again when the
#     tool ends and when the sketch is finished with a tool active.
#
# Clicks are given in sketch millimetres (ui_sketch_click), from the
# placement the app logs.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sketch-test.sh [screenshot.png]

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}

# expect_count "text" n description: the log has n lines with the text.
expect_count() {
  local count
  count=$(grep -cF -- "$1" "$UI_LOG")
  [ "$count" = "$2" ] || ui_fail "$3: $count lines with '$1', expected $2"
  echo "ok   $3"
}

# expect_near "log regex with two numbers" x y description
expect_near() {
  local at
  at=$(sed -n "s/.*$1.*/\\1 \\2/p" "$UI_LOG" | tail -1)
  [ -n "$at" ] || ui_fail "$4: not in the log"
  read -r ax ay <<< "$at"
  awk -v ax="$ax" -v ay="$ay" -v x="$2" -v y="$3" \
    'BEGIN { exit !((ax - x) ^ 2 + (ay - y) ^ 2 < 1.0) }' ||
    ui_fail "$4: at ($ax, $ay), expected near ($2, $3)"
  echo "ok   $4: ($ax, $ay)"
}

# zoom_to_show x1 y1 x2 y2: zooms out (the wheel at the view's middle)
# until the sketch rectangle from (x1, y1) to (x2, y2) mm is in the view.
zoom_to_show() {
  local area vx vy vw vh ax ay bx by
  for _ in $(seq 1 20); do
    area=$(grep -o "View area [0-9]* [0-9]* [0-9]* [0-9]*" "$UI_LOG" | tail -1)
    read -r _ _ vx vy vw vh <<< "$area"
    read -r ax ay <<< "$(ui_sketch_at "$1" "$2")"
    read -r bx by <<< "$(ui_sketch_at "$3" "$4")"
    if [ "$ax" -gt $((X + vx + 20)) ] && [ "$bx" -lt $((X + vx + vw - 20)) ] &&
      [ "$by" -gt $((Y + vy + 20)) ] && [ "$ay" -lt $((Y + vy + vh - 20)) ]; then
      return 0
    fi
    xdotool mousemove $(ui_view_at 50 50) click 5
    ui_sync
  done
  ui_fail "the view did not zoom out to show ($1, $2) to ($3, $4)"
}

# double_click_at x y: a double click at a screen position.
double_click_at() { xdotool mousemove "$1" "$2" click --repeat 2 --delay 90 1; }

# select_at x y [ctrl]: a click on sketch geometry at sketch mm.
select_at() {
  if [ "${3:-}" = ctrl ]; then
    xdotool keydown ctrl
    ui_sketch_click "$1" "$2"
    xdotool keyup ctrl
  else
    ui_sketch_click "$1" "$2"
  fi
}

# The standard pointer size: the precision cursor is 32 x 32 pixels.
export XCURSOR_SIZE=24
CURSOR="Sketch cursor: bitmap 32x32, hot spot 10,10, ratio 1, icon line"

ui_start_display
ui_start_app

echo "--- Create Sketch asks for a plane and looks at it"
ui_step "create sketch on XY"            ui_create_sketch xy
ui_expect_log "Sketch view" "the sketch's place in the view is logged"
ui_expect_log "Sketch Sketch1: 0 DOF" "an empty sketch"

echo "--- A closed profile with the line tool; constraints inferred while drawing"
ui_mark
ui_step "line tool (L)"                  ui_key l
ui_expect_log "Sketch tool Line" "line tool active"
ui_expect_new "$CURSOR" "the precision cursor with the line's icon"
ui_step "start at the origin"            ui_sketch_click 0 0
ui_step "to the right"                   ui_sketch_click 40 0
ui_step "up"                             ui_sketch_click 40 24
ui_step "to the left"                    ui_sketch_click 0 24
ui_step "back to the start"              ui_sketch_click 0 0
ui_expect_log "Line chain closed" "the chain closed on its start"
expect_count ") (horizontal)" 2 "two lines inferred horizontal"
expect_count ") (vertical)" 2 "two lines inferred vertical"
ui_expect_log "Sketch Sketch1: 2 DOF" "width and height are free (2 DOF)"
ui_mark
ui_step "end the tool (Esc)"             ui_key Escape
ui_expect_new "Sketch cursor: arrow" "the arrow without a tool"

echo "--- Dimensions, one an expression, until fully constrained"
ui_step "sketch dimension (D)"           ui_key d
ui_step "pick the bottom line"           ui_sketch_click 20 0
ui_step "place the value below it"       ui_sketch_click 20 -10
ui_expect_log "Sketch value editor at" "the value editor opened"
ui_step "type 40"                        xdotool type --delay 40 "40"
ui_step "Enter"                          ui_key Return
ui_expect_log "Added dimension k" "length dimension added"
ui_expect_log "Sketch Sketch1: 1 DOF" "one degree of freedom left"
ui_step "pick the right line"            ui_sketch_click 40 12
ui_step "place the value beside it"      ui_sketch_click 52 12
ui_step "type d1/2+5"                    xdotool type --delay 40 "d1/2+5"
ui_step "Enter"                          ui_key Return
ui_expect_log "= d1/2+5" "a dimension from an expression"
ui_expect_log "Sketch Sketch1: 0 DOF, fully constrained" "fully constrained"
ui_step "pick the top line"              ui_sketch_click 20 25
ui_step "place the value above it"       ui_sketch_click 20 34
ui_step "accept the value (Enter)"       ui_key Return
ui_expect_log "Added driven dimension" "an over-constraining dimension is driven"
ui_step "end the tool (Esc)"             ui_key Escape
[ -n "$SHOT" ] && ui_capture "$SHOT"

echo "--- A constraint that over-constrains is refused"
ui_step "perpendicular (search)"         ui_command "Perpendicular"
ui_step "pick the bottom line"           ui_sketch_click 20 0
ui_step "pick the right line"            ui_sketch_click 40 12
ui_expect_log "Constraint perpendicular refused" "refused with the model's reason"
ui_step "end the tool (Esc)"             ui_key Escape

echo "--- Drag with the solver"
ui_step "drag the top right corner"      ui_sketch_drag 40 25 46 31
grep -qE "Dragged p5 to \(40.000, 25.000\)|Drag of p5 did not move it" "$UI_LOG" ||
  ui_fail "the fully constrained corner moved"
echo "ok   the fully constrained corner stays"
# Points snap to the grid's minor lines, 10 mm apart at this zoom.
ui_step "line tool (L)"                  ui_key l
ui_step "free line: start"               ui_sketch_click 60 -20
ui_step "free line: end"                 ui_sketch_click 80 -10
ui_step "end the chain (Esc)"            ui_key Escape
ui_step "end the tool (Esc)"             ui_key Escape
ui_step "drag the free end"              ui_sketch_drag 80 -10 90 0
expect_near "Dragged p[0-9]* to (\([-0-9.]*\), \([-0-9.]*\))" 90 0 "the free end followed the cursor"

echo "--- Trim"
ui_step "line tool (L)"                  ui_key l
ui_step "line A: start"                  ui_sketch_click 100 -30
ui_step "line A: end"                    ui_sketch_click 100 10
ui_step "end the chain (Esc)"            ui_key Escape
ui_step "line B: start"                  ui_sketch_click 90 0
ui_step "line B: end"                    ui_sketch_click 120 0
ui_step "end the tool (Esc Esc)"         ui_key Escape Escape
ui_step "trim (T)"                       ui_key t
ui_step "click B beyond A"               ui_sketch_click 112 0
ui_expect_log "Trimmed c" "the piece beyond A was trimmed"
ui_step "end the tool (Esc)"             ui_key Escape
ui_step "finish sketch (Ctrl+Enter)"     ui_key ctrl+Return
ui_expect_log "Sketch finished" "sketch finished"
# The browser tells the free lines' degrees of freedom (P9).
ui_expect_log "Browser sketch DOF: Sketch1 " "the browser knows the sketch's DOF"
grep -qE "Browser sketch DOF: Sketch1 [1-9]" "$UI_LOG" || ui_fail "the free lines have no DOF in the browser"
echo "ok   not fully constrained: no lock in the browser"

echo "--- Extrude the profile; the model's volume"
ui_step "extrude (E)"                    ui_key e
ui_step "type d1/4"                      xdotool type --delay 40 "d1/4"
ui_expect_log "Extrude Distance: d1/4 = 10 mm" "distance from the sketch's dimension"
ui_step "OK (Enter)"                     ui_key Return
ui_expect_log "New body Body1 (F2.b0): volume 10000.000 mm3" "40 x 25 x 10 extruded"

echo "--- Edit the sketch from the timeline, a dimension by double-click"
ui_expect_log "Timeline Sketch1 at" "the timeline is logged"
read -r tx ty <<< "$(ui_logged_at "Timeline Sketch1")"
ui_step "double-click Sketch1"           double_click_at "$tx" "$ty"
ui_expect_log "Editing sketch F1 (Sketch1)" "sketch edited with the timeline rolled back"
ui_sync
read -r dx dy <<< "$(ui_sketch_at 20 -10)"
ui_step "double-click the 40"            double_click_at "$dx" "$dy"
ui_expect_log "Editing dimension" "the dimension's value editor opened"
ui_step "type 50"                        xdotool type --delay 40 "50"
ui_step "Enter"                          ui_key Return
ui_expect_log "= 50 mm (d1)" "the dimension changed"
ui_step "finish sketch (Ctrl+Enter)"     ui_key ctrl+Return
ui_expect_log "Body Body1 (F2.b0): volume 18750.000 mm3" "the body follows: 50 x 30 x 12.5"
ui_step "undo (Ctrl+Z)"                  ui_key ctrl+z
ui_expect_log "Undo: Edit Sketch1" "the sketch edit is one undo step"


echo "--- Each tool on a sample, in a second sketch"
# The view was fitted to the block: zoom out until the samples are in it.
# Sample points are on a 10 mm grid, so grid snapping keeps them at any
# zoom; the lower left of the view, where the orientation cube is, stays empty.
ui_step "create sketch on XY"            ui_create_sketch xy
ui_step "zoom out"                       zoom_to_show -110 -95 115 95
ui_step "circle tool (C)"                ui_key c
ui_step "circle: centre"                 ui_sketch_click -80 60
ui_step "circle: rim"                    ui_sketch_click -70 60
ui_expect_log "Added circle, diameter 20 mm at (-80, 60)" "circle"
ui_step "3-point arc (search)"           ui_command "3-Point Arc"
ui_step "arc: start"                     ui_sketch_click -100 20
ui_step "arc: end"                       ui_sketch_click -60 20
ui_step "arc: through"                   ui_sketch_click -80 40
ui_expect_log "Added arc" "3-point arc"
ui_step "polygon (search)"               ui_command "Circumscribed Polygon"
ui_step "polygon: centre"                ui_sketch_click -80 -30
ui_step "polygon: a side's middle"       ui_sketch_click -70 -30
ui_expect_log "Added polygon (6 sides)" "hexagon"
ui_step "slot (search)"                  ui_command "Center to Center Slot"
ui_step "slot: first end"                ui_sketch_click 60 60
ui_step "slot: second end"               ui_sketch_click 100 60
ui_step "slot: width"                    ui_sketch_click 100 70
ui_expect_log "Added slot, width 20 mm" "slot"
ui_step "ellipse (search)"               ui_command "Ellipse"
ui_step "ellipse: centre"                ui_sketch_click 80 20
ui_step "ellipse: major axis"            ui_sketch_click 100 20
ui_step "ellipse: minor radius"          ui_sketch_click 80 30
ui_expect_log "radii 20 and 10 mm" "ellipse"
ui_step "spline (search)"                ui_command "Fit Point Spline"
ui_step "spline: 1"                      ui_sketch_click 50 -20
ui_step "spline: 2"                      ui_sketch_click 70 -10
ui_step "spline: 3"                      ui_sketch_click 90 -30
ui_step "spline: finish (Enter)"         ui_key Return
ui_expect_log "through 3 points" "fit point spline"
ui_step "point (search)"                 ui_command "Point"
ui_step "point"                          ui_sketch_click 60 -50
ui_expect_log "Added point p" "point"
ui_step "text (search)"                  ui_command "Text"
ui_step "text: place"                    ui_sketch_click -30 90
ui_step "text: type"                     xdotool type --delay 40 "M1"
ui_step "text: Enter"                    ui_key Return
ui_expect_log 'Added text t' "text"
ui_step "a line for the mirror (L)"      ui_key l
ui_step "mirror line: start"             ui_sketch_click 50 -40
ui_step "mirror line: end"               ui_sketch_click 50 -90
ui_step "end the tool (Esc Esc)"         ui_key Escape Escape
ui_step "rectangle tool (R)"             ui_key r
ui_step "rectangle: first corner"        ui_sketch_click 10 -80
ui_step "rectangle: opposite corner"     ui_sketch_click 30 -60
ui_step "end the tool (Esc)"             ui_key Escape

echo "--- Commands with panels on the selection"
ui_step "select the circle"              select_at -70 60
ui_expect_log "Selected: 1 sketch curve" "circle selected"
ui_step "offset (O)"                     ui_key o
ui_expect_log "Offset Curves: 1 sketch curve" "the circle went to the offset"
ui_expect_log "Preview Offset: ok" "offset previewed"
ui_step "OK (Enter)"                     ui_key Return
ui_expect_log "Offset 1 curve(s) by 5 mm" "offset"
ui_step "select the point"               select_at 60 -50
ui_step "mirror (search)"                ui_command "Mirror"
ui_step "pick the mirror line"           ui_sketch_click 50 -70
ui_expect_log "Preview Mirror: ok" "mirror previewed"
ui_step "OK (Enter)"                     ui_key Return
ui_expect_log "Mirrored 1 object(s)" "mirror"
ui_step "select the point"               select_at 60 -50
ui_step "circular pattern (search)"      ui_command "Circular Pattern"
ui_expect_log "Preview Circular Pattern: ok" "circular pattern previewed"
ui_step "OK (Enter)"                     ui_key Return
ui_expect_log "Circular pattern of 1 object(s), 3 in all" "circular pattern"
ui_step "select the point"               select_at 60 -50
ui_step "rectangular pattern (search)"   ui_command "Rectangular Pattern"
ui_expect_log "Preview Rectangular Pattern: ok" "rectangular pattern previewed"
ui_step "OK (Enter)"                     ui_key Return
ui_expect_log "Rectangular pattern of 1 object(s), 3 x 1" "rectangular pattern"
ui_step "select the bottom line"         select_at 20 -80
ui_step "and the right line (Ctrl)"      select_at 30 -70 ctrl
ui_step "sketch fillet (search)"         ui_command "Sketch Fillet"
ui_expect_log "Preview Sketch Fillet: ok" "fillet previewed"
ui_step "OK (Enter)"                     ui_key Return
ui_expect_log "Sketch fillet of" "sketch fillet"
ui_step "select the top line"            select_at 20 -60
ui_step "and the left line (Ctrl)"       select_at 10 -70 ctrl
ui_step "sketch chamfer (search)"        ui_command "Sketch Chamfer"
ui_expect_log "Preview Sketch Chamfer: ok" "chamfer previewed"
ui_step "OK (Enter)"                     ui_key Return
ui_expect_log "Added Sketch Chamfer" "sketch chamfer"

echo "--- A constraint, construction, delete, undo"
ui_step "line tool (L)"                  ui_key l
ui_step "slanted line: start"            ui_sketch_click -40 70
ui_step "slanted line: end"              ui_sketch_click -20 80
ui_step "end the tool (Esc Esc)"         ui_key Escape Escape
ui_step "horizontal/vertical (search)"   ui_command "Horizontal/Vertical"
ui_step "pick the slanted line"          ui_sketch_click -30 75
ui_expect_log "Added constraint horizontal" "horizontal constraint added"
ui_step "end the tool (Esc)"             ui_key Escape
ui_step "select the ellipse"             select_at 80 30
ui_step "construction (X)"               ui_key x
ui_expect_log "Construction on:" "the ellipse is construction geometry"
ui_step "undo (Ctrl+Z)"                  ui_key ctrl+z
ui_expect_log "Undo: Construction in Sketch2" "undone"
ui_step "select the point"               select_at 60 -50
ui_step "delete (Delete)"                ui_key Delete
ui_expect_log "Deleted p" "the point deleted"

echo "--- Project the block's top face"
ui_step "project (P)"                    ui_key p
ui_step "pick the top face"              ui_sketch_click 20 12
ui_expect_log "Project Geometry: 1 face" "the face went to the projection"
ui_expect_log "Preview Project: ok" "projection previewed"
ui_step "OK (Enter)"                     ui_key Return
ui_expect_log "Projected 1 item(s)" "projected"
ui_mark
ui_step "undo the projection (Ctrl+Z)"   ui_key ctrl+z
ui_expect_new "Undo: " "projection undone"
ui_mark
ui_step "right-click the top face"       ui_sketch_click 20 12 3
ui_expect_new "Context menu: " "context menu opened"
grep "Context menu: " "$UI_LOG" | tail -1 | grep -qE "(:|\|) Project \|" ||
  ui_fail "no Project in the context menu of a face in sketch mode"
echo "ok   Project offered for a face in sketch mode"
ui_step "choose Project"                 ui_menu_choose "Project"
ui_expect_new "Project Geometry: 1 face" "the right-clicked face went to the projection"
ui_expect_new "Preview Project: ok" "projection previewed"
ui_step "OK (Enter)"                     ui_key Return
ui_expect_new "Projected 1 item(s)" "projected from the context menu"
[ -n "$SHOT" ] && ui_capture "${SHOT%.png}-tools.png"
ui_mark
ui_step "line tool (L)"                  ui_key l
ui_expect_new "$CURSOR" "the precision cursor again"
ui_mark
ui_step "finish sketch (Ctrl+Enter)"     ui_key ctrl+Return
ui_expect_new "Sketch cursor: arrow" "leaving the sketch restores the arrow"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"

ui_finish "UI sketch test"
