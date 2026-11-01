#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/JointCommands.cpp app/browser app/MainWindowJoints.cpp
# check-all sources: core/model/src/constraints.rs core/model/src/features/constraint_set.rs
# check-all sources: core/model/src/joints.rs core/solver/src/rigid.rs tools/cli
# Assembly constraints (mitcad#148) through the real UI, on a design
# mitcad-cli makes: Plate:1 (a 40 x 40 x 10 block with a hole of diameter
# 16 through its middle, grounded) and Pin:1 (a cylinder of diameter 16 and
# height 30 beside it):
#   - Assembly Constraint: the pin's top rim, then the hole's top rim: the pin is
#     a ghost until the hole is picked, the row's type follows the picks
#     (Automatic (Insert)), Flip turns the rims the same way, and OK
#     inserts the pin through the hole.
#   - Undone, a row of the pin's top rim and the hole's face: Automatic
#     (Insert (Axis)); OK puts the pin on the hole's axis at the height it
#     had, a second row added and removed on the way; the Joints folder
#     lists the set with its row; a drag up the axis moves the pin along
#     it and nowhere else.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-constraint-test.sh [screenshot.png]

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-constraint.XXXXXX)
FILE=$WORK/constraint.mitcad
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
RECT='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'

# Plate (C1, Plate:1 O1): Sketch1 F1, Extrude1 F2 (Body1), the hole
# Cylinder1 F3; Pin (C2, Pin:1 O2): Cylinder2 F4 (Body2) at (60, 20).
sed "s/RECT/$RECT/g" > "$WORK/model.json" << 'EOF'
[
  {"cmd": "create_component", "name": "Plate"},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 40, "height": 40},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "RECT"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "cylinder", "plane": "xy", "center": [20, 20],
    "diameter": 16, "height": 10, "operation": "cut"}},
  {"cmd": "activate_component", "component": "Root"},
  {"cmd": "create_component", "name": "Pin", "transform": {"translation": [60, 20, 0]}},
  {"cmd": "add_feature", "def": {"type": "cylinder", "plane": "xy", "center": [0, 0],
    "diameter": 16, "height": 30, "operation": "new_body"}},
  {"cmd": "activate_component", "component": "Root"},
  {"cmd": "ground_occurrence", "occurrence": "Plate:1"}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

PIN_TOP="edge F4.b0/E{F4:side0|F4:top}"
HOLE_TOP="edge F2.b0/E{F2:end($RECT)|F3:side0}"
HOLE_FACE="face F2.b0/F3:side0"

# check "description" expectations...: the saved design opened by mitcad-cli.
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
ui_expect_log "Body F4.b0 in O2 at" "the pin is shown"

echo "--- Insert: the pin's top rim into the hole's top rim"
ui_mark
ui_step "constraint (search)"            ui_command "Assembly Constraint"
ui_expect_new "Command Assembly Constraint started" "Constraint started"
ui_expect_new "Pick $PIN_TOP" "the pin's top rim can be picked"
ui_mark
ui_step "pick the pin's top rim"         ui_click_pick "$PIN_TOP"
ui_expect_new "Assembly Constraint Geometry 1: 1 edge" "geometry 1 on the pin"
ui_expect_new "View ghosts: O2 (1 body)" "the pin is a ghost until geometry 2 is picked"
ui_expect_new "Pick $HOLE_TOP" "the hole's rim can be picked"
ui_mark
ui_step "pick the hole's top rim"        ui_click_pick "$HOLE_TOP"
ui_expect_new "Assembly Constraint Geometry 2: 1 edge" "geometry 2 on the plate"
ui_expect_new "View ghosts: none" "with both picks the pin is shown normally"
ui_expect_new "options rows.0.type: Automatic (Insert) |" "two circles: an insert"
ui_expect_new "Preview Assembly Constraint: ok" "previewed"
ui_expect_new "Preview Assembly Constraint moves O2" "the preview shows the pin where the row puts it"
ui_mark
ui_step "flip"                           ui_click_logged "Panel Assembly Constraint input rows.0.flip"
ui_expect_new "Assembly Constraint: Flip = on" "the rims the same way: the pin through the hole"
ui_expect_new "Preview Assembly Constraint: ok" "previewed"
[ -n "$SHOT" ] && ui_capture "$SHOT"
ui_mark
ui_step "OK (Enter)"                     ui_key Return
ui_expect_new "Added constraint Constraint1: insert placed, moved O2; DOF 1" "the pin inserted, turning about the axis"
save
check "the pin's top at the plate's top, through the hole" \
  '{"expect": {"occurrence": "Pin:1", "body": "Body2", "center": [20, 20, -5], "tolerance": 1e-6}}'

echo "--- Insert (Axis): the pin's top rim on the hole's axis, depth free"
ui_mark
ui_step "undo (Ctrl+Z)"                  ui_key ctrl+z
ui_expect_new "Undo: Add Constraint1" "the insert taken back"
ui_mark
ui_step "constraint (search)"            ui_command "Assembly Constraint"
ui_expect_new "Command Assembly Constraint started" "Constraint started"
ui_expect_new "Pick $PIN_TOP" "the pin's top rim can be picked"
ui_mark
ui_step "pick the pin's top rim"         ui_click_pick "$PIN_TOP"
ui_expect_new "View ghosts: O2 (1 body)" "the pin is a ghost"
ui_expect_new "Pick $HOLE_FACE" "the hole's face can be picked"
ui_mark
ui_step "pick the hole's face"           ui_click_pick "$HOLE_FACE"
ui_expect_new "options rows.0.type: Automatic (Insert (Axis)) |" "a circle and a hole: an axis insert"
ui_expect_new "Preview Assembly Constraint: ok" "previewed"
ui_mark
ui_step "add a row"                      ui_click_logged "Panel Assembly Constraint add rows"
ui_expect_new "Assembly Constraint: added rows row 2" "a second row, without closing the panel"
ui_expect_new "Panel Assembly Constraint input rows.1.a at" "its inputs shown"
ui_mark
ui_step "remove it"                      ui_click_logged "Panel Assembly Constraint remove rows.1"
ui_expect_new "Preview Assembly Constraint: ok" "one row again"
ui_mark
ui_step "OK (Enter)"                     ui_key Return
ui_expect_new "Added constraint Constraint1: insert_axis placed, moved O2; DOF 2" \
  "the pin on the axis: it slides and turns"
save
check "the pin on the hole's axis at the height it had" \
  '{"expect": {"occurrence": "Pin:1", "body": "Body2", "center": [20, 20, 15], "tolerance": 1e-6}}'
ui_expect_log "/Joints at" "the browser has a Joints folder for the set"

echo "--- A drag up the axis"
ui_step "fit (F6)"                       ui_key F6
ui_expect_log "Body F4.b0 in O2 at" "the pin is shown"
ui_mark
ui_step "drag the pin up and aside"      ui_drag_from "Body F4.b0 in O2" 40 -80
ui_expect_new "Drag of Pin:1 started" "the drag takes the pin"
ui_expect_new "Dragged Pin:1 to (" "the drag kept"
save
"$CLI" info "$FILE" > "$WORK/info.log" 2>&1 || { cat "$WORK/info.log"; ui_fail "mitcad-cli info"; }
centre=$(sed -n 's/.*Pin:1\/Body2 .*center of mass \[\(.*\)\]$/\1/p' "$WORK/info.log")
read -r cx cy cz <<< "$(echo "$centre" | tr -d ',')"
[ "$cx" = "20.000" ] && [ "$cy" = "20.000" ] || ui_fail "the pin left the hole's axis: centre $centre"
awk -v z="$cz" 'BEGIN { exit !(z > 15.5) }' || ui_fail "the pin did not move up the axis: centre $centre"
echo "ok   the pin moved up the axis to z = $cz and stays on it"

grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
ui_finish "UI constraint test"
