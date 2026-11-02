#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/PlaceInputs.cpp app/commands/PrimitiveCommands.cpp app/commands/CommandSupport.cpp
# check-all sources: app/commands/PulleyCommands.cpp app/commands/GearCommands.cpp app/commands/BearingCommands.cpp
# check-all sources: app/commands/SweepCommands.cpp app/commands/HoleCommands.cpp app/framework/Selection.cpp
# check-all sources: app/framework/ModelShapes.cpp app/framework/CommandSession.cpp app/MainWindow.cpp tools/cli
# Features placed on a fixed plane (mitcad#1137), as importers and MCP
# agents write them (`"plane": {"origin": [0, 0, 36], "normal": [0, 0,
# 1]}`), edited through the real UI. mitcad-cli makes a 60 x 40 x 20 block
# (Sketch1 F1, Extrude1 F2) and on the fixed plane at Z 36: Cylinder1 (F3,
# centre (100, 0)), Box1 (F4, corner (100, 30)), Sphere1 (F5), Torus1 (F6),
# Coil1 (F7), Pulley1 (F8), Gear1 (F9), Bearing1 (F10); Hole1 (F11) by `at`
# on a fixed plane at Z 20 (the block's top), Box2 (F12) by `at` on the
# fixed plane at Z 36 with an x axis. The Windows counterpart is
# tools/ui-windows-fixed-plane-test.ps1 (ctest app.ui-windows-fixed-plane):
#   1. Every feature type Place On places opens for editing (canEdit no
#      longer refuses a fixed plane), its Place On row listing the plane
#      as "Plane at Z 36 mm (fixed)", drawn in the view where it lies.
#   2. Cylinder1: the diameter 12, OK: saved with the same fixed plane and
#      centre, 12 wide (the volume).
#   3. Box1: the height 6, OK: the same fixed plane and corner. Edited
#      again, the block's top face picked in Place On: saved with `at` on
#      that face, the fixed plane gone.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-fixed-plane-test.sh [screenshot.png]

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-fixed-plane.XXXXXX)
FILE=$WORK/fixed.mitcad
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
RECT='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'
Z36='{"origin": [0, 0, 36], "normal": [0, 0, 1]}'

cat > "$WORK/model.json" << EOF
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 60, "height": 40},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$RECT"}],
    "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "cylinder", "plane": $Z36, "center": [100, 0],
    "diameter": 10, "height": 20, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "box", "plane": $Z36, "corner": [100, 30],
    "length": 10, "width": 8, "height": 4, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "sphere", "plane": $Z36, "center": [130, 0],
    "diameter": 10, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "torus", "plane": $Z36, "center": [160, 0],
    "diameter": 20, "section_diameter": 4, "position": "on_center", "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "coil", "plane": $Z36, "center": [200, 0], "diameter": 20,
    "helix": {"type": "revolutions_and_pitch", "revolutions": 2, "pitch": 8}, "angle": 0,
    "clockwise": false, "section": "circular", "section_position": "on_center", "section_size": 2,
    "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "pulley", "kind": "toothed", "profile": "gt2_2mm", "plane": $Z36,
    "center": [0, 80], "teeth": 20, "belt_width": 6, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "gear", "kind": "spur", "plane": $Z36, "center": [0, -80],
    "module": 2, "teeth": 20, "face_width": 10, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "printable_bearing", "size": "608", "plane": $Z36, "center": [60, 80],
    "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "hole", "placement": {"type": "at", "at": {
      "face": {"origin": [0, 0, 20], "normal": [0, 0, 1]}, "edge1": "x", "offset1": 20, "edge2": "y", "offset2": 30,
      "near": [30, 20, 20]}}, "diameter": 6, "extent": {"type": "through_all"}, "participants": ["F2.b0"]}},
  {"cmd": "add_feature", "def": {"type": "box", "length": 5, "width": 5, "height": 5, "operation": "new_body",
    "at": {"face": {"origin": [0, 0, 36], "normal": [0, 0, 1], "x_axis": [1, 0, 0]},
           "edge1": "x", "offset1": 50, "edge2": "y", "offset2": 40, "near": [40, -50, 36]}}}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

ui_start_display
ui_start_app --open "$FILE"
ui_step "fit (F6)"                         ui_key F6
ui_expect_log "Bodies shown: F2.b0" "the design shown"

echo "--- 1. Every placed feature type opens on its fixed plane"
# edit_fixed "Timeline name" uid command plane-text: opened for editing,
# Place On listing the fixed plane; cancelled.
edit_fixed() {
  ui_mark
  ui_step "double-click $1"                ui_double_click_logged "Timeline $1"
  ui_expect_new "Editing $2 with $3" "$1 opens for editing" 30
  ui_expect_new "$3 Place On listed: $4" "$1 on the fixed plane, listed as $4"
  ui_expect_new "Place $3: face $4" "$1 placed on it"
  ui_mark
  ui_step "cancel (Escape)"                ui_key Escape
  ui_expect_new "Command $3 cancelled" "$1's edit cancelled"
}
edit_fixed Cylinder1 F3 Cylinder "Plane at Z 36 mm (fixed)"
edit_fixed Box1 F4 Box "Plane at Z 36 mm (fixed)"
edit_fixed Sphere1 F5 Sphere "Plane at Z 36 mm (fixed)"
edit_fixed Torus1 F6 Torus "Plane at Z 36 mm (fixed)"
edit_fixed Coil1 F7 Coil "Plane at Z 36 mm (fixed)"
edit_fixed Pulley1 F8 Pulley "Plane at Z 36 mm (fixed)"
edit_fixed Gear1 F9 Gear "Plane at Z 36 mm (fixed)"
edit_fixed Bearing1 F10 Bearing "Plane at Z 36 mm (fixed)"
edit_fixed Hole1 F11 Hole "Plane at Z 20 mm (fixed)"
edit_fixed Box2 F12 Box "Plane at Z 36 mm (fixed)"
grep -q "cannot be edited" "$UI_LOG" && ui_fail "a feature on a fixed plane was refused"
echo "ok   none refused"

echo "--- 2. Cylinder1: a new diameter keeps the fixed plane"
ui_mark
ui_step "double-click Cylinder1"           ui_double_click_logged "Timeline Cylinder1"
ui_expect_new "Editing F3 with Cylinder" "Cylinder1 opens for editing" 30
ui_expect_new "Cylinder Place On listed: Plane at Z 36 mm (fixed)" "on the fixed plane"
# The plane drawn as a construction plane is, about its origin (0, 0, 36).
ui_expect_new "View highlight places: " "the highlights' places logged"
for _ in $(seq 1 25); do
  line=$(grep "View highlight places: " "$UI_LOG" | tail -1)
  printf '%s\n' "$line" | tr ';' '\n' | grep -q 'plane fixed/.* at 0\.0,0\.0,36\.0' && break
  sleep 0.2
done
printf '%s\n' "$line" | tr ';' '\n' | grep -q 'plane fixed/.* at 0\.0,0\.0,36\.0' ||
  ui_fail "the fixed plane not drawn at Z 36: $line"
echo "ok   the fixed plane drawn where it lies"
ui_mark
ui_step "diameter 12"                      ui_type_in "Panel Cylinder input diameter" "12"
ui_expect_new "Preview Cylinder: ok" "12 wide previewed" 30
[ -n "$SHOT" ] && ui_capture "$SHOT"
ui_mark
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Edited F3" "Cylinder1 edited" 30
ui_expect_volume "Body [^ ]* (F3\.b0): volume [0-9.]* -> \([0-9.]*\) mm3" \
  "$(awk 'BEGIN { printf "%.6f", 3.14159265358979 * 36 * 20 }')" "12 wide, 20 high"

echo "--- 3. Box1: a new height keeps it, a picked face replaces it"
ui_mark
ui_step "double-click Box1"                ui_double_click_logged "Timeline Box1"
ui_expect_new "Editing F4 with Box" "Box1 opens for editing" 30
ui_expect_new "Box Place On listed: Plane at Z 36 mm (fixed)" "on the fixed plane"
ui_mark
ui_step "height 6"                         ui_type_in "Panel Box input height" "6"
ui_expect_new "Preview Box: ok" "6 high previewed" 30
ui_mark
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Edited F4" "Box1 edited" 30
ui_focus_main
ui_mark
ui_key ctrl+s
ui_expect_new "Saved $FILE" "saved"
# A size typed in a panel is saved as its parameter ("diameter": "d4"):
# its millimetres are the parameter's saved `value`.
SIZE='.parameters as $params | def size($v): if ($v | type) == "string"
  then ($params[] | select(.name == $v) | .value)
  else $v end;'
jq -e "$SIZE"' .features[] | select(.type == "cylinder") |
  .plane.origin == [0, 0, 36] and .plane.normal == [0, 0, 1] and .center == [100, 0] and size(.diameter) == 12 and
  (has("at") | not)' "$FILE" > /dev/null ||
  ui_fail "Cylinder1 not on its fixed plane: $(jq -c '.features[] | select(.type == "cylinder")' "$FILE")"
echo "ok   Cylinder1 saved with its fixed plane and centre, 12 wide"
jq -e "$SIZE"' [.features[] | select(.type == "box")][0] |
  .plane.origin == [0, 0, 36] and .plane.normal == [0, 0, 1] and .corner == [100, 30] and size(.height) == 6 and
  (has("at") | not)' "$FILE" > /dev/null ||
  ui_fail "Box1 not on its fixed plane: $(jq -c '[.features[] | select(.type == "box")][0]' "$FILE")"
echo "ok   Box1 saved with its fixed plane and corner, 6 high"

ui_mark
ui_step "double-click Box1"                ui_double_click_logged "Timeline Box1"
ui_expect_new "Editing F4 with Box" "Box1 opens again" 30
ui_mark
ui_step "Place On"                         ui_click_logged "Panel Box input at"
ui_expect_new "Pick places:" "the picks are logged"
ui_expect_new "Pick face F2.b0/F2:end($RECT) at" "the block's top face can be picked"
ui_mark
ui_step "click the block's top face"       ui_click_pick "face F2.b0/F2:end($RECT) at"
ui_expect_new "Place Box: face face F2:end(" "the top face in place of the fixed plane"
ui_expect_new "Preview Box: ok" "previewed on the block" 30
ui_mark
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Edited F4" "Box1 placed on the block" 30
ui_focus_main
ui_mark
ui_key ctrl+s
ui_expect_new "Saved $FILE" "saved"
jq -e '[.features[] | select(.type == "box")][0] | .at.face.body == "F2.b0" and (has("plane") | not)' "$FILE" \
  > /dev/null || ui_fail "Box1 not on the face: $(jq -c '[.features[] | select(.type == "box")][0]' "$FILE")"
echo "ok   Box1 saved by a reference to the top face, the fixed plane gone"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"

ui_finish "UI fixed plane test"
