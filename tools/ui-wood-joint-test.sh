#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/WoodJointCommands.cpp core/model/src/features/wood_joint.rs
# check-all sources: app/commands/MortiseTenonCommands.cpp app/framework/CommandSession.cpp tools/cli
# Wood Joint (mitcad#2604, the Build tab's Connect group) through the real
# UI, with the volumes the model reports, on two pairs of boards mitcad-cli
# makes as tools/ui-mortise-tenon-test.sh's: a rail 100 long, 20 across and
# 60 high (Box1 F1 Body1, Box3 F3 Body3, 120000 mm3 each) whose end face
# side1 stands at x = -10, and a post 40 x 12 x 40 (Box2 F2 Body2, Box4 F4
# Body4, 19200 mm3 each) from x = 0 on, in front of the middle of the
# rail's end; the second pair 200 along y.
#   1. Wood Joint, Type Tongue and Groove: the first rail's end face
#      clicked, its post picked as the Other Part, Depth 30, Thickness and
#      Clearance as they come (6 mm, 0.1 mm). OK: the rail gains the tongue
#      along its whole end, 60 6 30 = 10800 mm3; the post loses the groove
#      6.2 wide, 20.1 into it and through its 40 high, 20.1 6.2 40 =
#      4984.8 mm3. WoodJoint1 (F5) in the timeline.
#   2. WoodJoint1 edited to 25 deep: both bodies built again (60 6 25 =
#      9000 mm3 of tongue, 15.1 6.2 40 = 3744.8 mm3 of groove).
#   3. Wood Joint, Type Dowel on the second pair: one dowel 8 across (as it
#      comes) at the middle of the rail's end, Depth 20 both ways: the rail
#      loses its hole pi 4.1^2 20.1 = 1061.484 mm3, the post the 10.1 of it
#      past its front, pi 4.1^2 10.1 = 533.383 mm3; the dowel is a new body,
#      pi 4^2 40 = 2010.619 mm3.
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-wood-joint-test.sh [screenshot.png]

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-wood-joint.XXXXXX)
trap 'ui_cleanup; rm -rf "$WORK"' EXIT

cat > "$WORK/model.json" << 'EOF'
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": {"type": "origin", "plane": "xy"},
    "corner": [-110, 0], "length": 100, "width": 20, "height": 60, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "box", "plane": {"type": "origin", "plane": "xy"},
    "corner": [0, 4], "length": 40, "width": 12, "height": 40, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "box", "plane": {"type": "origin", "plane": "xy"},
    "corner": [-110, 200], "length": 100, "width": 20, "height": 60, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "box", "plane": {"type": "origin", "plane": "xy"},
    "corner": [0, 204], "length": 40, "width": 12, "height": 40, "operation": "new_body"}}
]
EOF
"$CLI" run "$WORK/model.json" --save "$WORK/boards.mitcad" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli model"; }

ui_start_display
ui_start_app --open "$WORK/boards.mitcad"
ui_step "fit (F6)"                         ui_key F6
ui_expect_log "Bodies shown: " "the boards shown"

echo "--- 1. A tongue along the rail's end, its groove in the post"
ui_mark
ui_step "Wood Joint"                       ui_command "Wood Joint"
ui_expect_new "Command Wood Joint started" "the panel opened"
ui_expect_new "Pick places:" "the picks are logged"
ui_step "Tongue and Groove"                ui_choose "Panel Wood Joint input type" 2
ui_step "click the rail's end face"        ui_click_pick "face F1.b0/F1:side1"
ui_expect_new "Wood Joint End Face: 1 face" "the end face picked"
ui_step "pick the post"                    ui_click_logged "Body F2.b0"
ui_expect_new "Wood Joint Other Part: 1 body [body F2.b0]" "the post picked"
ui_mark
ui_step "depth 30"                         ui_type_in "Panel Wood Joint input depth" "30"
ui_expect_new "Preview Wood Joint: ok" "previewed"
[ -n "$SHOT" ] && ui_capture "$SHOT"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_log "Added wood joint (Tongue and Groove)" "the joint added"
ui_expect_volume "Body Body1 (F1.b0): volume 120000.000 -> \([0-9.]*\) mm3" 130800 "the tongue on the rail"
ui_expect_volume "Body Body2 (F2.b0): volume 19200.000 -> \([0-9.]*\) mm3" 14215.2 "the groove in the post"
ui_expect_log "Timeline WoodJoint1 at " "the joint in the timeline"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"

echo "--- 2. Edited to 25 deep: both bodies again"
ui_mark
ui_step "edit WoodJoint1"                  ui_double_click_logged "Timeline WoodJoint1"
ui_expect_new "Editing F5 with Wood Joint" "the joint opens for editing"
ui_mark
ui_step "depth 25"                         ui_type_in "Panel Wood Joint input depth" "25"
ui_expect_new "Preview Wood Joint: ok" "the edit previewed"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Edited F5" "the joint edited"
ui_expect_volume "Body Body1 (F1.b0): volume 130800.000 -> \([0-9.]*\) mm3" 129000 "the shorter tongue"
ui_expect_volume "Body Body2 (F2.b0): volume 14215.200 -> \([0-9.]*\) mm3" 15455.2 "the shallower groove"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"

echo "--- 3. A dowel between the second rail and its post"
ui_mark
ui_step "Wood Joint"                       ui_command "Wood Joint"
ui_expect_new "Command Wood Joint started" "the panel opened again"
ui_step "Dowel"                            ui_choose "Panel Wood Joint input type" 6
ui_step "click the second rail's end face" ui_click_pick "face F3.b0/F3:side1"
ui_expect_new "Wood Joint End Face: 1 face" "the second end face picked"
# Dowel set Depth to 20 mm when it was chosen: the preview comes with the
# second pick, and typing the same 20 again computes nothing new.
ui_mark
ui_step "pick the second post"             ui_click_logged "Body F4.b0"
ui_expect_new "Wood Joint Other Part: 1 body [body F4.b0]" "the second post picked"
ui_step "depth 20"                         ui_type_in "Panel Wood Joint input depth" "20"
ui_expect_new "Preview Wood Joint: ok" "the dowel previewed"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_log "Added wood joint (Dowel)" "the dowel joint added"
ui_expect_volume "Body Body3 (F3.b0): volume 120000.000 -> \([0-9.]*\) mm3" 118938.516 "the rail's hole"
ui_expect_volume "Body Body4 (F4.b0): volume 19200.000 -> \([0-9.]*\) mm3" 18666.617 "the post's hole"
ui_expect_volume "New body [^ ]* (F6.b0): volume \([0-9.]*\) mm3" 2010.619 "the dowel"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"

ui_finish "UI wood joint test"
