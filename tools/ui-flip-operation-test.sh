#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/ExtrudeCommands.cpp
# check-all sources: app/framework/CommandSession.cpp app/framework/CommandSessionAuto.cpp
# check-all sources: app/framework/AutoOperationPolicy.hpp app/framework/Command.hpp
# check-all sources: core/model/src/document/auto_operation.rs core/model-api/src/lib.rs
# check-all sources: geometry/src/contact.cpp geometry/src/boolean.cpp tools/cli
# A new extrusion's direction flip whose preview fails tries the model's
# other operation (mitcad#2216), through the real UI, on a design
# mitcad-cli makes: a 40 x 30 block 20 deep under the XY plane (Body1,
# F2.b0; its top face lies in XY), a square on XY far from it (Sketch2, F3)
# and a square on XY over the block (Sketch3, F4; Extrude takes it first).
# A new Extrude starts at 0 mm (mitcad#2481): each gets 10 mm typed first.
#   1. Automatic: the square on the block's face is Join; Flip into the
#      block makes it Cut, Flip back Join (mitcad#289); cancelled.
#   2. Cut chosen by hand (out of the block it misses, so Flip turns on by
#      itself and the Cut works going in, mitcad#2283; that turn tries no
#      other operation); Flip out again by hand: the Cut fails, Join is
#      tried and taken (logged "flipped
#      out of the body"). Flip into the block with that Join: the preview
#      works (the Join itself, or Cut when the Join fails, logged "flipped
#      into the body"); Flip out: Join; OK joins.
#   3. Cut chosen by hand on the far square: it fails both ways and a flip
#      changes nothing (the model suggests New Body, which no flip takes).
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-flip-operation-test.sh [screenshot.png]

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-flip-operation.XXXXXX)
FILE=$WORK/flip.mitcad
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
R='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'

# no_new "text" description: no such line since the mark.
no_new() {
  tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qF -- "$1" && ui_fail "$2"
  echo "ok   $2"
}

cat > "$WORK/model.json" << EOF
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 40, "height": 30},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$R"}],
    "extent": {"type": "distance", "distance": -20}, "operation": "new_body"}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F3", "corner": [100, 0], "width": 10, "height": 10},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F4", "corner": [10, 10], "width": 10, "height": 10}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

ui_start_display
ui_start_app --open "$FILE"
ui_step "fit (F6)"                       ui_key F6
ui_expect_log "Bodies shown: F2.b0" "the block shown"

echo "--- 1. Automatic: Join, Flip into the block Cut, Flip back Join"
ui_mark
ui_step "extrude (E)"                    ui_key e
ui_expect_new "Extrude Profiles: 1 profile [profile $R of F4]" "the square on the block's face taken"
ui_step "distance 10 mm"                 ui_type_in "Panel Extrude input distance" "10 mm"
ui_expect_new "Extrude: automatic Operation = Join" "Join at once (10 mm typed, up from the face)"
ui_expect_new "Preview Extrude: ok" "the join previewed"
ui_mark
ui_step "flip into the block"            ui_click_logged "Panel Extrude input flip"
ui_expect_new "Extrude: Flip = on" "flipped into the block"
ui_expect_new "Extrude: automatic Operation = Cut" "Cut going into the block"
ui_expect_new "Preview Extrude: ok" "the cut previewed"
ui_mark
ui_step "flip back out"                  ui_click_logged "Panel Extrude input flip"
ui_expect_new "Extrude: Flip = off" "flipped out of the block"
ui_expect_new "Extrude: automatic Operation = Join" "Join again coming out"
ui_expect_new "Preview Extrude: ok" "the join previewed again"
ui_wait_idle
no_new "Extrude: flipped" "the automatic choice needs no flip's retry"
ui_focus_main
ui_mark
ui_step "cancel (Escape)"                ui_key Escape
ui_expect_new "Command Extrude cancelled" "cancelled"

echo "--- 2. Cut chosen by hand: a failed flip out takes Join"
ui_mark
ui_step "extrude (E)"                    ui_key e
ui_expect_new "Extrude Profiles: 1 profile [profile $R of F4]" "the square on the face again"
ui_step "distance 10 mm"                 ui_type_in "Panel Extrude input distance" "10 mm"
ui_expect_new "Extrude: automatic Operation = Join" "Join at once (10 mm typed)"
ui_expect_new "Preview Extrude: ok" "previewed"
ui_mark
ui_step "Cut by hand (one down from Join)" ui_choose "Panel Extrude input operation" 1
ui_expect_new "Extrude: Operation = Cut" "Cut chosen"
ui_expect_new "Extrude: operation chosen by hand" "the automatic choice is off"
ui_expect_new "Extrude: Cut misses, flipped into the body, Flip = on" \
  "a cut out of the block cuts nothing: Flip turns on by itself (mitcad#2283)"
ui_expect_new "Preview Extrude: ok" "the hand-chosen Cut works going in"
ui_wait_idle
no_new "Extrude: flipped" "the turn keeps the hand-chosen Cut (no retry of another operation)"
no_new "Extrude: Operation = Join" "the Operation field stays Cut"
ui_mark
ui_step "flip back out"                  ui_click_logged "Panel Extrude input flip"
ui_expect_new "Extrude: Flip = off" "flipped out of the block"
ui_expect_new "Extrude: flipped, Cut fails, trying Join" "the failed Cut tries Join"
ui_expect_new "Extrude: flipped out of the body, Operation = Join" "Join taken coming out"
ui_expect_new "Extrude Local bodies: 1 body [body F2.b0]" "the block is the object"
ui_expect_new "Preview Extrude: ok" "the join previewed"
[ -n "$SHOT" ] && ui_capture "$SHOT"
ui_mark
ui_step "flip the Join into the block"   ui_click_logged "Panel Extrude input flip"
ui_expect_new "Extrude: Flip = on" "flipped into the block again"
ui_expect_new "Preview Extrude: ok" "the flip into the block previews"
ui_wait_idle
no_new "Preview Extrude: failed" "no failure is left shown going in"
if tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qF "Extrude: flipped into the body, Operation = Cut"; then
  echo "ok   the Join failed going in: Cut taken"
else
  no_new "Extrude: Operation = " "the Join works going in: it stays Join"
fi
ui_mark
ui_step "flip out again"                 ui_click_logged "Panel Extrude input flip"
ui_expect_new "Extrude: Flip = off" "out of the block"
ui_expect_new "Preview Extrude: ok" "previewed coming out"
ui_wait_idle
no_new "Preview Extrude: failed" "no failure is left shown coming out"
ui_focus_main
ui_mark
ui_step "OK (Enter)"                     ui_key Return
ui_expect_new "Added extrude (Join), distance 10" "joined coming out"
ui_expect_volume "Body Body1 (F2.b0): volume 24000.000 -> \([0-9.]*\) mm3" 25000 "the block grew by the square's 1000 mm3"

echo "--- 3. Cut chosen by hand away from the bodies: a flip changes nothing"
ui_mark
ui_step "extrude (E)"                    ui_key e
ui_expect_new "Extrude Profiles: 1 profile [profile $R of F3]" "the far square taken"
ui_step "distance 10 mm"                 ui_type_in "Panel Extrude input distance" "10 mm"
ui_expect_new "Preview Extrude: ok" "the new body previewed"
ui_mark
ui_step "Cut by hand"                    ui_choose_text "Panel Extrude input operation" "Cut"
ui_expect_new "Extrude: Operation = Cut" "Cut chosen"
ui_expect_new "Preview Extrude: failed" "a cut away from the bodies cuts nothing"
ui_mark
ui_step "flip"                           ui_click_logged "Panel Extrude input flip"
ui_expect_new "Extrude: Flip = on" "flipped"
ui_expect_new "Preview Extrude: failed" "the cut fails the other way too"
ui_wait_idle
no_new "Extrude: flipped" "nothing near: no other operation is tried"
no_new "Extrude: Operation = " "Operation stays Cut, its failure shown"
ui_focus_main
ui_mark
ui_step "cancel (Escape)"                ui_key Escape
ui_expect_new "Command Extrude cancelled" "cancelled"

grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
ui_finish "UI flip operation test"
