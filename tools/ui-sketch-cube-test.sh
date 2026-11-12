#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/OcctViewer.cpp app/OcctViewer.hpp app/view app/sketch tools/cli
# The view cube in a sketch with a constraint selected (mitcad#773: a click
# on a cube arrow or Home ended the application with an exception out of
# the button's slot), on a design mitcad-cli makes: Sketch1 (F1) on XY with
# a line c2 from the origin to (30, 20) whose horizontal constraint k1 is
# inactive, as an import leaves one the file does not meet, and a line from
# (40, 0) to (40, 25) with an active vertical constraint k2. The Windows
# test is tools/ui-windows-sketch-cube-test.ps1.
#   1. The sketch opens looking at XY, face on: the cube's arrows show.
#      With the active constraint k2 selected (a click on its symbol), each
#      arrow (up, down, clockwise, counter-clockwise, right, left) turns the
#      view to the face beside or rolls it, and Home turns it to the home
#      view.
#   2. The same with the inactive constraint k1 selected, after a click on
#      the cube's top face turned the view back to the sketch.
#   3. A new sketch on XY drawn here: a horizontal line, whose inferred
#      horizontal constraint is selected; every arrow and Home again.
# After each click the application lives on and logged no failed view
# operation ("View operation failed: ...", which the view logs instead of
# letting an exception end the application).
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sketch-cube-test.sh [screenshot.png]

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-sketch-cube.XXXXXX)
FILE=$WORK/cube.mitcad
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT
# Where a click hits the cube's faces (the top face, to turn back to the
# sketch), logged as the camera rests.
export MITCAD_LOG_ORIENTATION_CUBE=1

# An active constraint has no "active" in the answer.
cat > "$WORK/model.json" << 'EOF'
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_point", "sketch": "F1", "at": [0, 0], "fixed": true},
  {"cmd": "sketch.add_line", "sketch": "F1", "start": "p1", "end": [30, 20]},
  {"cmd": "sketch.add_constraint", "sketch": "F1", "constraint": {"type": "horizontal", "line": "c2"}},
  {"cmd": "sketch.set_active", "sketch": "F1", "items": ["k1"], "active": false,
   "reason": "not met in the imported file"},
  {"cmd": "sketch.drag", "sketch": "F1", "entity": "p3", "to": [30, 20]},
  {"cmd": "sketch.add_line", "sketch": "F1", "start": [40, 0], "end": [40, 25]},
  {"cmd": "sketch.add_constraint", "sketch": "F1", "constraint": {"type": "vertical", "line": "c4"}},
  {"expect": {"query": {"query": "sketch", "uid": "F1"}, "result": {
    "constraints": [{"id": "k1", "type": "horizontal", "active": false},
                    {"id": "k2", "type": "vertical", "line": "c4"}],
    "unmet_inactive": ["k1"]}}}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# expect_camera "direction ..." "description": the camera at rest after
# the action since the mark.
expect_camera() { ui_expect_new "Camera direction $1" "$2" 8; }

# still_alive "description": nothing on the view's path failed since the
# mark, and the application lives on.
still_alive() {
  local failed
  failed=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -m1 -E "View operation failed|OCCT view event failed")
  [ -n "$failed" ] && ui_fail "$1: $failed"
  ui_crashed && ui_fail "$1: the application ended"
}

# turn "arrow" "expected camera" "description": a click on the cube's arrow.
turn() {
  ui_mark
  ui_step "the cube's arrow $1"             ui_click_logged "Orientation cube arrow $1"
  expect_camera "$2" "$3"
  still_alive "the arrow $1"
}

# arrows_and_home "constraint": every arrow, then Home, with it selected;
# the sketch stays edited.
arrows_and_home() {
  local from
  from=$(wc -l < "$UI_LOG")
  ui_expect_log "Orientation cube arrow up at" "the cube's arrows show face on"
  turn up "0 -1 0 up 0 0 -1" "up: turned to the face above"
  turn down "0 0 -1 up 0 1 0" "down: back to the sketch"
  turn cw "0 0 -1 up -1 0 0" "clockwise: rolled 90 degrees"
  turn ccw "0 0 -1 up 0 1 0" "counter-clockwise: rolled back"
  turn right "-1 0 0 up 0 1 0" "right: turned to the face on the right"
  turn left "0 0 -1 up 0 1 0" "left: back to the sketch"
  ui_mark
  ui_step "Home"                            ui_click_logged "View home"
  expect_camera "-0.568 0.654 -0.5 up" "Home: the home view (the default view, mitcad#1698)"
  still_alive "Home"
  tail -n +$((from + 1)) "$UI_LOG" | grep -q "Sketch finished" && ui_fail "the sketch was finished by a view turn"
  echo "ok   $1 selected: every arrow and Home turned the view, the sketch still edited"
}

# select_constraint "k": a click on its symbol selects it.
select_constraint() {
  ui_expect_log "Constraint $1 glyph at" "$1's symbol is placed"
  ui_mark
  ui_step "select $1 (its symbol)"          ui_click_logged "Constraint $1 glyph"
  ui_expect_new "Selected: 1 constraint" "$1 selected"
}

ui_start_display
ui_start_app --open "$FILE"
ui_expect_log "Timeline Sketch1 at" "Sketch1 in the timeline"
ui_mark
ui_step "edit Sketch1 (double-click)"      ui_double_click_logged "Timeline Sketch1"
ui_expect_new "Editing sketch F1 (Sketch1)" "Sketch1 edited"
ui_expect_new "Sketch inactive drawn: k1 constraint" "k1 is drawn inactive"
expect_camera "0 0 -1 up 0 1 0" "looking at the sketch"
ui_step "fit (F6)"                         ui_key F6

echo "--- 1. The active constraint k2 selected"
select_constraint k2
[ -n "$SHOT" ] && ui_capture "$SHOT"
arrows_and_home k2

echo "--- 2. The inactive constraint k1 selected"
ui_expect_log "Orientation cube top at" "the cube's top face is placed"
ui_mark
ui_step "the cube's top face"              ui_click_logged "Orientation cube top"
expect_camera "0 0 -1 up 0 1 0" "looking at the sketch again"
still_alive "the cube's top face"
ui_step "fit (F6)"                         ui_key F6
select_constraint k1
arrows_and_home k1

echo "--- 3. A sketch drawn here: a horizontal line, its constraint selected"
ui_mark
ui_step "finish sketch (Ctrl+Enter)"       ui_key ctrl+Return
ui_expect_new "Sketch finished" "Sketch1 finished"
ui_mark
ui_step "create sketch on XY"              ui_create_sketch xy
expect_camera "0 0 -1 up 0 1 0" "looking at the new sketch"
ui_mark
ui_step "line tool (L)"                    ui_key l
ui_expect_new "Sketch tool Line" "line tool active"
ui_step "line: start (-10, -5)"            ui_sketch_click -10 -5
ui_step "line: end (10, -5)"               ui_sketch_click 10 -5
ui_expect_new ") (horizontal)" "the line drawn, its horizontal constraint inferred"
ui_step "end the tool (Esc Esc)"           ui_key Escape Escape
# Its constraint: the symbol placed since the line tool started (the new
# sketch has only it; Sketch1's symbols were placed before).
DRAWN=""
for _ in $(seq 1 30); do
  DRAWN=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -oE "^Constraint k[0-9]+ glyph at " | tail -1 | cut -d' ' -f2)
  [ -n "$DRAWN" ] && break
  sleep 0.2
done
[ -n "$DRAWN" ] || ui_fail "the drawn line's constraint symbol was not placed"
select_constraint "$DRAWN"
arrows_and_home "$DRAWN"

ui_finish "UI sketch view cube test"
