#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/framework/ModelShapes.cpp tools/cli
# check-all sources: core/sketch core/model-api/src/sketch.rs core/solver
# check-all sources: app/commands/SketchCommands.cpp app/framework/CommandSession.cpp
# check-all sources: app/MainWindowSelection.cpp app/OcctViewer.cpp
# The sketch's axes and origin as picks of Sketch Dimension and the
# constraint tools (mitcad#500), on a design mitcad-cli makes: Sketch1 (F1)
# on XY with a free point p1 at (20, 12), a free line c2 from (40, 5) to
# (70, 20) (26.57 degrees to the x axis) and a free point p5 at (-8, 15);
# nothing at the origin (8 DOF).
#   1. Sketch Dimension: p1, then the x axis away from the origin: the
#      value editor offers 12.00 (p1's distance from the axis); 30 typed
#      makes the x axis (with the origin point) and a point_line_distance,
#      one step, and p1 goes to y = 30 (7 DOF).
#   2. Sketch Dimension: the line, then the x axis (now the line the step
#      made), the value placed between them: 26.57 offered, 25 typed: the
#      line turns to 25 degrees (6 DOF).
#   3. Coincident: p5, then the y axis: the y axis is made and p5 goes onto
#      it (5 DOF); undo takes both back (6 DOF), redo makes them again.
#   4. The saved design: the axes' lines flagged, the dimensions' values,
#      p5 at x = 0.
#   5. Symmetry with a selection (mitcad#865): Sketch2 (F2, also on XY)
#      has two free circles, about (-30, 20) radius 4 and (-27, -16)
#      radius 5 (6 DOF). Both selected, Symmetry takes them as its first
#      picks; one click on the x axis then adds the constraint (3 DOF).
#   6. Mirror about the y axis (mitcad#876): Sketch3 (F3, on XY) has a
#      free point p1 at (-8, 5) and a free circle c2 about (10, 12) radius
#      3. The circle selected, Mirror opens with Mirror Line active; a
#      click on the y axis picks it (no sketch entity there), the preview
#      and OK mirror the circle to (-10, 12), the y axis made in the same
#      step; the saved sketch has the copy symmetric about the axis.
#   7. The axes selected before a tool (mitcad#899): Sketch4 (F4, on XY)
#      has a free point p1 at (-6, 8) (2 DOF). A click on p1 and a Ctrl+
#      click on the x axis select both with no tool running; Sketch
#      Dimension then offers p1's distance from the x axis, 8.00, and 15
#      typed makes the x axis and the dimension in one step (1 DOF). p1
#      and the y axis selected the same way, which makes nothing,
#      Coincident makes the y axis in its step and puts p1 on it (0 DOF);
#      undo takes both back.
#   8. Drawing onto an axis (mitcad#948): Sketch5 (F5, on XY) is empty.
#      Hovering the x axis with the circle tool makes nothing; a circle
#      whose centre is clicked on the x axis makes the x axis and holds
#      the centre on it with a Coincident in the circle's step (2 DOF);
#      one undo takes all of it back (0 DOF).
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sketch-axes-test.sh [screenshot.png]

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-sketch-axes.XXXXXX)
FILE=$WORK/axes.mitcad
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT

cat > "$WORK/model.json" << 'EOF'
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_point", "sketch": "F1", "at": [20, 12]},
  {"cmd": "sketch.add_line", "sketch": "F1", "start": [40, 5], "end": [70, 20]},
  {"cmd": "sketch.add_point", "sketch": "F1", "at": [-8, 15]},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.circle", "sketch": "F2", "mode": "center", "center": [-30, 20], "radius": 4},
  {"cmd": "sketch.circle", "sketch": "F2", "mode": "center", "center": [-27, -16], "radius": 5},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_point", "sketch": "F3", "at": [-8, 5]},
  {"cmd": "sketch.circle", "sketch": "F3", "mode": "center", "center": [10, 12], "radius": 3},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_point", "sketch": "F4", "at": [-6, 8]},
  {"cmd": "sketch.create"}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# double_click_at x y: a double click at a screen position.
double_click_at() { xdotool mousemove "$1" "$2" click --repeat 2 --delay 90 1; }

# ctrl_sketch_click x y: a click at a sketch point (mm) with Ctrl held,
# adding to the selection.
ctrl_sketch_click() {
  xdotool keydown ctrl
  ui_sketch_click "$1" "$2"
  xdotool keyup ctrl
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

ui_start_display
ui_start_app --open "$FILE"
ui_expect_log "Timeline Sketch1 at" "Sketch1 in the timeline"
read -r tx ty <<< "$(ui_logged_at "Timeline Sketch1")"
ui_mark
ui_step "double-click Sketch1"           double_click_at "$tx" "$ty"
ui_expect_new "Editing sketch F1 (Sketch1)" "Sketch1 edited"
ui_expect_new "Sketch Sketch1: 8 DOF" "two free points and a free line"
ui_step "fit (F6)"                       ui_key F6
ui_sync
ui_step "show the origin and the axes"   zoom_to_show -15 -10 75 45

echo "--- 1. A point 30 mm from the x axis"
ui_mark
ui_step "sketch dimension (D)"           ui_key d
ui_expect_new "Sketch tool Sketch Dimension" "Sketch Dimension active"
ui_step "pick p1"                        ui_sketch_click 20 12
ui_step "pick the x axis"                ui_sketch_click 8 0
ui_step "place the value"                ui_sketch_click 26 6
ui_expect_new "Sketch value editor at" "the value editor opened"
ui_expect_new " with 12.00" "p1's distance from the x axis offered"
ui_step "type 30"                        xdotool type --delay 40 "30"
ui_step "Enter"                          ui_key Return
ui_expect_new "Made the sketch x axis for a pick: c7" "the x axis made for the pick"
ui_expect_new "Added dimension k" "a driving dimension"
ui_expect_new "point_line_distance" "the point's distance from the axis"
ui_expect_new "Sketch Sketch1: 7 DOF" "p1 held at its distance"
grep -q "Made the sketch origin" "$UI_LOG" && ui_fail "the origin was made on its own"

echo "--- 2. A line at 25 degrees to the x axis"
ui_mark
ui_step "pick the line"                  ui_sketch_click 55 12.5
ui_step "pick the x axis"                ui_sketch_click 8 0
ui_step "place the value between them"   ui_sketch_click 45 3
ui_expect_new "Sketch value editor at" "the value editor opened"
ui_expect_new " with 26.57" "the angle between them offered"
ui_step "type 25"                        xdotool type --delay 40 "25"
ui_step "Enter"                          ui_key Return
ui_expect_new "Added dimension k" "a driving dimension"
ui_expect_new ": 25.00 deg, the arc from 25.0 to 0.0 deg" "the line turned to 25 degrees"
ui_expect_new "Sketch Sketch1: 6 DOF" "the line's direction held"
grep -c "Made the sketch x axis" "$UI_LOG" | grep -qx 1 || ui_fail "the x axis was made twice"
ui_step "end the tool (Esc)"             ui_key Escape

echo "--- 3. A point on the y axis"
ui_mark
ui_step "coincident (search)"            ui_command "Coincident"
ui_expect_new "Sketch tool Coincident" "Coincident active"
ui_step "pick p5"                        ui_sketch_click -8 15
ui_step "pick the y axis"                ui_sketch_click 0 25
ui_expect_new "Made the sketch y axis for a pick: c9" "the y axis made for the pick"
ui_expect_new "Added constraint coincident k" "the point on the axis"
ui_expect_new "on p5, c9" "p5 and the y axis's line"
ui_expect_new "Sketch Sketch1: 5 DOF" "p5 on the y axis"
[ -n "$SHOT" ] && ui_capture "$SHOT"
ui_step "end the tool (Esc)"             ui_key Escape
ui_mark
ui_step "undo (Ctrl+Z)"                  ui_key ctrl+z
ui_expect_new "Sketch Sketch1: 6 DOF" "the constraint and the y axis taken back as one step"
ui_mark
ui_step "redo (Ctrl+Y)"                  ui_key ctrl+y
ui_expect_new "Sketch Sketch1: 5 DOF" "both again"
ui_mark
ui_step "finish sketch (Ctrl+Enter)"     ui_key ctrl+Return
ui_expect_new "Sketch finished" "Sketch1 finished"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"

echo "--- 4. The saved design"
ui_mark
ui_step "save (Ctrl+S)"                  ui_key ctrl+s
ui_expect_new "Saved $FILE" "saved"
# The entities in the order they were made: p1, the line's points and
# the line, p5, then the origin point, the x axis's far end and line, the
# y axis's far end and line.
cat > "$WORK/check.json" << 'EOF2'
[
  {"expect": {"sketch": "Sketch1", "dof": 5}},
  {"expect": {"query": {"query": "sketch", "uid": "F1"}, "result": {
    "entities": [
      {"id": "p1", "at": [20, 30]},
      {"id": "p3"}, {"id": "p4"}, {"id": "c2"},
      {"id": "p5", "at": [0, 15]},
      {"id": "p6", "at": [0, 0], "fixed": true},
      {"id": "p8", "axis": "x", "fixed": true},
      {"id": "c7", "type": "line", "start": "p6", "end": "p8", "axis": "x", "construction": true, "fixed": true},
      {"id": "p10", "axis": "y", "fixed": true},
      {"id": "c9", "type": "line", "start": "p6", "end": "p10", "axis": "y", "construction": true, "fixed": true}],
    "constraints": [{"type": "coincident", "point": "p5", "entity": "c9"}],
    "dimensions": [{"type": "point_line_distance", "point": "p1", "line": "c7", "measured": 30},
                   {"type": "angle", "a": "c2", "b": "c7", "measured": 0.4363323129985824}],
    "regions": []}}}
]
EOF2
"$CLI" run "$WORK/check.json" --open "$FILE" > "$WORK/check.log" 2>&1 ||
  { cat "$WORK/check.log"; ui_fail "the saved axes, dimensions and constraint"; }
echo "ok   the axes, dimensions and constraint saved"

echo "--- 5. Symmetry with two circles selected, then the x axis (mitcad#865)"
ui_mark
ui_step "double-click Sketch2"           ui_double_click_logged "Timeline Sketch2"
ui_expect_new "Editing sketch F2 (Sketch2)" "Sketch2 edited"
ui_expect_new "Sketch Sketch2: 6 DOF" "two free circles"
ui_step "show both circles"              zoom_to_show -40 -25 75 45
ui_mark
ui_step "select the upper circle"        ui_sketch_click -30 24
ui_step "and the lower one (Ctrl)"       ctrl_sketch_click -27 -11
ui_expect_new "Selected: 2 sketch curves" "both circles selected"
ui_mark
ui_step "symmetry (search)"              ui_command "Symmetry"
ui_expect_new "Sketch tool Symmetry" "Symmetry active"
ui_expect_new "Symmetry takes the selection as its picks: c" "the two circles are its first picks"
ui_step "pick the x axis"                ui_sketch_click 8 0
ui_expect_new "Made the sketch x axis for a pick: c" "Sketch2's x axis made for the pick"
ui_expect_new "Added constraint symmetric k" "the circles mirrored about the x axis by one click"
ui_expect_new "Sketch Sketch2: 3 DOF" "the lower circle follows the upper one"
grep -q "Symmetry pick .* refused" "$UI_LOG" && ui_fail "the axis pick was refused"
ui_step "end the tool (Esc)"             ui_key Escape
ui_mark
ui_step "finish sketch (Ctrl+Enter)"     ui_key ctrl+Return
ui_expect_new "Sketch finished" "Sketch2 finished"

echo "--- 6. Mirror a circle about the y axis (mitcad#876)"
ui_mark
ui_step "double-click Sketch3"           ui_double_click_logged "Timeline Sketch3"
ui_expect_new "Editing sketch F3 (Sketch3)" "Sketch3 edited"
ui_expect_new "Sketch Sketch3: 5 DOF" "a free point and a free circle"
ui_step "show the circle and the y axis" zoom_to_show -15 -5 20 20
ui_mark
ui_step "select the circle"              ui_sketch_click 10 15
ui_expect_new "Selected: 1 sketch curve" "the circle selected"
ui_mark
ui_step "mirror (search)"                ui_command "Mirror"
ui_step "pick the y axis"                ui_sketch_click 0 18
ui_expect_new "Sketch axis picked for the command: y_axis (y axis)" "the y axis picked as the mirror line"
ui_expect_new "Mirror Mirror Line: 1 sketch curve [sketch curve y_axis of F3]" "Mirror Line holds the y axis"
ui_expect_new "Preview Mirror: ok" "the mirror previewed"
ui_mark
ui_step "OK (Enter)"                     ui_key Return
ui_expect_new "Mirrored 1 object(s) about the y axis" "the circle mirrored about the y axis"
ui_mark
ui_step "finish sketch (Ctrl+Enter)"     ui_key ctrl+Return
ui_expect_new "Sketch finished" "Sketch3 finished"
ui_mark
ui_step "save (Ctrl+S)"                  ui_key ctrl+s
ui_expect_new "Saved $FILE" "saved"
# The point, the circle's centre and the circle, then what the mirror
# made in its step: the origin point, the y axis's far end and line, the
# copy's centre and the copy.
cat > "$WORK/check-mirror.json" << 'EOF3'
[
  {"expect": {"sketch": "Sketch3", "dof": 5}},
  {"expect": {"query": {"query": "sketch", "uid": "F3"}, "result": {
    "entities": [
      {"id": "p1", "at": [-8, 5]},
      {"id": "p3", "at": [10, 12]},
      {"id": "c2", "type": "circle", "center": "p3", "radius": 3},
      {"id": "p4", "at": [0, 0], "fixed": true},
      {"id": "p6", "axis": "y", "fixed": true},
      {"id": "c5", "type": "line", "start": "p4", "end": "p6", "axis": "y", "construction": true, "fixed": true},
      {"id": "p8", "at": [-10, 12]},
      {"id": "c7", "type": "circle", "center": "p8", "radius": 3}],
    "constraints": [{"type": "symmetric", "a": "c2", "b": "c7", "axis": "c5"}]}}}
]
EOF3
"$CLI" run "$WORK/check-mirror.json" --open "$FILE" > "$WORK/check-mirror.log" 2>&1 ||
  { cat "$WORK/check-mirror.log"; ui_fail "the saved mirror about the y axis"; }
echo "ok   the copy saved symmetric about the y axis"

echo "--- 7. The axes selected before a tool: Sketch Dimension, then Coincident (mitcad#899)"
ui_mark
ui_step "double-click Sketch4"           ui_double_click_logged "Timeline Sketch4"
ui_expect_new "Editing sketch F4 (Sketch4)" "Sketch4 edited"
ui_expect_new "Sketch Sketch4: 2 DOF" "a free point"
ui_step "show the point and the axes"    zoom_to_show -15 -5 20 20
ui_mark
ui_step "select p1"                      ui_sketch_click -6 8
ui_expect_new "Selected: 1 sketch point" "p1 selected"
ui_mark
ui_step "and the x axis (Ctrl)"          ctrl_sketch_click 12 0
ui_expect_new "Sketch axis selected: x_axis (x axis)" "the x axis selected with no tool running"
ui_expect_new "Selected: 1 sketch curve, 1 sketch point [sketch point p1 of F4; sketch curve x_axis of F4]" \
  "p1 and the x axis in the selection"
ui_mark
ui_step "sketch dimension (D)"           ui_key d
ui_expect_new "Sketch tool Sketch Dimension" "Sketch Dimension takes the selection"
ui_step "place the value"                ui_sketch_click -12 3
ui_expect_new "Sketch value editor at" "the value editor opened"
ui_expect_new " with 8.00" "p1's distance from the x axis offered"
ui_step "type 15"                        xdotool type --delay 40 "15"
ui_step "Enter"                          ui_key Return
ui_expect_new "Made the sketch x axis for a pick: c" "the x axis made in the dimension's step"
ui_expect_new "point_line_distance" "p1's distance from the x axis"
ui_expect_new "Sketch Sketch4: 1 DOF" "p1 held 15 mm from the x axis"
ui_step "end the tool (Esc)"             ui_key Escape
ui_mark
ui_step "select p1"                      ui_sketch_click -6 15
ui_expect_new "Selected: 1 sketch point" "p1 selected again"
ui_mark
ui_step "and the y axis (Ctrl)"          ctrl_sketch_click 0 5
ui_expect_new "Sketch axis selected: y_axis (y axis)" "the y axis selected with no tool running"
ui_expect_new "Selected: 1 sketch curve, 1 sketch point [sketch point p1 of F4; sketch curve y_axis of F4]" \
  "p1 and the y axis in the selection"
tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -q "Made the sketch" && ui_fail "selecting the y axis made it"
ui_mark
ui_step "coincident (search)"            ui_command "Coincident"
ui_expect_new "Made the sketch y axis for a pick: c" "the y axis made in the constraint's step"
ui_expect_new "Added constraint coincident k" "p1 on the y axis"
ui_expect_new "on p1, c" "p1 and the y axis's line"
ui_expect_new "Sketch Sketch4: 0 DOF" "p1 held on the y axis"
ui_mark
ui_step "undo (Ctrl+Z)"                  ui_key ctrl+z
ui_expect_new "Sketch Sketch4: 1 DOF" "the constraint and the y axis taken back as one step"
ui_mark
ui_step "redo (Ctrl+Y)"                  ui_key ctrl+y
ui_expect_new "Sketch Sketch4: 0 DOF" "both again"
ui_mark
ui_step "finish sketch (Ctrl+Enter)"     ui_key ctrl+Return
ui_expect_new "Sketch finished" "Sketch4 finished"

echo "--- 8. A circle's centre snapped onto the x axis (mitcad#948)"
ui_mark
ui_step "double-click Sketch5"           ui_double_click_logged "Timeline Sketch5"
ui_expect_new "Editing sketch F5 (Sketch5)" "Sketch5 edited"
ui_expect_new "Sketch Sketch5: 0 DOF" "an empty sketch"
ui_step "show the axes"                  zoom_to_show -15 -5 20 20
ui_mark
ui_step "circle tool (C)"                ui_key c
ui_expect_new "Sketch tool Center Diameter Circle" "the circle tool"
ui_step "hover the x axis"               xdotool mousemove $(ui_sketch_at 4 0)
ui_sync
tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -q "Made the sketch" && ui_fail "hovering the x axis made it"
echo "ok   hovering the x axis makes nothing"
ui_mark
ui_step "centre on the x axis"           ui_sketch_click 10 0
ui_step "rim"                            ui_sketch_click 10 6
ui_expect_new "Added circle, diameter" "the circle added"
ui_expect_new "Made the sketch x axis for a pick: c" "the x axis made in the circle's step"
ui_expect_new " on the sketch x axis c" "the centre held on the x axis by a Coincident"
ui_expect_new "Sketch Sketch5: 2 DOF" "only the circle's x and radius free"
tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -q "Constraint coincident on .* not added" &&
  ui_fail "the Coincident with the axis was refused"
ui_step "end the tool (Esc)"             ui_key Escape
ui_mark
ui_step "undo (Ctrl+Z)"                  ui_key ctrl+z
ui_expect_new "Sketch Sketch5: 0 DOF" "the circle, the x axis and the Coincident taken back as one step"
ui_mark
ui_step "finish sketch (Ctrl+Enter)"     ui_key ctrl+Return
ui_expect_new "Sketch finished" "Sketch5 finished"

ui_finish "UI sketch axes test"
