#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch tools/cli
# check-all sources: core/sketch core/model-api/src/sketch.rs core/solver
# Inactive sketch constraints (mitcad#721), on a design mitcad-cli makes:
# Sketch1 (F1) on XY with a fixed point p1 at the origin and a line c2 from
# it to p3 at (30, 20); a horizontal constraint k1 on c2, made inactive as
# an import leaves one the file does not meet ("not met in the imported
# file"), and a vertical distance k2 of 20 mm from p1 to p3.
#   1. The sketch opens with k1 drawn inactive (pink); its tooltip says
#      why: "Not applied: not met in the imported file".
#   2. Its context menu's Activate is refused: the sketch does not solve
#      with it and the vertical distance; k1 stays inactive.
#   3. With the vertical distance deleted (its value's context menu),
#      Activate applies k1: nothing is drawn inactive, the line is
#      horizontal. Undo makes it inactive again, its symbol placed anew.
#   4. Delete (its context menu) removes it; the saved design has no
#      constraint and no dimension.
#   5. Sketch2 (F2): two horizontal lines and a parallel between them that
#      their horizontals imply. Edited, the Sketch Palette shows the
#      redundant constraint as a warning (mitcad#1051).
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sketch-inactive-test.sh [screenshot.png]

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-sketch-inactive.XXXXXX)
FILE=$WORK/inactive.mitcad
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT

# The horizontal constraint made first, then set aside: the line's end is
# dragged back up and dimensioned there.
cat > "$WORK/model.json" << 'EOF'
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_point", "sketch": "F1", "at": [0, 0], "fixed": true},
  {"cmd": "sketch.add_line", "sketch": "F1", "start": "p1", "end": [30, 20]},
  {"cmd": "sketch.add_constraint", "sketch": "F1", "constraint": {"type": "horizontal", "line": "c2"}},
  {"cmd": "sketch.set_active", "sketch": "F1", "items": ["k1"], "active": false,
   "reason": "not met in the imported file"},
  {"cmd": "sketch.drag", "sketch": "F1", "entity": "p3", "to": [30, 20]},
  {"cmd": "sketch.add_dimension", "sketch": "F1", "value": 20,
   "dimension": {"type": "vertical_distance", "a": "p1", "b": "p3"}},
  {"expect": {"query": {"query": "sketch", "uid": "F1"}, "result": {
    "constraints": [{"id": "k1", "type": "horizontal", "active": false}],
    "dimensions": [{"id": "k2", "type": "vertical_distance", "measured": 20}],
    "unmet_inactive": ["k1"]}}},
  {"cmd": "add_feature", "def": {"type": "sketch", "plane": "xy",
    "entities": [
      {"id": "p1", "type": "point", "at": [0, -30]},
      {"id": "p2", "type": "point", "at": [30, -30]},
      {"id": "c3", "type": "line", "start": "p1", "end": "p2"},
      {"id": "p4", "type": "point", "at": [0, -40]},
      {"id": "p5", "type": "point", "at": [30, -40]},
      {"id": "c6", "type": "line", "start": "p4", "end": "p5"}],
    "constraints": [
      {"id": "k7", "type": "horizontal", "line": "c3"},
      {"id": "k8", "type": "horizontal", "line": "c6"},
      {"id": "k9", "type": "parallel", "a": "c3", "b": "c6"}]}}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# double_click_at x y: a double click at a screen position.
double_click_at() { xdotool mousemove "$1" "$2" click --repeat 2 --delay 90 1; }

# right_click_choose "logged name" "entry": a right click on what the log
# placed, then the context menu's entry.
right_click_choose() {
  ui_mark
  ui_step "right-click $1"               ui_click_logged "$1" 3
  ui_expect_new "Context menu: " "the context menu opened"
  ui_step "$2"                           ui_menu_choose "$2"
}

ui_start_display
ui_start_app --open "$FILE"
ui_expect_log "Timeline Sketch1 at" "Sketch1 in the timeline"

read -r tx ty <<< "$(ui_logged_at "Timeline Sketch1")"
ui_mark
ui_step "double-click Sketch1"           double_click_at "$tx" "$ty"
ui_expect_new "Editing sketch F1 (Sketch1)" "Sketch1 edited"
ui_step "fit (F6)"                       ui_key F6
ui_sync

echo "--- Drawn inactive, with why in its tooltip"
ui_expect_log "Sketch inactive drawn: k1 constraint" "k1 is drawn inactive"
ui_expect_log "Constraint k1 glyph at" "its symbol is placed"
[ -n "$SHOT" ] && ui_capture "$SHOT"
ui_mark
ui_step "point at its symbol"            xdotool mousemove $(ui_logged_at "Constraint k1 glyph")
ui_expect_new "Constraint tooltip k1: Not applied: not met in the imported file" "the tooltip says why"

echo "--- Activate refused while the vertical distance holds the line up"
right_click_choose "Constraint k1 glyph" "Activate"
ui_expect_new "Sketch items not activated: k1 cannot be activated" "refused with the reason"
grep -q "Sketch items activated: k1" "$UI_LOG" && ui_fail "k1 was activated"
echo "ok   k1 stays inactive"

echo "--- Without the vertical distance, Activate applies it"
right_click_choose "Dimension k2 text" "Delete"
ui_expect_new "Deleted k2" "the vertical distance deleted"
right_click_choose "Constraint k1 glyph" "Activate"
ui_expect_new "Sketch items activated: k1" "k1 activated"
ui_expect_new "Sketch inactive drawn: none" "nothing is drawn inactive"
ui_mark
ui_step "undo (Ctrl+Z)"                  ui_key ctrl+z
ui_expect_new "Undo: Activate in Sketch1" "the activation is one undo step"
ui_expect_new "Sketch inactive drawn: k1 constraint" "k1 inactive again"
ui_expect_new "Constraint k1 glyph at" "its symbol placed again after the sketch changed (mitcad#723)"

echo "--- Delete removes it"
right_click_choose "Constraint k1 glyph" "Delete"
ui_expect_new "Deleted k1" "k1 deleted"
ui_expect_new "Sketch inactive drawn: none" "nothing is drawn inactive"
ui_mark
ui_step "finish sketch (Ctrl+Enter)"     ui_key ctrl+Return
ui_expect_new "Sketch finished" "Sketch1 finished"

echo "--- The saved design"
ui_mark
ui_step "save (Ctrl+S)"                  ui_key ctrl+s
ui_expect_new "Saved $FILE" "saved"
cat > "$WORK/check.json" << 'EOF2'
[
  {"expect": {"query": {"query": "sketch", "uid": "F1"}, "result": {
    "constraints": [], "dimensions": []}}}
]
EOF2
"$CLI" run "$WORK/check.json" --open "$FILE" > "$WORK/check.log" 2>&1 ||
  { cat "$WORK/check.log"; ui_fail "the saved sketch"; }
echo "ok   no constraint and no dimension saved"

echo "--- Redundant constraints: a warning (mitcad#1051)"
read -r tx ty <<< "$(ui_logged_at "Timeline Sketch2")"
ui_mark
ui_step "double-click Sketch2"           double_click_at "$tx" "$ty"
ui_expect_new "Editing sketch F2 (Sketch2)" "Sketch2 edited"
ui_expect_new "Sketch Palette solve: warning: Redundant constraints (they follow from the others): k" \
  "the Sketch Palette shows the redundant constraint as a warning"
ui_mark
ui_step "finish sketch (Ctrl+Enter)"     ui_key ctrl+Return
ui_expect_new "Sketch finished" "Sketch2 finished"

ui_finish "UI sketch inactive constraint test"
