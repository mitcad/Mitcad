#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/ExtrudeCommands.cpp app/commands/CommandSupport.cpp
# check-all sources: app/framework/CommandSession.cpp app/framework/CommandSessionAuto.cpp
# check-all sources: app/framework/Numbers.cpp
# check-all sources: app/framework/AutoOperationPolicy.hpp app/framework/Command.hpp
# check-all sources: core/model/src/document/auto_operation.rs core/model-api/src/lib.rs
# check-all sources: geometry/src/contact.cpp geometry/src/boolean.cpp tools/cli
# Extrude's automatic operation (mitcad#289) for new features, and edits that keep
# their stored operation (mitcad#868), through the real UI, on a
# design mitcad-cli makes: a 40 x 30 block 20 deep under the XY plane
# (Body1, F2.b0; its top face lies in XY), a square on XY over the block
# (Sketch2, F3) and a square on XY far from it (Sketch3, F4).
#   1. Extrude of the far square: New Body, nothing chosen automatically.
#   2. Extrude of the square on the block's face: Join at once; a negative
#      distance makes it Cut, a positive one Join again; OK joins. A typed
#      "-5 mm" has no "= -5 mm" line under the field, an expression has
#      one (mitcad#994).
#   3. Undone; Extrude of the same square with New Body chosen by hand:
#      the choice stays when the distance goes into the block; OK adds a
#      new body.
#   2a. Editing the committed Join keeps it (mitcad#868): going into the
#       block, then flipped into it, the Operation field stays Join; Cut
#       chosen by hand still works; cancelled.
#   4. Editing the deliberate touching New Body keeps its stored choice.
# tools/ui-windows-auto-operation-test.ps1 is the Windows counterpart.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-auto-operation-test.sh [screenshot.png]

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-auto-operation.XXXXXX)
FILE=$WORK/auto.mitcad
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
  {"cmd": "sketch.add_rectangle", "sketch": "F3", "corner": [10, 10], "width": 10, "height": 10},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F4", "corner": [100, 0], "width": 10, "height": 10}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

ui_start_display
ui_start_app --open "$FILE"
ui_step "fit (F6)"                       ui_key F6
ui_expect_log "Bodies shown: F2.b0" "the block shown"

echo "--- 1. A profile away from the bodies: New Body"
ui_mark
ui_step "extrude (E)"                    ui_key e
ui_expect_new "Extrude Profiles: 1 profile [profile $R of F4]" "the far square taken"
ui_expect_new "Preview Extrude: ok" "previewed"
ui_step "distance 15"                    ui_type_in "Panel Extrude input distance" "15"
ui_expect_new "Extrude Distance: 15 = 15 mm" "15 mm up"
ui_wait_idle
no_new "Extrude: automatic Operation" "no operation chosen: it stays New Body"
ui_mark
ui_step "OK (Enter)"                     ui_key Return
ui_expect_new "Added extrude (New Body), distance 15" "a new body added"

echo "--- 2. A profile on the block's face: Join, Cut going in, Join coming out"
ui_mark
ui_step "extrude (E)"                    ui_key e
ui_expect_new "Extrude Profiles: 1 profile [profile $R of F3]" "the square on the face taken"
ui_expect_new "Extrude: automatic Operation = Join" "Join at once (10 mm up from the face)"
ui_expect_new "Extrude Local bodies: 1 body [body F2.b0]" "the block is the object"
ui_expect_new "Preview Extrude: ok" "the join previewed"
ui_mark
ui_step "distance -5"                    ui_type_in "Panel Extrude input distance" "-5"
ui_expect_new "Extrude: automatic Operation = Cut" "Cut going into the block"
ui_expect_new "Preview Extrude: ok" "the cut previewed"
[ -n "$SHOT" ] && ui_capture "$SHOT"
# The "= value" line under the field (mitcad#994): none for a signed
# number with its unit, as a drag of the arrow writes it; an expression's.
ui_mark
ui_step "distance -5 mm"                 ui_type_in "Panel Extrude input distance" "-5 mm"
ui_expect_new "Extrude Distance: -5 mm = -5 mm" "-5 mm evaluated"
no_new "Panel Extrude distance hint:" "no '= -5 mm' line under the field: the panel keeps its height"
ui_step "distance -4 mm - 1 mm"          ui_type_in "Panel Extrude input distance" "-4 mm - 1 mm"
ui_expect_new "Panel Extrude distance hint: = -5 mm" "the expression's value shown under the field"
ui_mark
ui_step "distance 5"                     ui_type_in "Panel Extrude input distance" "5"
ui_expect_new "Extrude: automatic Operation = Join" "Join again coming out"
ui_expect_new "Panel Extrude distance hint cleared" "a plain number: the line under the field goes"
ui_expect_new "Preview Extrude: ok" "the join previewed again"
ui_focus_main
ui_mark
ui_step "OK (Enter)"                     ui_key Return
ui_expect_new "Added extrude (Join), distance 5" "joined"
ui_expect_volume "Body Body1 (F2.b0): volume 24000.000 -> \([0-9.]*\) mm3" 24500 "the block grew by the square's 500 mm3"
echo "--- 2a. Editing keeps the stored Join; Operation changes only by hand (mitcad#868)"
ui_mark
ui_step "edit automatic Extrude3"         ui_double_click_logged "Timeline Extrude3"
ui_expect_new "Extrude: editing keeps the stored operation" "the edit keeps the stored Join"
ui_expect_new "operation=join" "it opens with its Join"
ui_expect_new "Preview Extrude: ok" "the stored Join previewed"
ui_mark
ui_step "edited distance -5"              ui_type_in "Panel Extrude input distance" "-5"
ui_expect_new "Extrude Distance: -5 = -5 mm" "the edit goes into the block"
ui_expect_new "Preview Extrude: ok" "the edited Join previewed going in"
ui_wait_idle
no_new "Extrude: automatic Operation" "the edited Join stays Join going into the block"
ui_mark
ui_step "edited distance 5"               ui_type_in "Panel Extrude input distance" "5"
ui_expect_new "Extrude Distance: 5 = 5 mm" "out of the block again"
ui_step "flip into the block"             ui_click_logged "Panel Extrude input flip"
ui_expect_new "Extrude: Flip = on" "flipped into the block, as the reported boss"
ui_expect_new "Preview Extrude: ok" "the flipped Join previewed"
ui_wait_idle
no_new "Extrude: automatic Operation" "Flip into the block leaves the stored Join"
no_new "Extrude: Operation = " "nothing changes the Operation field"
ui_mark
ui_step "Cut by hand (one down from Join)" ui_choose "Panel Extrude input operation" 1
ui_expect_new "Extrude: Operation = Cut" "the user still changes Operation by hand"
ui_expect_new "Preview Extrude: ok" "the hand-picked Cut previewed"
ui_focus_main
ui_step "cancel the edit"                 ui_key Escape
ui_expect_new "Command Extrude cancelled" "the original stored Join retained"
ui_focus_main

ui_mark
ui_step "undo (Ctrl+Z)"                  ui_key ctrl+z
ui_expect_new "Undo: " "the join undone"

echo "--- 3. New Body chosen by hand stays when the distance goes in"
ui_mark
ui_step "extrude (E)"                    ui_key e
ui_expect_new "Extrude Profiles: 1 profile [profile $R of F3]" "the square on the face again"
ui_expect_new "Extrude: automatic Operation = Join" "Join at once"
ui_expect_new "Preview Extrude: ok" "previewed"
ui_mark
ui_step "New Body (one up from Join)"    ui_choose "Panel Extrude input operation" -1
ui_expect_new "Extrude: Operation = New Body" "New Body chosen"
ui_expect_new "Extrude: operation chosen by hand" "the automatic choice is off"
ui_mark
ui_step "distance -5"                    ui_type_in "Panel Extrude input distance" "-5"
ui_expect_new "Extrude Distance: -5 = -5 mm" "into the block"
ui_expect_new "Preview Extrude: ok" "previewed"
ui_step "distance 5"                     ui_type_in "Panel Extrude input distance" "5"
ui_expect_new "Extrude Distance: 5 = 5 mm" "out of the block"
ui_step "distance -5 again"              ui_type_in "Panel Extrude input distance" "-5"
ui_wait_idle
no_new "Extrude: automatic Operation" "the hand-picked New Body stays"
ui_focus_main
ui_mark
ui_step "OK (Enter)"                     ui_key Return
ui_expect_new "Added extrude (New Body), distance -5" "a new body in the block"

echo "--- 4. Editing a deliberate touching New Body keeps its stored operation"
ui_mark
ui_step "edit Extrude3 (double-click)"   ui_double_click_logged "Timeline Extrude3"
ui_expect_new " with Extrude" "editing Extrude3"
ui_expect_new "operation=new_body" "it opens with its New Body"
ui_expect_new "Extrude: editing keeps the stored operation" "the edit keeps the deliberate New Body"
ui_mark
ui_step "distance -8"                    ui_type_in "Panel Extrude input distance" "-8"
ui_expect_new "Preview Extrude: ok" "the edit previewed"
ui_wait_idle
no_new "Extrude: automatic Operation" "editing preserves the deliberate New Body"
ui_focus_main
ui_mark
ui_step "OK (Enter)"                     ui_key Return
ui_expect_new "Edited F" "Extrude3 edited"
ui_mark
ui_step "edit Extrude3 again"            ui_double_click_logged "Timeline Extrude3"
ui_expect_new " with Extrude" "editing Extrude3 again"
ui_expect_new "operation=new_body" "still New Body"
ui_step "cancel (Escape)"                ui_key Escape
ui_expect_new "Command Extrude cancelled" "edit cancelled"

grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
ui_finish "UI auto operation test"
