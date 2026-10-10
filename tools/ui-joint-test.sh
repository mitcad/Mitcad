#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands app/MainWindowJoints.cpp
# Joints between components (mitcad#55) through the real UI, on a design
# mitcad-cli makes: Plate:1 (a 40 x 40 x 10 block, grounded), Pin:1 (a
# 10 x 10 x 20 block at x = 100) and Cap:1 (a 10 x 10 x 5 block at y = 100):
#   - The ASSEMBLE group in the ribbon.
#   - Joint (J): the pin's top face onto the plate's top face, revolute,
#     flipped, limited, at rest 30 degrees; the preview shows the pin where
#     the joint puts it; the handles of the offset and the angle stand on
#     the joint's frame. The saved design, checked by mitcad-cli: the pin
#     stands on the plate, turned 30 degrees.
#   - The browser's Joints folder: the joint's row, the component's degrees
#     of freedom; the joint edited from the timeline (rest 60 degrees).
#   - Drive Joint from the joint's row in the browser: 45 degrees, kept as
#     the joint's position.
#   - Joint Origin on the cap's top face in the cap's component (activated
#     with its radio button), 5 mm up; As-Built Joint of the cap to the
#     plate; Rigid Group of the pin and the cap, picked in the browser.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-joint-test.sh [screenshot.png]
# check-all sources: tools/cli

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-joint.XXXXXX)
FILE=$WORK/joints.mitcad
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
RECT='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'

# Plate (C1, Plate:1 O1): Sketch1 F1, Extrude1 F2 (Body1); Pin (C2, Pin:1
# O2): Sketch2 F3, Extrude2 F4 (Body2); Cap (C3, Cap:1 O3): Sketch3 F5,
# Extrude3 F6 (Body3).
sed "s/RECT/$RECT/g" > "$WORK/model.json" << 'EOF'
[
  {"cmd": "create_component", "name": "Plate"},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 40, "height": 40},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "RECT"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
  {"cmd": "activate_component", "component": "Root"},
  {"cmd": "create_component", "name": "Pin", "transform": {"translation": [100, 0, 0]}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F3", "corner": [0, 0], "width": 10, "height": 10},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F3", "region": "RECT"}],
    "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
  {"cmd": "activate_component", "component": "Root"},
  {"cmd": "create_component", "name": "Cap", "transform": {"translation": [0, 100, 0]}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F5", "corner": [0, 0], "width": 10, "height": 10},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F5", "region": "RECT"}],
    "extent": {"type": "distance", "distance": 5}, "operation": "new_body"}},
  {"cmd": "activate_component", "component": "Root"},
  {"cmd": "ground_occurrence", "occurrence": "Plate:1"}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# check "description" expectations...: the saved design opened by mitcad-cli,
# with the script's expectations (commands.md, "Scripts").
check() {
  local description=$1; shift
  local IFS=,
  echo "[$*]" > "$WORK/check.json"
  "$CLI" run "$WORK/check.json" --open "$FILE" > "$WORK/check.log" 2>&1 ||
    { cat "$WORK/check.log"; ui_fail "$description"; }
  echo "ok   $description"
}

save() {
  ui_mark
  ui_key ctrl+s
  ui_expect_new "Saved $FILE" "saved"
}

ui_start_display
ui_start_app --open "$FILE"
ui_step "fit (F6)"                       ui_key F6
ui_expect_log "Ribbon SOLID/ASSEMBLE at" "the ASSEMBLE group is in the ribbon"

echo "--- Joint: the pin's top face onto the plate's top face"
ui_mark
ui_step "joint (J)"                      ui_key j
ui_expect_new "Command Joint started" "Joint started"
ui_expect_new "Pick places:" "Joint logs where its picks are"
ui_step "pick the pin's top face"        ui_click_pick "face F4.b0/F4:end("
ui_expect_log "Joint Origin A: 1 face [face F4:end($RECT) of F4.b0 in O2]" "origin A on the pin"
ui_expect_log "Manipulator Joint offset at" "the offset's arrow stands on A's frame"
ui_mark
ui_step "pick the plate's top face"      ui_click_pick "face F2.b0/F2:end("
ui_expect_log "Joint Origin B: 1 face [face F2:end($RECT) of F2.b0 in O1]" "origin B on the plate"
ui_expect_new "Preview Joint: ok" "previewed"
ui_expect_new "Preview Joint moves O2" "the preview shows the pin where the joint puts it"
ui_expect_new "Manipulator Joint angle at" "the angle's ring stands on B's frame"
ui_step "revolute"                       ui_choose "Panel Joint input kind" 1
ui_expect_log "Joint: Type = Revolute" "revolute"
ui_step "flip"                           ui_click_logged "Panel Joint input flip"
ui_expect_log "Joint: Flip = on" "flipped: the faces meet"
ui_step "limits of the turn"             ui_click_logged "Panel Joint input limit_rz"
ui_expect_log "Joint: Turn Z Limits = on" "limits on"
ui_step "minimum -90"                    ui_type_in "Panel Joint input min_rz" "-90"
ui_step "maximum 90"                     ui_type_in "Panel Joint input max_rz" "90"
ui_step "a rest value"                   ui_click_logged "Panel Joint input rest_on_rz"
ui_mark
ui_step "rest 30"                        ui_type_in "Panel Joint input rest_rz" "30"
ui_expect_new "Joint Rest Value: 30 = 30 deg" "rest 30 degrees"
ui_expect_new "Preview Joint: ok" "previewed"
ui_mark
ui_step "OK (Enter)"                     ui_key Return
ui_expect_new "Added joint Joint1 (revolute): placed, rz 30 deg, moved O2" "the pin placed by the joint"
ui_expect_new "Added Joint as F7" "Joint1 is F7"
ui_expect_new "Browser joints: Joint1 placed" "the browser has the joint"
ui_expect_log "Browser Root/Joints/Joint1 at" "in the root's Joints folder"
save
check "the pin stands on the plate, turned 30 degrees about its normal" \
  '{"expect": {"occurrence": "Pin:1", "body": "Body2", "center": [20, 20, 20], "tolerance": 1e-6}}' \
  '{"expect": {"query": {"query": "joints"}, "result": {"joints": [{"name": "Joint1", "kind": "revolute", "state": "placed", "values": {"rz": 0.5235987755982988}, "limits": {"rz": {"min": -1.5707963267948966, "max": 1.5707963267948966}}}]}}}'
[ -n "$SHOT" ] && ui_capture "$SHOT"

echo "--- Joint1 edited from the timeline: rest 60 degrees"
ui_mark
ui_step "double-click Joint1"            ui_double_click_logged "Timeline Joint1"
ui_expect_new "Editing F7 with Joint" "editing Joint1"
ui_expect_new "Joint Origin A: 1 face [face F4:end($RECT) of F4.b0 in O2]" "origin A came from the joint"
ui_expect_new "Joint Rest Value: 30 deg" "the rest value came from the joint"
ui_step "rest 60"                        ui_type_in "Panel Joint input rest_rz" "60"
ui_expect_log "Joint Rest Value: 60 = 60 deg" "rest 60 degrees"
ui_mark
ui_step "OK (Enter)"                     ui_key Return
ui_expect_new "Edited F7" "Joint1 edited"
save
check "the pin turned 60 degrees" \
  '{"expect": {"query": {"query": "joints"}, "result": {"joints": [{"name": "Joint1", "state": "placed", "values": {"rz": 1.0471975511965976}}]}}}'

echo "--- Drive Joint from the browser"
ui_mark
ui_step "joint's menu (right click)"     ui_click_logged "Browser Root/Joints/Joint1" 3
ui_expect_new "Context menu: Edit Joint1 | Drive Joint1" "the joint's menu offers Drive"
ui_step "drive"                          ui_menu_choose "Drive Joint1"
ui_expect_new "Command Drive Joint started" "Drive Joint started"
ui_expect_new "Drive Joint Joint: 1 feature [feature F7]" "on Joint1"
ui_expect_new "Drive Joint Angle: 60 deg" "the turn where it is"
ui_expect_new "Manipulator Drive Joint angle at" "a ring on the joint's frame"
ui_mark
ui_step "45 degrees"                     ui_type_in "Panel Drive Joint input angle" "45"
ui_expect_new "Preview Drive Joint: ok" "the joint driven while the value changes"
ui_mark
ui_step "OK (Enter)"                     ui_key Return
ui_expect_new "Drove joint Joint1 (revolute): placed, rz 45 deg" "driven to 45 degrees"
save
check "the joint's position is 45 degrees" \
  '{"expect": {"query": {"query": "joints"}, "result": {"joints": [{"name": "Joint1", "position": {"rz": 0.7853981633974483}, "values": {"rz": 0.7853981633974483}}]}}}'

echo "--- Dragging the pin in the view: the joint turns"
ui_mark
ui_step "drag the pin sideways"          ui_drag_from "Body F4.b0 in O2" 80 0
ui_expect_new "Drag of Pin:1 started" "the drag takes the pin"
ui_expect_new "Dragged Pin:1 to (" "the drag kept"
turn=$(grep "^Dragged Pin:1 to (" "$UI_LOG" | tail -1 | sed -n 's/.*: Joint1 rz \([-0-9.]*\) deg$/\1/p')
awk -v t="$turn" 'BEGIN { exit !(t != "" && (t - 45) ^ 2 > 1 && t >= -90 && t <= 90) }' ||
  ui_fail "the drag left Joint1 at '$turn' degrees"
echo "ok   Joint1 turned to $turn degrees, within its limits"
ui_mark
ui_step "undo the drag (Ctrl+Z)"         ui_key ctrl+z
ui_expect_new "Undo: Drag Pin:1" "the drag is one undo step"
ui_mark
ui_step "a click on the pin"             ui_click_logged "Body F4.b0 in O2"
ui_expect_new "Selected: 1 face [face F4:" "a click without a drag still selects"
ui_step "clear the selection (Esc)"      ui_key Escape

echo "--- Animate Joint: Joint1 through its limits, previews only"
ui_mark
ui_step "animate (search)"               ui_command "Animate Joint"
ui_expect_new "Animating Joint1 rz: 36 frames from 45 deg to 45 deg" "from 45 degrees to the limits and back"
ui_expect_new "Animated Joint1: 36 frames" "every frame shown" 30
ui_mark
ui_step "undo (Ctrl+Z)"                  ui_key ctrl+z
ui_expect_new "Undo: Drive Joint1" "the animation left no undo step: the last is the drive"

echo "--- Joint Origin on the cap's top face, in the cap's component"
ui_mark
ui_step "activate Cap"                   ui_click_logged "Browser radio Root/Cap:1"
ui_expect_new "Activated Cap" "Cap is active"
ui_mark
ui_step "joint origin (search)"          ui_command "Joint Origin"
ui_expect_new "Pick places:" "Joint Origin logs where its picks are"
ui_step "pick the cap's top face"        ui_click_pick "face F6.b0/F6:end("
ui_expect_log "Joint Origin Geometry: 1 face" "the cap's top face"
ui_step "offset 5"                       ui_type_in "Panel Joint Origin input offset" "5"
ui_expect_log "Joint Origin Offset: 5 = 5 mm" "5 mm"
ui_mark
ui_step "OK (Enter)"                     ui_key Return
ui_expect_new "Added joint origin JointOrigin1: origin (5, 5, 10), normal (0, 0, 1)" "a frame 5 mm above the cap's top"
ui_expect_log "Browser Root/Cap:1/Joints/JointOrigin1 at" "in the cap's Joints folder"
ui_mark
ui_step "activate Root"                  ui_click_logged "Browser radio Root"
ui_expect_new "Activated Root" "the root is active again"

echo "--- As-Built Joint: the cap to the plate where they are"
ui_mark
ui_step "as-built joint (search)"        ui_command "As-Built Joint"
ui_expect_new "Command As-Built Joint started" "As-Built Joint started"
ui_step "the cap in the browser"         ui_click_logged "Browser Root/Cap:1"
ui_expect_log "As-Built Joint Component A: 1 component [component C3 in O3]" "A: the cap"
ui_step "the plate in the browser"       ui_click_logged "Browser Root/Plate:1"
ui_expect_log "As-Built Joint Component B: 1 component [component C1 in O1]" "B: the plate"
ui_expect_log "Preview As-Built Joint: ok" "previewed"
ui_mark
ui_step "OK (Enter)"                     ui_key Return
ui_expect_new "Added as-built joint AsBuiltJoint1 (rigid): " "the as-built joint added"
ui_expect_new "Browser joints: Joint1 placed, AsBuiltJoint1 " "the browser has both joints"

echo "--- Rigid Group: the pin and the cap, picked in the browser"
ui_mark
ui_step "rigid group (search)"           ui_command "Rigid Group"
ui_expect_new "Command Rigid Group started" "Rigid Group started"
ui_step "the pin in the browser"         ui_click_logged "Browser Root/Pin:1"
ui_step "the cap in the browser"         ui_click_logged "Browser Root/Cap:1"
ui_expect_log "Rigid Group Components: 2 components" "two components"
ui_expect_log "Preview Rigid Group: ok" "previewed"
ui_mark
ui_step "OK (Enter)"                     ui_key Return
ui_expect_new "Added rigid group RigidGroup1: Pin:1, Cap:1" "the group added"
ui_expect_log "Browser Root/Joints/RigidGroup1 at" "in the Joints folder"
save
check "the saved design has the joints, the joint origin and the group" \
  '{"expect": {"occurrence": "Pin:1", "body": "Body2", "center": [20, 20, 20], "tolerance": 1e-6}}' \
  '{"expect": {"occurrence": "Cap:1", "body": "Body3", "center": [5, 105, 2.5], "tolerance": 1e-6}}' \
  '{"expect": {"query": {"query": "joints"}, "result": {"joints": [{"name": "Joint1"}, {"name": "AsBuiltJoint1", "type": "as_built_joint", "kind": "rigid"}], "rigid_groups": [{"name": "RigidGroup1"}]}}}' \
  '{"expect": {"query": {"query": "datum", "uid": "F8"}, "result": {"origin": [5, 5, 10]}}}'

grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
ui_finish "UI joint test"
