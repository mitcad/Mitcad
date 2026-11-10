#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework app/MainWindow.cpp app/commands/SketchCommands.cpp app/sketch tools/cli
# check-all sources: app/tests/palette_create_tools.cpp
# check-all sources: app/OcctViewer.cpp app/OcctViewer.hpp
# check-all sources: core/sketch core/model-api/src/sketch.rs
# What the sketch tools make and keep (mitcad#352, #353, #354, #355, #356,
# #1024), through the real UI, on a design mitcad-cli makes: Sketch1 (F1)
# empty, Sketch2 (F2) on XY with a line c1 from (0, 40) to (40, 40), the
# points p4 at (-20, -20) and p5 at (20, -20), a line from (-50, 40) to
# (-20, 40), and a line from (50, -30) to (70, -30); and Sketch3 (F3) with a
# circle of radius 20 about the origin and the points (-30, 50), (-40, -40),
# (40, -40) and (-40, 30). The design is saved after the drawing and its
# file read with jq.
#   1. The Point tool on the origin of Sketch1 makes one fixed point and no
#      constraint; a second click adds nothing (mitcad#355).
#   2. In Sketch2: a 3-Point Arc whose start snapped to the midpoint of c1
#      has a midpoint constraint; a Center to Center Slot with both ends
#      snapped to existing points has coincident constraints to them; an
#      Ellipse with its centre snapped to the midpoint of the line at
#      (-50, 40) has a midpoint constraint (mitcad#353).
#   3. An Ellipse with a typed major radius of 10 and a rim click 20 mm off
#      its axis dimensions the radius it was typed for: the ellipse's minor
#      one (mitcad#356).
#   4. The Dimension tool: "w = 40" typed into the value editor adds one
#      dimension with the parameter w, and the editor closes (mitcad#352).
#   5. Esc in the Dimension tool's editor sends the tool back to picking; in
#      the editor of Edit Dimension (context menu) it only closes the editor
#      (mitcad#354).
#   6. A Line's clicks where its value fields are (the focused length field
#      placed for the start, then the angle field beside the corner) end the
#      lines there; no field covers the pointer (mitcad#671).
#   7. Constraints the drawn lines already satisfy, added only when safe
#      (mitcad#1024), in Sketch3: a line from (-30, 50) to the circle's top
#      quadrant is held there (coincident with the circle) and plumb with
#      its centre (vertical_points); a line between the level points
#      (-40, -40) and (40, -40) is horizontal; a line from (-40, 30)
#      clicked near where it touches the circle, (-17.4, -9.9), ends there
#      on the circle and is tangent to it; a rectangle drawn afterwards adds
#      no over-constraint warning or refused constraint.
#   8. The Sketch Palette's Create section (mitcad#1027): the palette opened
#      for Sketch1 before any tool started shows the defaults; the
#      Constraints take two rows of seven, Create two rows, and no button
#      is clipped; the Ellipse, started twice in Sketch2, is in the section
#      when Sketch3 opens; in a new sketch (Sketch4) the Center Rectangle,
#      started three times, does not move the buttons during the sketch and
#      is there when Sketch4 opens again; its Create button starts the tool
#      and shows checked.
#   9. A right-click while a sketch tool runs is its Esc (mitcad#1085): in
#      a new sketch (Sketch5) two Line segments, a right-button drag in the
#      view that does not end the chain, a right-click that ends the chain
#      and keeps the tool, a second one that ends the tool, both without a
#      menu; with no tool the right-click opens the context menu. Saved,
#      Sketch5 has the two lines.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sketch-tools-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-sketch-tools.XXXXXX)
FILE=$WORK/tools.mitcad
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT

cat > "$WORK/model.json" << 'EOF'
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_line", "sketch": "F2", "start": [0, 40], "end": [40, 40]},
  {"cmd": "sketch.add_point", "sketch": "F2", "at": [-20, -20]},
  {"cmd": "sketch.add_point", "sketch": "F2", "at": [20, -20]},
  {"cmd": "sketch.add_line", "sketch": "F2", "start": [-50, 40], "end": [-20, 40]},
  {"cmd": "sketch.add_line", "sketch": "F2", "start": [50, -30], "end": [70, -30]},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.circle", "sketch": "F3", "mode": "center", "center": [0, 0], "radius": 20},
  {"cmd": "sketch.add_point", "sketch": "F3", "at": [-30, 50]},
  {"cmd": "sketch.add_point", "sketch": "F3", "at": [-40, -40]},
  {"cmd": "sketch.add_point", "sketch": "F3", "at": [40, -40]},
  {"cmd": "sketch.add_point", "sketch": "F3", "at": [-40, 30]}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

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

# expect_file filter expected description: a jq filter on the saved file.
expect_file() {
  local actual
  actual=$(jq -c "$1" "$FILE") || ui_fail "$3: cannot read $FILE"
  [ "$actual" = "$2" ] || ui_fail "$3: $1 is '$actual', expected '$2'"
  echo "ok   $3"
}

# sketch_of sx sy: the sketch point (mm) at a screen place, from the last
# logged "Sketch view" (ui_sketch_at backwards).
sketch_of() {
  local view
  view=$(grep -o "Sketch view [-0-9.,]* [-0-9.,]* [-0-9.,]*" "$UI_LOG" | tail -1)
  [ -n "$view" ] || ui_fail "the app did not log the sketch view"
  awk -v view="$view" -v sx="$1" -v sy="$2" -v wx="$X" -v wy="$Y" -v s="$UI_SCALE" 'BEGIN {
    split(view, part, " "); split(part[3], o, ","); split(part[4], a, ","); split(part[5], b, ",")
    px = (sx - wx) / s - o[1]; py = (sy - wy) / s - o[2]
    ax = (a[1] - o[1]) / 100; ay = (a[2] - o[2]) / 100; bx = (b[1] - o[1]) / 100; by = (b[2] - o[2]) / 100
    d = ax * by - bx * ay
    printf "%.3f %.3f\n", (px * by - bx * py) / d, (ax * py - ay * px) / d }'
}

# last_line: the start and end (mm) of the line added last since the mark.
last_line() {
  local last
  last=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -o "Added line c[0-9]* from ([^)]*) to ([^)]*)" | tail -1)
  [ -n "$last" ] || ui_fail "no line added since the mark"
  sed 's/.* from (\([^,]*\), \([^)]*\)) to (\([^,]*\), \([^)]*\))/\1 \2 \3 \4/' <<< "$last"
}

# expect_line_end x y description: the line added last ends near (x, y):
# within 6 mm (an inference may move it a little), or on the grid point the
# snap to the grid rounds (x, y) to, up to half a grid step off in x and in
# y (the step of the layout grid the app logged last; 0.5 mm more for the
# pixel the click lands on). A 10 mm grid moves a point up to 7.1 mm.
expect_line_end() {
  local sx sy ex ey step
  read -r sx sy ex ey <<< "$(last_line)"
  step=$(grep -o "Layout grid [0-9.]* mm" "$UI_LOG" | tail -1 | awk '{ print $3 }')
  awk -v ex="$ex" -v ey="$ey" -v x="$1" -v y="$2" -v g="${step:-0}" 'BEGIN {
    dx = ex - x; dy = ey - y; if (dx < 0) dx = -dx; if (dy < 0) dy = -dy
    exit !(dx * dx + dy * dy <= 36 || (g > 0 && dx <= g / 2 + 0.5 && dy <= g / 2 + 0.5)) }' ||
    ui_fail "$3: the line ends at ($ex, $ey), expected near ($1, $2) (grid ${step:-unknown} mm)"
  echo "ok   $3"
}

# expect_new_re regex description: a line logged since the mark matches the
# extended regular expression (after a ui_expect_new that waited for it).
expect_new_re() {
  tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qE -- "$1" || ui_fail "$2: no line since the mark matches '$1'"
  echo "ok   $2"
}

# no_browser: no field opened its parameter browser since the mark.
no_browser() {
  ui_sync
  if tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -q "Parameter browser opened"; then
    ui_fail "a click meant for the view opened a field's parameter browser"
  fi
  echo "ok   no parameter browser opened"
}

# fields_clear_of sx sy description: none of the value fields logged since
# the mark ("Sketch field <id> at x,y", their centres; 104 x 26 px at most)
# covers the screen place (sx, sy).
fields_clear_of() {
  local covering
  covering=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -o "Sketch field [a-z_]* at [0-9]*,[0-9]*" |
    awk -v sx="$1" -v sy="$2" -v wx="$X" -v wy="$Y" -v s="$UI_SCALE" '{
      split($5, c, ","); dx = wx + c[1] * s - sx; dy = wy + c[2] * s - sy
      if (dx < 0) dx = -dx; if (dy < 0) dy = -dy
      if (dx < 52 * s && dy < 13 * s) print $3 }')
  [ -z "$covering" ] || ui_fail "$3: the field $covering covers the pointer"
  echo "ok   $3"
}

ui_start_display
ui_start_app --open "$FILE"
ui_step "fit (F6)"                       ui_key F6
ui_expect_log "Timeline Sketch2 at" "the timeline is logged"

echo "--- 1. The Point tool on the origin makes one fixed point (mitcad#355)"
ui_mark
ui_step "edit Sketch1 (double-click)"    ui_double_click_logged "Timeline Sketch1"
ui_expect_new "Editing sketch F1 (Sketch1)" "Sketch1 is edited"
ui_step "zoom out"                       zoom_to_show -20 -20 20 20
ui_step "point tool (search)"            ui_command "Point"
ui_expect_new "Sketch tool Point" "the Point tool is active"
ui_mark
ui_step "click the origin"               ui_sketch_click 0 0
ui_expect_new "Added point p1 at" "one point added at the origin"
ui_step "click the origin again"         ui_sketch_click 0 0
ui_sync
if tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -c "Added point" | grep -qv '^1$'; then
  ui_fail "the second click on the origin added another point"
fi
echo "ok   the second click added nothing"
ui_step "end the tool (Esc)"             ui_key Escape
ui_step "finish sketch (Ctrl+Enter)"     ui_key ctrl+Return
ui_expect_new "Sketch finished" "Sketch1 finished"

echo "--- 2. Snapped arc, slot and ellipse keep their snaps (mitcad#353)"
ui_mark
ui_step "edit Sketch2 (double-click)"    ui_double_click_logged "Timeline Sketch2"
ui_expect_new "Editing sketch F2 (Sketch2)" "Sketch2 is edited"
ui_step "zoom out"                       zoom_to_show -70 -45 100 75
ui_mark
ui_step "3-point arc (search)"           ui_command "3-Point Arc"
ui_expect_new "Sketch tool 3-Point Arc" "the arc tool is active"
ui_step "start at the midpoint of c1"    ui_sketch_click 20 40
ui_step "end"                            ui_sketch_click 20 70
ui_step "through"                        ui_sketch_click 35 55
ui_expect_new "Added arc c" "the arc is added"
ui_step "end the tool (Esc)"             ui_key Escape
ui_mark
ui_step "slot (search)"                  ui_command "Center to Center Slot"
ui_expect_new "Sketch tool Center to Center Slot" "the slot tool is active"
ui_step "first end on p4"                ui_sketch_click -20 -20
ui_step "second end on p5"               ui_sketch_click 20 -20
ui_step "width"                          ui_sketch_click 0 -28
ui_expect_new "Added slot, width 20 mm" "the slot is added"
ui_step "end the tool (Esc)"             ui_key Escape
ui_mark
ui_step "drag the end of c1 (the arc starts at its middle)" ui_sketch_drag 40 40 50 45
ui_sync
ui_mark
ui_step "ellipse (search)"               ui_command "Ellipse"
ui_expect_new "Sketch tool Ellipse" "the ellipse tool is active"
ui_step "centre on the midpoint"         ui_sketch_click -35 40
ui_step "end of the major axis"          ui_sketch_click -10 40
ui_step "a point on the ellipse"         ui_sketch_click -35 50
ui_expect_new "Added ellipse c" "the ellipse is added"
ui_step "end the tool (Esc)"             ui_key Escape

echo "--- 3. A typed major radius goes to the right axis (mitcad#356)"
ui_mark
ui_step "ellipse (search)"               ui_command "Ellipse"
ui_step "centre"                         ui_sketch_click 60 10
ui_step "towards the major end"          xdotool mousemove $(ui_sketch_at 70 10)
ui_sync
ui_step "type 10"                        xdotool type --delay 40 "10"
ui_step "Enter"                          ui_key Return
ui_step "a point on the ellipse, 20 off the axis" ui_sketch_click 50 30
ui_expect_new "Added ellipse c" "the ellipse is added"
ui_expect_new "radii 20 and 10 mm" "its major radius is the larger one"
ui_step "end the tool (Esc)"             ui_key Escape

echo "--- 4. name = value in the Dimension tool (mitcad#352)"
ui_mark
ui_step "sketch dimension (D)"           ui_key d
ui_step "pick the line"                  ui_sketch_click 60 -30
ui_step "place the value"                ui_sketch_click 60 -40
ui_expect_new "Sketch value editor at" "the value editor opened"
ui_step "type w = 40"                    xdotool type --delay 40 "w = 40"
ui_step "Enter"                          ui_key Return
ui_expect_new "Added dimension k" "the dimension is added"
ui_expect_new "Sketch value editor closed" "the editor closed"
ui_sync
if tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -q "Sketch value editor kept open"; then
  ui_fail "the editor came back after the name was taken"
fi
echo "ok   the editor did not come back"
ui_mark
ui_step "pick the line c6"               ui_sketch_click -45 40
ui_step "place the value"                ui_sketch_click -35 56
ui_expect_new "Sketch value editor at" "the second editor opened"
ui_step "type h = 25"                    xdotool type --delay 40 "h = 25"
ui_step "Enter"                          ui_key Return
ui_expect_new "Added dimension k" "the second dimension is added"
ui_step "end the tool (Esc)"             ui_key Escape

echo "--- 5. Esc in a value editor (mitcad#354)"
# The Dimension tool's own editor: Esc closes it and sends the tool back to
# picking, so that the next picks start a new dimension.
ui_mark
ui_step "sketch dimension (D)"           ui_key d
ui_step "pick the line"                  ui_sketch_click 60 -30
ui_step "place the value"                ui_sketch_click 60 -40
ui_expect_new "Sketch value editor at" "the editor opened"
ui_mark
ui_step "Esc"                            ui_key Escape
ui_expect_new "Sketch value editor closed" "the editor closed"
ui_mark
ui_step "pick the line again"            ui_sketch_click 60 -30
ui_step "place the value again"          ui_sketch_click 60 -40
ui_expect_new "Sketch value editor at" "the tool went back to picking: a new dimension starts"
ui_step "Esc"                            ui_key Escape
ui_step "end the tool (Esc)"             ui_key Escape
# An editor of Edit Dimension (the context menu of the dimension's text):
# Esc closes it alone and the sketch stays in edit mode.
ui_mark
ui_step "right-click the dimension"      ui_sketch_click 60 -40 3
ui_expect_new "Context menu: " "the context menu opened"
ui_step "Edit Dimension"                 ui_menu_choose "Edit Dimension"
ui_expect_new "Sketch value editor at" "the editor opened"
ui_mark
ui_step "type h = 30 (h is another parameter)" xdotool type --delay 40 "h = 30"
ui_step "Enter"                          ui_key Return
ui_expect_new "Sketch value editor kept open" "the editor stays open when the model refuses the name"
ui_mark
ui_step "Esc"                            ui_key Escape
ui_expect_new "Sketch value editor closed" "the editor closed"
ui_step "line tool (L)"                  ui_key l
ui_expect_new "Sketch tool Line" "a tool starts as usual"
ui_step "end the tool (Esc)"             ui_key Escape

echo "--- 6. A drawing tool's value fields never take its next click (mitcad#671)"
# Each click of a drawing tool goes where the pointer is. The fields keep
# off the pointer; one placed for the pointer's last place, under the
# pointer when it jumps there and clicks at once, passes the click on to
# the view while nothing is typed into it.
ui_mark
ui_step "line tool (L)"                  ui_key l
ui_expect_new "Sketch tool Line" "the line tool is active"
ui_step "start a line"                   ui_sketch_click -68 15
ui_expect_new "Sketch field length at" "the length field is placed"
ui_sync
read -r FX FY <<< "$(ui_logged_at "Sketch field length")"
read -r MX MY <<< "$(sketch_of "$FX" "$FY")"
ui_mark
ui_step "click where the length field is" xdotool mousemove "$FX" "$FY" click 1
ui_expect_new "Added line c" "the click ended the line"
expect_line_end "$MX" "$MY" "the line ends where the length field was"
no_browser
# The chain goes on with an angle field (not focused) beside the corner;
# the pointer 24 mm on, turned 20 degrees from the first line, where the
# angle's value would lie on it.
read -r SX SY EX EY <<< "$(last_line)"
read -r PX PY <<< "$(awk -v sx="$SX" -v sy="$SY" -v ex="$EX" -v ey="$EY" 'BEGIN {
  a = atan2(ey - sy, ex - sx) + 20 * 3.14159265358979 / 180
  printf "%.3f %.3f\n", ex + 24 * cos(a), ey + 24 * sin(a) }')"
ui_sync
ui_mark
ui_step "point on from the corner"       xdotool mousemove $(ui_sketch_at "$PX" "$PY")
ui_expect_new "Sketch field angle at" "the angle field is placed"
ui_sync
read -r AX AY <<< "$(ui_sketch_at "$PX" "$PY")"
fields_clear_of "$AX" "$AY" "no field lies under the pointer"
read -r FX FY <<< "$(ui_logged_at "Sketch field angle")"
read -r MX MY <<< "$(sketch_of "$FX" "$FY")"
ui_mark
ui_step "click where the angle field is" xdotool mousemove "$FX" "$FY" click 1
ui_expect_new "Added line c" "the click ended the second line"
expect_line_end "$MX" "$MY" "the second line ends where the angle field was"
no_browser
ui_step "end the chain (Esc)"            ui_key Escape
ui_step "end the tool (Esc)"             ui_key Escape

ui_step "finish sketch (Ctrl+Enter)"     ui_key ctrl+Return
ui_expect_new "Sketch finished" "Sketch2 finished"

echo "--- 7. Constraints the drawn lines satisfy, added when safe (mitcad#1024)"
ui_mark
ui_step "edit Sketch3 (double-click)"    ui_double_click_logged "Timeline Sketch3"
ui_expect_new "Editing sketch F3 (Sketch3)" "Sketch3 is edited"
ui_step "zoom out"                       zoom_to_show -50 -50 60 60
ui_step "line tool (L)"                  ui_key l
ui_expect_new "Sketch tool Line" "the line tool is active"
ui_mark
ui_step "from the point (-30, 50)"       ui_sketch_click -30 50
ui_step "to the circle's top quadrant"   ui_sketch_click 0 20.3
ui_expect_new "from (-30, 50) to (0, 20)" "the line ends at the top quadrant"
ui_expect_new "(quadrant)" "the quadrant adds its constraints"
expect_new_re "Constraint added automatically: coincident p[0-9]+ on c[0-9]+ \(quadrant\)" \
  "the quadrant holds the end on the circle"
expect_new_re "Constraint added automatically: vertical_points p[0-9]+ p[0-9]+ \(quadrant\)" \
  "and plumb with its centre"
ui_step "end the chain (Esc)"            ui_key Escape
ui_mark
ui_step "from the point (-40, -40)"      ui_sketch_click -40 -40
ui_step "to the level point (40, -40)"   ui_sketch_click 40 -40
ui_expect_new "from (-40, -40) to (40, -40) (horizontal)" "the line between level points"
expect_new_re "Constraint added automatically: horizontal c[0-9]+" "is horizontal"
ui_step "end the chain (Esc)"            ui_key Escape
ui_mark
ui_step "from the point (-40, 30)"       ui_sketch_click -40 30
ui_step "near where it touches the circle" ui_sketch_click -17.4 -9.9
ui_expect_new "from (-40, 30) to (-17.39818" "the end goes to the touching point"
expect_new_re "from \(-40, 30\) to \(-17\.39818[0-9]*, -9\.86424[0-9]*\) \(tangent\)" \
  "where the line touches the circle (-17.39818, -9.86424)"
ui_expect_new "Constraint added automatically: tangent c" "the line is tangent to the circle"
ui_step "end the chain (Esc)"            ui_key Escape
ui_step "end the tool (Esc)"             ui_key Escape
ui_mark
ui_step "rectangle tool (R)"             ui_key r
ui_step "first corner"                   ui_sketch_click 25 -10
ui_step "opposite corner"                ui_sketch_click 45 -30
ui_expect_new "Added rectangle" "a rectangle afterwards"
ui_sync
if tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -E "not added|over-constrain|can't hold"; then
  ui_fail "the rectangle after the automatic constraints met a refused constraint or an over-constrained sketch"
fi
echo "ok   no refused constraint, no over-constrained sketch"
ui_step "end the tool (Esc)"             ui_key Escape
ui_step "finish sketch (Ctrl+Enter)"     ui_key ctrl+Return
ui_expect_new "Sketch finished" "Sketch3 finished"

echo "--- 8. The Sketch Palette's Create section, the constraints on two rows (mitcad#1027)"
# create_line n: the n-th "Palette create tools" line of the log (one each
# time the palette opened for a sketch).
create_line() { grep "^Palette create tools: " "$UI_LOG" | sed -n "$1p"; }
# create_has line tool: the line names the tool (ids without "sketch.").
create_has() { grep -qE "(: |, )$2(,| \()" <<< "$1"; }
DEFAULTS="line, point, rectangle, circle, arc, polygon, slot, spline, mirror, dimension, fillet, trim, extend, offset"
# Sketch1 opened (step 1) before any tool started: the defaults, in the
# ribbon's order (Create, then Modify).
[ "$(create_line 1)" = "Palette create tools: $DEFAULTS (defaults)" ] ||
  ui_fail "the first palette's Create section is '$(create_line 1)', expected the defaults ($DEFAULTS)"
echo "ok   the palette opened before any tool started shows the defaults"
# The Ellipse (not a default) started twice in Sketch2 (steps 2 and 3): in
# the section when Sketch3 opened (step 7).
SKETCH3=$(create_line 3)
create_has "$SKETCH3" ellipse && [[ $SKETCH3 == *" (counts)" ]] ||
  ui_fail "Sketch3's Create section '$SKETCH3' lacks the Ellipse started twice in Sketch2"
echo "ok   the Ellipse started in Sketch2 is in the section when Sketch3 opens"
ui_mark
ui_step "create sketch on XY"            ui_create_sketch xy
ui_expect_new "Palette create tools: " "the palette chose its Create tools as it opened"
ui_expect_new "Palette Constraints: 13 buttons in 2 rows of 7" "the constraints on two rows of seven"
ui_expect_new "Palette Create: 14 buttons in 2 rows of 7" "the Create section's two rows of seven"
ui_expect_new "Palette buttons clipped: none" "no button is clipped: the palette keeps its width"
SKETCH4=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep "^Palette create tools: " | tail -1)
create_has "$SKETCH4" rectangle_center && ui_fail "the Center Rectangle is in the section before it started"
ui_step "zoom out"                       zoom_to_show -50 -65 60 60
ui_mark
for at in -30 0 30; do
  ui_step "center rectangle (search)"    ui_command "Center Rectangle"
  ui_expect_new "Sketch tool Center Rectangle" "the Center Rectangle tool is active"
  ui_step "centre ($at, -55)"            ui_sketch_click "$at" -55
  ui_step "corner"                       ui_sketch_click $((at + 4)) -51
  ui_expect_new "Added rectangle" "a centre rectangle at ($at, -55)"
  ui_step "end the tool (Esc)"           ui_key Escape
done
ui_sync
if tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -q "^Palette create tools: "; then
  ui_fail "the Create section was chosen again during the sketch"
fi
echo "ok   the Create buttons stay as they are during the sketch"
ui_step "finish sketch (Ctrl+Enter)"     ui_key ctrl+Return
ui_expect_new "Sketch finished" "Sketch4 finished"
ui_mark
ui_step "edit Sketch4 (double-click)"    ui_double_click_logged "Timeline Sketch4"
ui_expect_new "Editing sketch" "Sketch4 is edited again"
ui_expect_new "Palette create tools: " "the Create tools chosen again"
AGAIN=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep "^Palette create tools: " | tail -1)
create_has "$AGAIN" rectangle_center && [[ $AGAIN == *" (counts)" ]] ||
  ui_fail "the Center Rectangle started three times is not in '$AGAIN'"
echo "ok   the Center Rectangle started three times is in the section when the sketch opens again"
ui_expect_new "Palette paletteCreate_sketch.rectangle_center at" "its Create button is placed"
ui_mark
ui_step "the Center Rectangle's Create button" ui_click_logged "Palette paletteCreate_sketch.rectangle_center"
ui_expect_new "Sketch tool Center Rectangle" "the Create button starts its tool"
ui_expect_new "Palette create checked: rectangle_center" "its button shows checked while the tool is on"
ui_step "end the tool (Esc)"             ui_key Escape
ui_step "finish sketch (Ctrl+Enter)"     ui_key ctrl+Return
ui_expect_new "Sketch finished" "Sketch4 finished again"

echo "--- 9. A right-click ends the line chain, a second one the tool (mitcad#1085)"
# no_new_line text description: no line logged since the mark has the text.
no_new_line() {
  ui_sync
  if tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qF -- "$1"; then
    ui_fail "$2: '$1' was logged"
  fi
  echo "ok   $2"
}
ui_mark
ui_step "create sketch on XY"            ui_create_sketch xy
ui_step "zoom out"                       zoom_to_show -50 -65 60 60
ui_step "line tool (L)"                  ui_key l
ui_expect_new "Sketch tool Line" "the line tool is active"
ui_mark
ui_step "start a chain"                  ui_sketch_click -30 20
ui_step "the first segment's end"        ui_sketch_click -10 20
ui_step "the second segment's end"       ui_sketch_click -10 45
ui_expect_new "Added line c" "a line is added"
ui_sync
[ "$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -c "Added line c")" = 2 ] ||
  ui_fail "the chain did not add two lines"
echo "ok   two segments drawn"
# A right-button drag is the view's (it moves the view where the
# navigation binds the right button), not a click: the chain goes on.
ui_mark
ui_step "right-button drag in the view"  ui_view_drag 3 30 70 45 70
no_new_line "Line chain ended" "the right drag did not end the chain"
no_new_line "Sketch right-click" "the right drag is not a right-click"
ui_mark
ui_step "right-click in the view"        ui_view_click 45 60 3
ui_expect_new "Sketch right-click: Esc of the tool" "the right-click is the tool's Esc"
ui_expect_new "Line chain ended" "the right-click ended the chain"
no_new_line "Sketch tool ended" "the Line tool stays on"
no_new_line "Context menu: " "no menu opened"
ui_mark
ui_step "right-click again"              ui_view_click 60 60 3
ui_expect_new "Sketch tool ended: Line" "the second right-click left the tool"
no_new_line "Context menu: " "no menu opened"
# With no tool on the right-click menu is as before.
ui_mark
ui_step "right-click with no tool"       ui_view_click 50 80 3
ui_expect_new "Context menu: " "with no tool the context menu opens"
no_new_line "Sketch right-click" "it is not a tool's Esc"
ui_step "Escape (close the menu)"        ui_key Escape
ui_mark
ui_step "finish sketch (Ctrl+Enter)"     ui_key ctrl+Return
ui_expect_new "Sketch finished" "Sketch5 finished"

echo "--- The saved design"
ui_mark
ui_step "save (Ctrl+S)"                  ui_key ctrl+s
ui_expect_new "Saved $FILE" "the design is saved"
# Sketch1: one fixed point at the origin, no constraint.
expect_file '[.features[0].entities[] | select(.type == "point")] | length' 1 "Sketch1 has one point"
expect_file '.features[0].entities[0].fixed' true "it is fixed"
expect_file '(.features[0].constraints // []) | length' 0 "it has no constraint"
# Sketch2: the arc's start is on the midpoint of c1.
expect_file '[.features[1].entities[] | select(.type == "arc") | .start] as $s
  | [.features[1].constraints[] | select(.type == "midpoint" and .curve == "c1") | .point]
  | map(. as $p | $s | index($p) != null) | any' true "the arc start is held at the midpoint of c1"
# Dragging c1's end moved the middle of c1, and the arc start with it.
expect_file '.features[1].entities as $e | ($e[] | select(.id == "c1")) as $l
  | ($e[] | select(.id == $l.start) | .at) as $a | ($e[] | select(.id == $l.end) | .at) as $b
  | ([$e[] | select(.type == "arc")][0].start) as $s | ($e[] | select(.id == $s) | .at) as $p
  | $b[0] > 45 and ((($p[0] - ($a[0] + $b[0]) / 2) | fabs) < 1e-6)
    and ((($p[1] - ($a[1] + $b[1]) / 2) | fabs) < 1e-6)' true "the arc start followed the middle of the dragged line"
# The slot ends are held on the points p4 and p5.
expect_file '[.features[1].constraints[] | select(.type == "coincident" and (.entity == "p4" or .entity == "p5"))
  | .entity] | unique' '["p4","p5"]' "the slot ends are coincident with p4 and p5"
# The first ellipse's centre is held at the midpoint of the line.
expect_file '[.features[1].entities[] | select(.type == "ellipse") | .center] as $c
  | [.features[1].constraints[] | select(.type == "midpoint") | .point]
  | map(. as $p | $c | index($p) != null) | any' true "an ellipse centre is held at a midpoint"
# The typed 10 is the second ellipse's minor radius.
expect_file '[.features[1].dimensions[] | select(.type == "minor_radius")] | length' 1 "one minor radius dimension"
expect_file '[.features[1].dimensions[] | select(.type == "major_radius")] | length' 0 "no major radius dimension"
expect_file '[.features[1].dimensions[] | select(.type == "minor_radius") | .ellipse] as $d | [.features[1].entities[] | select(.type == "ellipse" and .minor_radius == 10 and ([.id] | inside($d)))] | length' 1 "the dimensioned ellipse has the typed 10 as its minor radius"
# w = 40 and h = 25: a dimension each, and the refused h = 30 changed nothing.
expect_file '[.parameters[] | select(.name == "w")] | length' 1 "the parameter w exists once"
expect_file '[.features[1].dimensions[] | select(.type == "length") | .value] | sort' '["h","w"]' "one dimension each named w and h, none doubled"
# Sketch3 (mitcad#1024): the circle holds the two line ends that reached
# it, the quadrant's end is plumb with its centre, the line between the
# level points is horizontal, and one line is tangent to the circle.
expect_file '.features[2] as $s | ($s.entities[] | select(.type == "circle")) as $c
  | [$s.constraints[] | select(.type == "coincident" and .entity == $c.id)] | length' 2 \
  "two line ends are held on the circle"
expect_file '.features[2] as $s | ($s.entities[] | select(.type == "circle")) as $c
  | [$s.constraints[] | select(.type == "vertical_points" and (.a == $c.center or .b == $c.center))] | length' 1 \
  "the quadrant's end is plumb with the circle's centre"
expect_file '.features[2] as $s | [$s.entities[] | select(.type == "point" and .at[1] == -40) | .id] as $p
  | [$s.entities[] | select(.type == "line" and ([.start, .end] | inside($p))) | .id] as $l
  | [$s.constraints[] | select(.type == "horizontal" and ([.line] | inside($l)))] | length' 1 \
  "the line between the level points is horizontal"
expect_file '.features[2] as $s | ($s.entities[] | select(.type == "circle")) as $c
  | [$s.constraints[] | select(.type == "tangent" and (.a == $c.id or .b == $c.id))] | length' 1 \
  "one line is tangent to the circle"
# Sketch5 (mitcad#1085): the two segments drawn before the right-clicks,
# nothing more. The fixed line the model makes for a sketch axis that a
# point snaps onto (mitcad#500, "axis" set) is not one of them; the chain
# keeps off the axes, so none is made.
expect_file '[.features[-1].entities[] | select(.type == "line" and .axis == null)] | length' 2 \
  "Sketch5 has the two lines drawn before the right-clicks"

ui_finish "UI sketch tools test"
