#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch tools/cli app/commands/SketchCommands.cpp
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
#      redundant constraint as a warning (mitcad#1051) that counts it
#      ("Redundant: 1 constraint follows from the others", mitcad#1171), a
#      link whose click selects the two lines; the log keeps its id.
#   6. Sketch3 (F3): a 60 x 40 outline with corners rounded by 10, fixed
#      reference lines and arcs as a projection makes them (mitcad#1171).
#      Edited, nothing is redundant; Offset with Chain Selection from its
#      right edge offsets it all, and still nothing is redundant and nothing
#      is free (0 DOF): no warning.
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
      {"id": "k9", "type": "parallel", "a": "c3", "b": "c6"}]}},
  {"cmd": "add_feature", "def": {"type": "sketch", "plane": "xy",
    "entities": [
      {"id": "p1", "type": "point", "at": [70, 0], "fixed": true, "reference": true},
      {"id": "p2", "type": "point", "at": [110, 0], "fixed": true, "reference": true},
      {"id": "c3", "type": "line", "start": "p1", "end": "p2", "fixed": true, "reference": true},
      {"id": "p4", "type": "point", "at": [110, 10], "fixed": true, "reference": true},
      {"id": "p5", "type": "point", "at": [120, 10], "fixed": true, "reference": true},
      {"id": "c6", "type": "arc", "center": "p4", "start": "p2", "end": "p5", "fixed": true, "reference": true},
      {"id": "p7", "type": "point", "at": [120, 30], "fixed": true, "reference": true},
      {"id": "c8", "type": "line", "start": "p5", "end": "p7", "fixed": true, "reference": true},
      {"id": "p9", "type": "point", "at": [110, 30], "fixed": true, "reference": true},
      {"id": "p10", "type": "point", "at": [110, 40], "fixed": true, "reference": true},
      {"id": "c11", "type": "arc", "center": "p9", "start": "p7", "end": "p10", "fixed": true, "reference": true},
      {"id": "p12", "type": "point", "at": [70, 40], "fixed": true, "reference": true},
      {"id": "c13", "type": "line", "start": "p10", "end": "p12", "fixed": true, "reference": true},
      {"id": "p14", "type": "point", "at": [70, 30], "fixed": true, "reference": true},
      {"id": "p15", "type": "point", "at": [60, 30], "fixed": true, "reference": true},
      {"id": "c16", "type": "arc", "center": "p14", "start": "p12", "end": "p15", "fixed": true, "reference": true},
      {"id": "p17", "type": "point", "at": [60, 10], "fixed": true, "reference": true},
      {"id": "c18", "type": "line", "start": "p15", "end": "p17", "fixed": true, "reference": true},
      {"id": "p19", "type": "point", "at": [70, 10], "fixed": true, "reference": true},
      {"id": "c20", "type": "arc", "center": "p19", "start": "p17", "end": "p1", "fixed": true, "reference": true}]}}
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

echo "--- Redundant constraints: a warning that counts them (mitcad#1051, mitcad#1171)"
read -r tx ty <<< "$(ui_logged_at "Timeline Sketch2")"
ui_mark
ui_step "double-click Sketch2"           double_click_at "$tx" "$ty"
ui_expect_new "Editing sketch F2 (Sketch2)" "Sketch2 edited"
ui_expect_new "Sketch Palette warnings: Redundant: 1 constraint follows from the others" \
  "the Sketch Palette shows the redundant constraint as a warning that counts it"
ui_expect_new "Sketch Palette redundant: k9" "the log keeps its id"
ui_expect_new "Palette warning 0 at" "the warning is a link"
ui_mark
ui_step "click the warning"              ui_click_logged "Palette warning 0"
ui_expect_new "Palette: warning 0 selects 2 entities: c3, c6" "the click selects the lines it names"
ui_step "clear the selection (Esc)"      ui_key Escape
ui_mark
ui_step "finish sketch (Ctrl+Enter)"     ui_key ctrl+Return
ui_expect_new "Sketch finished" "Sketch2 finished"

echo "--- An offset of a projected outline adds nothing redundant (mitcad#1171)"
# dof: the degrees of freedom of Sketch3 the app logged last.
dof() { sed -n 's/^Sketch Sketch3: \([0-9]*\) DOF.*/\1/p' "$UI_LOG" | tail -1; }
# not_since_mark "text" "what": fails when a line with the text was logged
# since the mark.
not_since_mark() {
  if tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -F "$1" | grep -v -F "$1none"; then
    ui_fail "$2"
  fi
  echo "ok   $2"
}
read -r tx ty <<< "$(ui_logged_at "Timeline Sketch3")"
ui_mark
ui_step "double-click Sketch3"           double_click_at "$tx" "$ty"
ui_expect_new "Editing sketch F3 (Sketch3)" "Sketch3 edited"
ui_step "fit (F6)"                       ui_key F6
ui_sync
last=$(grep "^Sketch Palette redundant: " "$UI_LOG" | tail -1)
[ "$last" = "Sketch Palette redundant: none" ] || ui_fail "the fixed outline: $last"
echo "ok   its fixed lines and arcs are not redundant"
[ "$(dof)" = 0 ] || ui_fail "the fixed outline has $(dof) DOF"
ui_mark
ui_step "click the right edge"           ui_sketch_click 120 20
ui_expect_new "Selected: 1 sketch curve" "one curve selected"
ui_mark
ui_step "offset (O)"                     ui_key o
ui_expect_new "Preview Offset: ok" "the offset previews"
ui_step "OK (Enter)"                     ui_key Return
ui_expect_new "Offset 1 curve(s) by" "the outline offset"
ui_sync
not_since_mark "Sketch Palette warnings: " "no warning after the offset"
not_since_mark "Sketch Palette redundant: " "nothing redundant after the offset"
[ "$(dof)" = 0 ] || ui_fail "the offset leaves $(dof) DOF (its helpers slide)"
echo "ok   the offset leaves 0 DOF"
ui_mark
ui_step "finish sketch (Ctrl+Enter)"     ui_key ctrl+Return
ui_expect_new "Sketch finished" "Sketch3 finished"

ui_finish "UI sketch inactive constraint test"
