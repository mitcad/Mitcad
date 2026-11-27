#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/browser tools/cli
# check-all sources: core/sketch core/model-api/src/sketch.rs core/solver
# Signed horizontal and vertical distances from the Sketch Dimension tool
# (mitcad#409), on a design mitcad-cli makes: the user parameters r = 30 mm
# and a = 30 deg; Sketch1 (F1) on XY with a fixed centre p1 at (20, 10) and
# a ray c2 from it to p3 at (50, 30), and a fixed point p4 at (-40, 10) with
# a line c5 to p6 at (-20, 20).
#   1. Signed distances off: a horizontal distance from p4 to p6 typed -15
#      is added signed (an unsigned one cannot be negative): p6 goes to
#      x = -55 and the value shows -15.00.
#   2. Signed distances checked in the palette: p1 to p3, a horizontal
#      distance r * cos(a) and a vertical one r * sin(a), both signed; the
#      values show their signs (+25.98, +15.00).
#   3. a stepped through the quadrants in Edit Parameters (120, 210, 300
#      deg): each time the sketch opens with the signed values that put p3
#      in that quadrant, no recompute fails.
#   4. At 300 deg the context menu makes the vertical distance unsigned
#      (25.98, the expression -(r * sin(a)): p3 stays) and signed again;
#      back at 30 deg the values are +25.98 and +15.00 again.
#   5. The saved design has the three signed dimensions.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sketch-signed-test.sh [screenshot.png]

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-sketch-signed.XXXXXX)
FILE=$WORK/signed.mitcad
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT

cat > "$WORK/model.json" << 'EOF'
[
  {"cmd": "add_parameter", "name": "r", "value": "30 mm"},
  {"cmd": "add_parameter", "name": "a", "value": "30 deg"},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_point", "sketch": "F1", "at": [20, 10], "fixed": true},
  {"cmd": "sketch.add_line", "sketch": "F1", "start": "p1", "end": [50, 30]},
  {"cmd": "sketch.add_point", "sketch": "F1", "at": [-40, 10], "fixed": true},
  {"cmd": "sketch.add_line", "sketch": "F1", "start": "p4", "end": [-20, 20]}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# double_click_at x y: a double click at a screen position.
double_click_at() { xdotool mousemove "$1" "$2" click --repeat 2 --delay 90 1; }

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

# edit_sketch: opens Sketch1 from the timeline (its dimensions' texts are
# logged as it opens).
edit_sketch() {
  local tx ty
  read -r tx ty <<< "$(ui_logged_at "Timeline Sketch1")"
  ui_mark
  ui_step "double-click Sketch1"         double_click_at "$tx" "$ty"
  ui_expect_new "Editing sketch F1 (Sketch1)" "Sketch1 edited"
  # Everything in sight, beside the palette.
  ui_step "fit (F6)"                     ui_key F6
  ui_sync
  ui_step "show the points"              zoom_to_show -65 -10 70 40
}

finish_sketch() {
  ui_mark
  ui_step "finish sketch (Ctrl+Enter)"   ui_key ctrl+Return
  ui_expect_new "Sketch finished" "Sketch1 finished"
}

# expect_texts h v: the texts of the two signed distances as the sketch
# opened last.
expect_texts() {
  local texts
  texts=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep "^Dimension texts: " | tail -1)
  [[ $texts == *"$H fx: $1"* ]] || ui_fail "the horizontal distance $H is not fx: $1: $texts"
  [[ $texts == *"$V fx: $2"* ]] || ui_fail "the vertical distance $V is not fx: $2: $texts"
  echo "ok   the values show $1 and $2"
}

# set_angle degrees: a = <degrees> deg in Edit Parameters.
set_angle() {
  ui_mark
  ui_step "Edit Parameters (search)"   ui_command "Edit Parameters"
  ui_expect_new "Parameters dialog opened" "the dialog opened"
  # Its cells are logged as it is first laid out.
  ui_expect_log "Parameters a expression at" "the dialog lists a"
  ui_focus_dialog '^Parameters$'
  ui_mark
  ui_double_click_logged "Parameters a expression"
  ui_sync
  ui_key ctrl+a
  xdotool type --delay 40 "$1 deg"
  ui_step "a = $1 deg"                   ui_key Return
  ui_expect_new "Parameter a expression: $1 deg" "a is $1 deg"
  ui_step "OK"                           ui_click_logged "Parameters OK"
  ui_focus_main
  grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed at $1 deg"
  echo "ok   recomputed at $1 deg"
}

ui_start_display
ui_start_app --open "$FILE"
ui_expect_log "Timeline Sketch1 at" "Sketch1 in the timeline"
edit_sketch

echo "--- Signed distances off: a negative value typed"
ui_mark
ui_step "sketch dimension (D)"           ui_key d
ui_step "pick p4"                        ui_sketch_click -40 10
ui_step "pick p6"                        ui_sketch_click -20 20
ui_step "place the value below"          ui_sketch_click -30 0
ui_expect_new "Sketch value editor at" "the value editor opened"
ui_expect_new " with 20.00" "the size offered"
ui_step "type -15"                       xdotool type --delay 40 -- "-15"
ui_step "Enter"                          ui_key Return
ui_expect_new "horizontal_distance signed d" "added signed"
ui_expect_new "shows -15.00" "the value shows its sign"
ui_step "end the tool (Esc)"             ui_key Escape

echo "--- Signed distances on: r * cos(a), r * sin(a)"
ui_mark
ui_step "check Signed distances"         ui_click_logged "Palette paletteSignedDistances"
ui_expect_new "Signed distances on" "the option is on"
ui_mark
ui_step "sketch dimension (D)"           ui_key d
ui_step "pick p1"                        ui_sketch_click 20 10
ui_step "pick p3"                        ui_sketch_click 50 30
ui_step "place the value below"          ui_sketch_click 35 0
ui_expect_new "Sketch value editor at" "the value editor opened"
ui_expect_new " with 30.00" "the offset offered"
ui_step "type r * cos(a)"                xdotool type --delay 40 "r * cos(a)"
ui_step "Enter"                          ui_key Return
ui_expect_new "horizontal_distance signed d" "a signed horizontal distance"
ui_expect_new "shows fx: +25.98" "its value with its sign"
H=$(grep -o "Added dimension k[0-9]* horizontal_distance signed" "$UI_LOG" | tail -1 | cut -d' ' -f3)
# p3 is at (45.98, 30) now: only its x is set.
ui_mark
ui_step "pick p1"                        ui_sketch_click 20 10
ui_step "pick p3"                        ui_sketch_click 45.98 30
ui_step "place the value beside"         ui_sketch_click 60 17
ui_expect_new "Sketch value editor at" "the value editor opened"
ui_expect_new " with 20.00" "the offset offered"
ui_step "type r * sin(a)"                xdotool type --delay 40 "r * sin(a)"
ui_step "Enter"                          ui_key Return
ui_expect_new "vertical_distance signed d" "a signed vertical distance"
ui_expect_new "shows fx: +15.00" "its value with its sign"
V=$(grep -o "Added dimension k[0-9]* vertical_distance signed" "$UI_LOG" | tail -1 | cut -d' ' -f3)
[ -n "$SHOT" ] && ui_capture "$SHOT"
ui_step "end the tool (Esc)"             ui_key Escape
finish_sketch

echo "--- a through the quadrants"
set_angle 120
edit_sketch
expect_texts -15.00 +25.98
finish_sketch
set_angle 210
edit_sketch
expect_texts -25.98 -15.00
finish_sketch
set_angle 300
edit_sketch
expect_texts +15.00 -25.98

echo "--- Switched unsigned and signed again: the point stays"
ui_mark
ui_step "right-click the vertical value" ui_click_logged "Dimension $V text" 3
ui_expect_new "Context menu: " "the context menu opened"
ui_step "Make Distance Unsigned"         ui_menu_choose "Make Distance Unsigned"
ui_expect_new "Dimension $V unsigned: fx: 25.98 (-(r * sin(a)))" "unsigned, its value negated: p3 stays"
ui_mark
ui_step "right-click the vertical value" ui_click_logged "Dimension $V text" 3
ui_expect_new "Context menu: " "the context menu opened"
ui_step "Make Distance Signed"           ui_menu_choose "Make Distance Signed"
ui_expect_new "Dimension $V signed: fx: -25.98 (r * sin(a))" "signed again, the expression as it was"
finish_sketch
set_angle 30
edit_sketch
expect_texts +25.98 +15.00
finish_sketch

echo "--- The saved design"
ui_mark
ui_step "save (Ctrl+S)"                  ui_key ctrl+s
ui_expect_new "Saved $FILE" "saved"
cat > "$WORK/check.json" << 'EOF2'
[
  {"expect": {"query": {"query": "sketch", "uid": "F1"}, "result": {
    "dimensions": [{"type": "horizontal_distance", "a": "p4", "b": "p6", "signed": true, "measured": -15},
                   {"type": "horizontal_distance", "a": "p1", "b": "p3", "signed": true,
                    "expression": "r * cos(a)", "measured": 25.98076211},
                   {"type": "vertical_distance", "a": "p1", "b": "p3", "signed": true,
                    "expression": "r * sin(a)", "measured": 15}]}}}
]
EOF2
"$CLI" run "$WORK/check.json" --open "$FILE" > "$WORK/check.log" 2>&1 ||
  { cat "$WORK/check.log"; ui_fail "the saved signed dimensions"; }
echo "ok   three signed dimensions saved"

ui_finish "UI sketch signed dimension test"
