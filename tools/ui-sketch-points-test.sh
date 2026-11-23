#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/commands/SketchCommands.cpp app/MainWindow.cpp tools/cli
# check-all sources: core/sketch core/model-api/src/sketch.rs
# Points that become one and projected geometry made ordinary, through the
# real UI, on a design mitcad-cli makes: Box1 (F1.b0, 30 x 20 x 10 from 20
# above XY, its front at y = -60) and Sketch1 (F2) on XY with two collinear
# lines with a gap (c1 from (0, 0) to (10, 0), c4 from (20, 0) to (30, 0)), two lines
# whose ends are apart (c7 from (0, 30) to (20, 30), c10 from (24, 34) to
# (24, 50)) and the box's front top edge projected, linked (c13 from p14 at
# (0, -60) to p15 at (30, -60)).
#   1. Extend c1 (mitcad#259): its end joins c4's start p5, two degrees of
#      freedom fewer.
#   2. Coincident on c7's end and c10's start (mitcad#260): one point; Offset
#      of the two lines works (one chain); undo takes both back.
#   3. Convert to Sketch Geometry (mitcad#257) from the context menu of the
#      projected line: before it the line does not drag, after it it does;
#      undo makes it projected again.
#   4. On a second design (mitcad#287): a rectangle standing on a projected
#      line, its bottom Horizontal and its left corner on the line's left
#      end; Coincident on its right corner and the line's right end is
#      added, the Horizontal it implies removed, and the hint says so; one
#      undo takes both back.
#   5. The Point tool on a derived offset curve: the point is added and the
#      coincident of its snap holds it on the offset (mitcad#2278), one
#      degree of freedom along it, nothing refused.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sketch-points-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-sketch-points.XXXXXX)
FILE=$WORK/points.mitcad
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT

cat > "$WORK/model.json" << 'EOF'
[
  {"cmd": "add_feature", "def": {"type": "box", "corner": [0, -60],
    "plane": {"origin": [0, 0, 20], "normal": [0, 0, 1], "x_axis": [1, 0, 0]},
    "length": 30, "width": 20, "height": 10, "operation": "new_body"}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_line", "sketch": "F2", "start": [0, 0], "end": [10, 0]},
  {"cmd": "sketch.add_line", "sketch": "F2", "start": [20, 0], "end": [30, 0]},
  {"cmd": "sketch.add_line", "sketch": "F2", "start": [0, 30], "end": [20, 30]},
  {"cmd": "sketch.add_line", "sketch": "F2", "start": [24, 34], "end": [24, 50]},
  {"cmd": "sketch.project", "sketch": "F2", "source": "E{F1:side0|F1:top}", "body": "F1.b0",
    "linked": true}
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

# dof: the degrees of freedom of Sketch1 the app logged last.
dof() { sed -n 's/^Sketch Sketch1: \([0-9]*\) DOF.*/\1/p' "$UI_LOG" | tail -1; }

# drag_line stays|moves description: after a drag of c13 since the mark,
# whether it stayed at y = -60 (fixed geometry: no step, or no motion) or
# followed the pointer.
drag_line() {
  local line
  ui_sync
  line=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -E "Drag of c13 did not move it|Dragged c13 to " | tail -1)
  [ -n "$line" ] || ui_fail "$2: no drag of c13 in the log"
  if [[ $line == *"did not move"* || $line == *", -60.000)" ]]; then
    [ "$1" = stays ] || ui_fail "$2: $line"
  else
    [ "$1" = moves ] || ui_fail "$2: $line"
  fi
  echo "ok   $2: $line"
}

expect_dof() {
  local now
  ui_sync
  now=$(dof)
  [ "$now" = "$1" ] || ui_fail "$2: $now DOF, expected $1"
  echo "ok   $2 ($1 DOF)"
}

ui_start_display
ui_start_app --open "$FILE"
ui_step "fit (F6)"                       ui_key F6
ui_expect_log "Timeline Sketch1 at" "the timeline is logged"
ui_step "edit Sketch1 (double-click)"    ui_double_click_logged "Timeline Sketch1"
ui_expect_log "Editing sketch F2 (Sketch1)" "Sketch1 is edited"
ui_step "zoom out"                       zoom_to_show -15 -75 45 60
expect_dof 16 "four free lines and a projected one"

echo "--- 1. Extend joins the end it reaches (mitcad#259)"
ui_mark
ui_step "extend (search)"                ui_command "Extend"
ui_expect_new "Sketch tool Extend" "Extend active"
ui_mark
ui_step "click c1 near its end"          ui_sketch_click 8 0
ui_expect_new "Extended c1 at" "c1 extended"
ui_expect_new "(joined p5)" "its end is c4's start"
expect_dof 14 "one point fewer"
ui_step "end the tool (Esc)"             ui_key Escape

echo "--- 2. Coincident makes two ends one point (mitcad#260)"
ui_mark
ui_step "coincident (search)"            ui_command "Coincident"
ui_expect_new "Sketch tool Coincident" "Coincident active"
ui_mark
ui_step "pick c7's end"                  ui_sketch_click 20 30
ui_step "pick c10's start"               ui_sketch_click 24 34
ui_expect_new "Merged points p11 into p9 (coincident on p9, p11)" "the ends merged"
expect_dof 12 "one point fewer again"
ui_step "end the tool (Esc)"             ui_key Escape
# The corner is where the solve put it, between the two old ends.
ui_mark
ui_step "select c7"                      ui_sketch_click 6 30.6
ui_step "and c10 (Ctrl)"                 xdotool keydown ctrl
ui_sketch_click 23.4 45
xdotool keyup ctrl
ui_expect_new "Selected: 2 sketch curves" "both lines selected"
ui_mark
ui_step "offset (O)"                     ui_key o
ui_expect_new "Offset Curves: 2 sketch curves" "the lines went to the offset"
ui_expect_new "Preview Offset: ok" "one chain: the offset previews"
ui_step "OK (Enter)"                     ui_key Return
ui_expect_new "Offset 2 curve(s) by" "offset"
ui_mark
ui_step "undo the offset (Ctrl+Z)"       ui_key ctrl+z
ui_expect_new "Undo: " "the offset undone"
ui_mark
ui_step "undo the merge (Ctrl+Z)"        ui_key ctrl+z
ui_expect_new "Undo: " "the merge undone"
expect_dof 14 "two points again"
ui_step "clear the selection (Esc)"      ui_key Escape

echo "--- 3. Convert to Sketch Geometry (mitcad#257)"
# Seen from the front, right and above (Home), the projected line on XY is
# apart from the box 20 above it, whose edges a right-click would take
# first (mitcad#217: the context menu is for what Shift+click takes).
ui_mark
ui_step "home (isometric)"               ui_command "Home"
ui_expect_new "Camera direction" "seen from the front, right and above"
ui_sync
ui_mark
ui_step "drag the projected line"        ui_sketch_drag 15 -60 15 -50
drag_line stays "projected: it does not drag"
ui_mark
ui_step "right-click the projected line" ui_sketch_click 15 -60 3
ui_expect_new "Context menu: " "context menu opened"
grep "Context menu: " "$UI_LOG" | tail -1 | grep -qE "(:|\|) Convert to Sketch Geometry( \||$)" ||
  ui_fail "no Convert to Sketch Geometry in the context menu of a projected line"
echo "ok   Convert to Sketch Geometry offered for projected geometry"
ui_mark
ui_step "choose it"                      ui_menu_choose "Convert to Sketch Geometry"
ui_expect_new "Converted to sketch geometry: c13, p14, p15" "the line and its ends converted"
ui_step "clear the selection (Esc)"      ui_key Escape
ui_mark
ui_step "drag the line"                  ui_sketch_drag 15 -60 15 -50
drag_line moves "free: it drags"
ui_mark
ui_step "undo the drag (Ctrl+Z)"         ui_key ctrl+z
ui_expect_new "Undo: Drag in Sketch1" "the drag undone"
ui_mark
ui_step "undo the convert (Ctrl+Z)"      ui_key ctrl+z
ui_expect_new "Undo: Convert to Sketch Geometry in Sketch1" "one undo step"
ui_mark
ui_step "drag the line again"            ui_sketch_drag 15 -60 15 -50
drag_line stays "projected again"

ui_step "finish the sketch (Ctrl+Enter)" ui_key ctrl+Return
ui_expect_log "Sketch finished" "sketch finished"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"

echo "--- 4. A partly implied coincident replaces the Horizontal (mitcad#287)"
# A second design: Sketch1 (F1) on XY with a projected line (c3, fixed
# reference geometry, from p1 at (0, 0) to p2 at (60, 0)); a rectangle on
# it (c8 its bottom from p4 at (0, 0) to p5 at (40, 0), Horizontal k12),
# its left corner coincident with p1 (k16); and far above it a closed
# outline of three lines and a spline offset out by 2 (derived: its lower
# line at y = 58 is computed from c19 after every solve).
cat > "$WORK/implied.json" << 'JSON'
[
  {"cmd": "add_feature", "def": {"type": "sketch", "plane": "xy", "entities": [
    {"id": "p1", "type": "point", "at": [0, 0], "fixed": true, "reference": true},
    {"id": "p2", "type": "point", "at": [60, 0], "fixed": true, "reference": true},
    {"id": "c3", "type": "line", "start": "p1", "end": "p2", "fixed": true, "reference": true},
    {"id": "p4", "type": "point", "at": [0, 0]},
    {"id": "p5", "type": "point", "at": [40, 0]},
    {"id": "p6", "type": "point", "at": [40, 25]},
    {"id": "p7", "type": "point", "at": [0, 25]},
    {"id": "c8", "type": "line", "start": "p4", "end": "p5"},
    {"id": "c9", "type": "line", "start": "p5", "end": "p6"},
    {"id": "c10", "type": "line", "start": "p6", "end": "p7"},
    {"id": "c11", "type": "line", "start": "p7", "end": "p4"},
    {"id": "p17", "type": "point", "at": [0, 60]},
    {"id": "p18", "type": "point", "at": [40, 60]},
    {"id": "c19", "type": "line", "start": "p17", "end": "p18"},
    {"id": "p20", "type": "point", "at": [40, 80]},
    {"id": "c21", "type": "line", "start": "p18", "end": "p20"},
    {"id": "p22", "type": "point", "at": [20, 88]},
    {"id": "p23", "type": "point", "at": [0, 80]},
    {"id": "c24", "type": "fitted_spline", "points": ["p20", "p22", "p23"]},
    {"id": "c25", "type": "line", "start": "p23", "end": "p17"}],
    "constraints": [
      {"id": "k12", "type": "horizontal", "line": "c8"},
      {"id": "k13", "type": "vertical", "line": "c9"},
      {"id": "k14", "type": "horizontal", "line": "c10"},
      {"id": "k15", "type": "vertical", "line": "c11"},
      {"id": "k16", "type": "coincident", "point": "p4", "entity": "p1"}]}},
  {"cmd": "sketch.offset", "sketch": "F1", "curves": ["c19", "c21", "c24", "c25"], "distance": 2}
]
JSON
"$CLI" run "$WORK/implied.json" --save "$WORK/implied.mitcad" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }
ui_stop_app
ui_start_app --open "$WORK/implied.mitcad"
UI_MARK=0
ui_step "fit (F6)"                       ui_key F6
ui_expect_log "Timeline Sketch1 at" "the timeline is logged"
ui_step "edit Sketch1 (double-click)"    ui_double_click_logged "Timeline Sketch1"
ui_expect_log "Editing sketch F1 (Sketch1)" "Sketch1 is edited"
ui_step "zoom out"                       zoom_to_show -10 -10 70 95
ui_sync
before=$(dof)
ui_mark
ui_step "coincident (search)"            ui_command "Coincident"
ui_expect_new "Sketch tool Coincident" "Coincident active"
ui_mark
ui_step "pick the right bottom corner"   ui_sketch_click 40 0
ui_step "pick the projected line's end"  ui_sketch_click 60 0
ui_expect_new "Removed constraint horizontal k12: implied by the new coincident k" \
  "the Horizontal the coincident implies is removed"
ui_expect_new " on p5, p2" "the coincident is added"
ui_expect_new "Sketch notice: Horizontal removed: implied by the new Coincident." "the hint says so"
expect_dof $((before - 1)) "the corner on the line's end"
ui_step "end the tool (Esc)"             ui_key Escape
ui_mark
ui_step "undo (Ctrl+Z)"                  ui_key ctrl+z
ui_expect_new "Undo: " "undone"
expect_dof "$before" "one undo takes both back"

echo "--- 5. A point snapped onto a derived offset is held on it (mitcad#2278)"
ui_mark
ui_step "point (search)"                 ui_command "Point"
ui_expect_new "Sketch tool Point" "Point active"
ui_sync
before=$(dof)
ui_mark
ui_step "click on the derived offset"    ui_sketch_click 10 58
ui_expect_new "Added point " "the point is added"
expect_dof $((before + 1)) "the snap's coincident holds it on the offset, free along it"
tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -q " not added: " &&
  ui_fail "the snap's coincident on the derived offset was refused"
echo "ok   the snap's coincident on the derived offset is added"
ui_step "end the tool (Esc)"             ui_key Escape
ui_step "finish the sketch (Ctrl+Enter)" ui_key ctrl+Return
ui_expect_log "Sketch finished" "sketch finished"
ui_finish "UI sketch points test"
