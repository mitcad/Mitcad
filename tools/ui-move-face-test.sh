#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/MoveCommands.cpp app/commands/MoveFaceText.cpp app/commands/MoveFaceText.hpp app/commands/MoveDragMotion.cpp
# check-all sources: app/MainWindowMoveFace.cpp app/framework/CommandPanel.cpp core/model/src/features/move_face.rs
# check-all sources: geometry/src/move_faces.cpp tools/cli
# MODIFY > Move Face (mitcad#531) through the real UI, on a 60 x 40 x 20
# block mitcad-cli makes (Sketch1 F1, Extrude1 F2, Body1), as
# geometry/tests/test_move_faces.cpp moves its faces:
#   1. The end face at x = 60 pushed out 5 mm (Translate): previewed, the
#      panel's note naming it ("Move Face: 1 face of Body1, translate (5,
#      0, 0) mm"); placed, the block 65 long (52000 mm3).
#   2. Its timeline cell: the selection card names it (MoveFace1: 1 face of
#      Body1, translate (5, 0, 0) mm). A double-click edits it: the panel
#      opens with its face and motion, 10 mm makes the block 70 long
#      (56000 mm3).
#   3. The top tilted 5 degrees about its front edge (Rotate, the edge as
#      the axis): the top rises or falls by y tan(5 deg) towards the back,
#      as the edge's direction runs (70 x (800 +- 800 tan 5 deg) mm3).
#   4. The top pushed down 30 mm, below the bottom: the model refuses it,
#      the panel says why in red, and OK is refused; Esc.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-move-face-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

ROOT=$(cd "$(dirname "$0")/.." && pwd)
CLI=${UI_CLI:-$(dirname "$UI_APP")/../tools/cli/mitcad-cli}
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-move-face.XXXXXX")
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
FILE=$WORK/block.mitcad
R='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'
TOP="F2:end($R)"
FRONT='F2:side(c1[c4,c2])'
RIGHT='F2:side(c2[c1,c3])'
TOP_FRONT="E{$TOP|$FRONT}"

cat > "$WORK/block.json" << EOF
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 60, "height": 40},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$R"}],
    "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
  {"expect": {"body": "Body1", "volume": 48000, "tolerance": 1e-9}}
]
EOF
"$CLI" run "$WORK/block.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# Move Face's type choices: Free Move, Translate, Translate Along, Rotate,
# Point to Point, Point to Position.
TRANSLATE=1
ROTATE=3

ui_start_display
ui_start_app --open "$FILE"
ui_step "fit (F6)"                         ui_key F6

echo "--- The end face pushed out 5 mm"
ui_mark
ui_step "move face (search)"               ui_command "Move Face"
ui_expect_new "Command Move Face started" "Move Face started"
ui_expect_new "Pick places:" "it logs where its faces are"
ui_step "pick the end face at x = 60"      ui_click_pick "face F2.b0/$RIGHT"
ui_expect_log "Move Face Faces: 1 face [face $RIGHT of F2.b0]" "the end face picked"
ui_step "Translate"                        ui_choose "Panel Move Face input type" $TRANSLATE
ui_mark
ui_step "X 5"                              ui_type_in "Panel Move Face input x" "5"
ui_expect_new "Preview Move Face: ok" "previewed"
ui_expect_new "Panel Move Face note: Move Face: 1 face of Body1, translate (5, 0, 0) mm" "the note names it"
ui_mark
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Moved faces: Move Face: 1 face of Body1, translate (5, 0, 0) mm" "placed"
ui_expect_volume "Body Body1 (F2.b0): volume 48000.000 -> \([0-9.]*\) mm3" 52000 "the block 65 long"

echo "--- Its timeline cell: the card, then an edit"
ui_mark
ui_step "click MoveFace1 in the timeline"  ui_click_logged "Timeline MoveFace1"
ui_expect_new "Move Face card: MoveFace1: 1 face of Body1, translate (5, 0, 0) mm" "the selection card names it"
ui_step "clear the selection (Esc)"        ui_key Escape
ui_mark
ui_step "double-click MoveFace1"           ui_double_click_logged "Timeline MoveFace1"
ui_expect_new "Editing F3 with Move Face" "the same panel edits it"
ui_expect_new "Move Face Faces: 1 face [face $RIGHT of F2.b0]" "with its face"
ui_expect_new "Move Face X Distance: 5" "and its motion"
ui_step "X 10"                             ui_type_in "Panel Move Face input x" "10"
ui_expect_log "Panel Move Face note: Move Face: 1 face of Body1, translate (10, 0, 0) mm" "previewed at 10 mm"
ui_mark
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Edited F3" "edited"
ui_expect_volume "Body Body1 (F2.b0): volume [0-9.]* -> \([0-9.]*\) mm3" 56000 "the block 70 long"

echo "--- The top tilted 5 degrees about its front edge"
ui_step "fit (F6)"                         ui_key F6
ui_mark
ui_step "move face (search)"               ui_command "Move Face"
ui_expect_new "Pick places:" "it logs where its faces are"
ui_step "pick the top"                     ui_click_pick "face F2.b0/$TOP"
ui_expect_log "Move Face Faces: 1 face [face $TOP of F2.b0]" "the top picked"
ui_step "Rotate"                           ui_choose "Panel Move Face input type" $ROTATE
ui_mark
ui_step "the axis input"                   ui_click_logged "Panel Move Face input axis"
ui_expect_new "Pick places:" "the axis input logs its edges"
ui_step "pick the top's front edge"        ui_click_pick "edge F2.b0/$TOP_FRONT"
ui_expect_log "Move Face Axis: 1 edge [edge $TOP_FRONT of F2.b0]" "the front edge is the axis"
ui_mark
ui_step "5 degrees"                        ui_type_in "Panel Move Face input angle" "5"
ui_expect_new "Preview Move Face: ok" "previewed"
ui_expect_new "Panel Move Face note: Move Face: 1 face of Body1, rotate 5 deg about an edge" "the note names it"
ui_mark
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Moved faces: " "placed"
for _ in $(seq 1 50); do
  volume=$(sed -n 's/.*Body Body1 (F2.b0): volume 56000.000 -> \([0-9.]*\) mm3.*/\1/p' "$UI_LOG" | tail -1)
  [ -n "$volume" ] && break
  sleep 0.2
done
# 70 x (20 x 40 +- 40 x 40 / 2 x tan 5 deg): up or down as the edge runs.
awk -v v="$volume" 'BEGIN {
  t = sin(5 * atan2(0, -1) / 180) / cos(5 * atan2(0, -1) / 180)
  up = 70 * (800 + 800 * t); down = 70 * (800 - 800 * t)
  exit !(v != "" && ((v - up) ^ 2 < 1e-4 || (v - down) ^ 2 < 1e-4)) }' ||
  ui_fail "the tilted top: volume '$volume' mm3, expected 60899.364 or 51100.636"
echo "ok   the top tilted about its front edge: $volume mm3"

echo "--- A motion the neighbours cannot follow"
ui_mark
ui_step "move face (search)"               ui_command "Move Face"
ui_expect_new "Pick places:" "it logs where its faces are"
ui_step "pick the top"                     ui_click_pick "face F2.b0/$TOP"
ui_step "Translate"                        ui_choose "Panel Move Face input type" $TRANSLATE
ui_mark
ui_step "Z -30 (below the bottom)"         ui_type_in "Panel Move Face input z" "-30"
ui_expect_new "Preview Move Face: failed: " "the model refuses it"
ui_expect_new "Panel Move Face error: " "the panel says why in red"
grep "^Panel Move Face error: " "$UI_LOG" | tail -1
ui_mark
ui_step "Enter (refused)"                  ui_key Return
if tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -q "^Moved faces: "; then
  ui_fail "OK placed a motion the model refused"
fi
echo "ok   OK is refused"
ui_step "cancel (Esc)"                     ui_key Escape

ui_stop_app
ui_finish "Move Face"
