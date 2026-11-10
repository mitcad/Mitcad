#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/SketchCommands.cpp
# check-all sources: core/sketch/src/modify.rs core/sketch/src/project.rs
# Offset of a projected outline whose edge was projected twice (mitcad#741),
# through the real UI, on a design mitcad-cli makes: Sketch1 (F1) on XY as
# an earlier version left a face projected after one of its edges, all
# fixed reference geometry: the edge's line c3 from p1 (40, 0) to p2
# (40, 30) with points of its own, and the face's rectangle c8 (bottom), c9
# (right, on c3), c10 (top), c11 (left) sharing p4 (0, 0), p5 (40, 0),
# p6 (40, 30), p7 (0, 30); far above, two fixed lines whose ends lie at one
# place but are two points: c12 from p13 (0, 60) to p14 (20, 60) and c15
# from p16 (20, 60) to p17 (20, 80).
#   1. Offset with Chain Selection, one click on the right edge (c3 or c9,
#      whichever the click takes): the whole outline is offset (no degrees
#      of freedom left; one line would leave two).
#   2. Offset of c12 and c15: the panel says why there is no chain (the
#      ends are two points; a Coincident constraint joins them), and OK
#      stays off.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-projected-offset-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-projected-offset.XXXXXX)
FILE=$WORK/outline.mitcad
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT

cat > "$WORK/model.json" << 'EOF'
[
  {"cmd": "add_feature", "def": {"type": "sketch", "plane": "xy", "entities": [
    {"id": "p1", "type": "point", "at": [40, 0], "fixed": true, "reference": true},
    {"id": "p2", "type": "point", "at": [40, 30], "fixed": true, "reference": true},
    {"id": "c3", "type": "line", "start": "p1", "end": "p2", "fixed": true, "reference": true},
    {"id": "p4", "type": "point", "at": [0, 0], "fixed": true, "reference": true},
    {"id": "p5", "type": "point", "at": [40, 0], "fixed": true, "reference": true},
    {"id": "p6", "type": "point", "at": [40, 30], "fixed": true, "reference": true},
    {"id": "p7", "type": "point", "at": [0, 30], "fixed": true, "reference": true},
    {"id": "c8", "type": "line", "start": "p4", "end": "p5", "fixed": true, "reference": true},
    {"id": "c9", "type": "line", "start": "p5", "end": "p6", "fixed": true, "reference": true},
    {"id": "c10", "type": "line", "start": "p6", "end": "p7", "fixed": true, "reference": true},
    {"id": "c11", "type": "line", "start": "p7", "end": "p4", "fixed": true, "reference": true},
    {"id": "p13", "type": "point", "at": [0, 60], "fixed": true},
    {"id": "p14", "type": "point", "at": [20, 60], "fixed": true},
    {"id": "c12", "type": "line", "start": "p13", "end": "p14", "fixed": true},
    {"id": "p16", "type": "point", "at": [20, 60], "fixed": true},
    {"id": "p17", "type": "point", "at": [20, 80], "fixed": true},
    {"id": "c15", "type": "line", "start": "p16", "end": "p17", "fixed": true}]}}
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
ui_expect_log "Editing sketch F1 (Sketch1)" "Sketch1 is edited"
ui_step "zoom out"                       zoom_to_show -15 -15 55 90
expect_dof 0 "all of it fixed"

echo "--- 1. Chain Selection from the edge projected twice (mitcad#741)"
ui_mark
ui_step "click the right edge"           ui_sketch_click 40 15
ui_expect_new "Selected: 1 sketch curve" "one curve selected"
echo "info $(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep "Selected: " | tail -1)"
ui_mark
ui_step "offset (O)"                     ui_key o
ui_expect_new "Preview Offset: ok" "the offset previews"
ui_step "OK (Enter)"                     ui_key Return
ui_expect_new "Offset 1 curve(s) by" "offset"
expect_dof 0 "the whole outline is offset, held by its source"
ui_mark
ui_step "undo the offset (Ctrl+Z)"       ui_key ctrl+z
ui_expect_new "Undo: " "the offset undone"
ui_step "clear the selection (Esc)"      ui_key Escape

echo "--- 2. Ends at one place that are two points: the panel says why"
ui_mark
ui_step "select c12"                     ui_sketch_click 10 60
ui_step "and c15 (Ctrl)"                 xdotool keydown ctrl
ui_sketch_click 20 70
xdotool keyup ctrl
ui_expect_new "Selected: 2 sketch curves" "both lines selected"
ui_mark
ui_step "offset (O)"                     ui_key o
ui_expect_new "Preview Offset: failed: " "no chain: the preview fails"
ui_expect_new "a curve ends at another curve's end but on a point of its own" "the reason says where the chain breaks"
ui_expect_new "a Coincident constraint joins them" "and what joins them"
ui_mark
ui_step "cancel (Esc)"                   ui_key Escape
ui_expect_new "Command Offset cancelled" "the panel closes"
expect_dof 0 "nothing added"

ui_step "finish the sketch (Ctrl+Enter)" ui_key ctrl+Return
ui_expect_log "Sketch finished" "sketch finished"
ui_finish "UI projected offset test"
