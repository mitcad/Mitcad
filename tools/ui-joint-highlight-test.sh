#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindowJointHighlight.cpp app/framework/JointHighlight.cpp app/framework/JointHighlight.hpp
# check-all sources: app/MainWindow.cpp app/OcctViewer.cpp app/browser core/model-api/src/joints.rs
# check-all sources: core/model/src/build core/model-build core/vcs/src/library tools/cli
# The joint highlight (mitcad#523) through the real UI. A design mitcad-cli
# makes: Plate:1 (a 40 x 40 x 10 block, grounded), Pin:1 (a 10 x 10 x 20
# block) and Shaft:1 (a cylinder of diameter 6); Joint1 puts the pin's
# bottom face on the plate's top face (revolute, flipped), Joint2 the
# shaft's bottom circular edge on the plate's top face with an override
# that moves the frame to (5, 5, 10):
#   - Joint1's row in the browser: the log names side a's face of the pin
#     and side b's face of the plate, both frames where the faces meet, the
#     two occurrences; the card names it.
#   - Joint2's row replaces it: side a the plate's face, its frame only
#     (the override), side b the shaft's circular edge; nothing of Joint1
#     is left.
#   - Joint1's cell in the timeline highlights it as its row does; a click
#     on nothing in the view clears it.
#   - The pointer over Joint2's row previews it; off the rows, nothing.
# Then a BUILD joint, in a design with a small library of synthetic parts
# made here (mitcad-cli: a 300 x 200 timber member; no supplier's files),
# fetched from a local git repository: a post on the ground and a member
# on its side. Its joint's row: both sides are mount frames, each shown
# with its frame and its part.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-joint-highlight-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

ROOT=$(cd "$(dirname "$0")/.." && pwd)
CLI=${UI_CLI:-$(dirname "$UI_APP")/../tools/cli/mitcad-cli}
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-joint-highlight.XXXXXX")
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
export MITCAD_LIBRARIES_DIR=$WORK/cache
LIB=$WORK/test-timber
FILE=$WORK/joints.mitcad
TIMBER=$WORK/timber.mitcad
g() { git -C "$1" -c user.name=T -c user.email=t@example.invalid "${@:2}" > /dev/null; }
RECT='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'

# Plate (C1, Plate:1 O1): Sketch1 F1, Extrude1 F2 (Body1); Pin (C2, Pin:1
# O2): Sketch2 F3, Extrude2 F4 (Body2); Shaft (C3, Shaft:1 O3): Cylinder1
# F5 (Body3); Joint1 F6, Joint2 F7.
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
  {"cmd": "create_component", "name": "Shaft", "transform": {"translation": [-50, 0, 0]}},
  {"cmd": "add_feature", "def": {"type": "cylinder", "plane": "xy", "center": [0, 0],
    "diameter": 6, "height": 30, "operation": "new_body"}},
  {"cmd": "activate_component", "component": "Root"},
  {"cmd": "ground_occurrence", "occurrence": "Plate:1"},
  {"cmd": "add_joint", "kind": "revolute", "flip": true,
    "a": {"occurrence": "Pin:1", "geometry": {"body": "F4.b0", "face": "F4:start(RECT)"}},
    "b": {"occurrence": "Plate:1", "geometry": {"body": "F2.b0", "face": "F2:end(RECT)"}}},
  {"cmd": "add_joint", "kind": "rigid",
    "a": {"occurrence": "Plate:1", "geometry": {"body": "F2.b0", "face": "F2:end(RECT)"},
          "frame_override": {"origin": [5, 5, 10]}},
    "b": {"occurrence": "Shaft:1", "geometry": {"body": "F5.b0", "edge": "E{F5:bottom|F5:side0}"}}},
  {"expect": {"query": {"query": "joints"}, "result": {"joints": [
    {"name": "Joint1", "state": "placed", "a": {"on": "face", "path": ["O2"]}, "b": {"on": "face"}},
    {"name": "Joint2", "state": "placed", "a": {"on": "face"},
     "b": {"on": "edge", "component": "C3"}}]}}}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli joints"; }

# A timber member 300 wide (along X) and 200 thick (along Y), centred on
# the Z axis, extruded by L from z = 0, with its end frames and a face
# frame on each side (docs/libraries.md, "Timber parts").
frame() {
  printf '  {"cmd": "add_feature", "name": "mount:%s", "def": {"type": "joint_origin", "geometry": "origin",
    "frame_override": {"origin": %s, "z_axis": %s, "x_axis": %s}}},\n' "$1" "$2" "$3" "$4"
}
{ printf '[\n  {"cmd": "add_parameter", "name": "L", "expression": "600 mm"},\n'
  printf '  {"cmd": "sketch.create", "plane": "xy"},\n'
  printf '  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [-150, -100], "width": 300, "height": 200},\n'
  printf '  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "%s"}],
    "extent": {"type": "distance", "distance": "L"}, "operation": "new_body"}},\n' "$RECT"
  frame end_a "[0, 0, 0]" "[0, 0, -1]" "[1, 0, 0]"
  printf '  {"cmd": "add_feature", "name": "mount:end_b", "def": {"type": "joint_origin",
    "geometry": {"body": "F2.b0", "face": "F2:end(%s)"}}},\n' "$RECT"
  frame face_xp "[150, 0, 0]" "[1, 0, 0]" "[0, 0, 1]"
  frame face_xn "[-150, 0, 0]" "[-1, 0, 0]" "[0, 0, 1]"
  frame face_yp "[0, 100, 0]" "[0, 1, 0]" "[0, 0, 1]"
  frame face_yn "[0, -100, 0]" "[0, -1, 0]" "[0, 0, 1]"
  printf '  {"expect": {"bodies": 1}}\n]\n'; } > "$WORK/member.json"
"$CLI" library init "$LIB" --id test-timber --name "Test timber" --license MIT > /dev/null || ui_fail "library init"
mkdir -p "$LIB/dimensioned-timber"
"$CLI" run "$WORK/member.json" --save "$LIB/dimensioned-timber/timber-200-test.mitcad" > /dev/null ||
  ui_fail "mitcad-cli run member.json failed"
cat > "$LIB/mitcad-library.json" <<'EOF'
{
  "format": "mitcad-library", "version": 1, "id": "test-timber", "name": "Test timber",
  "description": "A synthetic BUILD part of the joint highlight UI test.", "library_version": "1.0.0", "license": "MIT",
  "authors": ["Mitcad UI test"], "units": "mm",
  "categories": [{"id": "dimensioned-timber", "name": "Dimensioned timber"}],
  "components": [
    {"id": "timber-200-test", "path": "dimensioned-timber/timber-200-test.mitcad", "name": "Timber 200x300 test",
     "category": "dimensioned-timber", "designation": "Timber 200x300 test L={L}",
     "build": {"role": "profile", "kind": "timber",
       "material": {"species": "spruce", "finish": "dimensioned", "grade": "", "density": 450, "moisture": 20},
       "availability": "standard",
       "section": {"width": "300 mm", "height": "200 mm"}, "nominal": {"width": "300 mm", "height": "200 mm"},
       "length": {"parameter": "L", "min": "100 mm", "max": "6000 mm", "default": "600 mm",
                  "stock": ["3000 mm", "3600 mm", "4200 mm"]},
       "frames": [{"name": "end_a", "kind": "end"}, {"name": "end_b", "kind": "end"},
                  {"name": "face_xp", "kind": "face"}, {"name": "face_xn", "kind": "face"},
                  {"name": "face_yp", "kind": "face"}, {"name": "face_yn", "kind": "face"}]}}
  ]
}
EOF
g "$LIB" add -A
g "$LIB" commit -q -m "Test part"
g "$LIB" tag v1.0.0
"$CLI" library fetch "$LIB" > /dev/null || ui_fail "library fetch"

# The post (O1, its derived part F1) on the ground at the origin, 1200
# long; the member (O2, F2) on the post's +X side 300 up, 600 long; its
# joint Joint1 (F3) in the root.
cat > "$WORK/timber.json" << EOF
[
  {"cmd": "build_place", "part": {"library": "test-timber", "url": "$LIB", "rev": "HEAD", "component": "timber-200-test"},
   "at": {"point": [0, 0, 0]}, "length": 1200},
  {"cmd": "build_place", "part": {"source": "S1"},
   "at": {"occurrence": "O1", "geometry": {"mount": "F1:mount(face_xp)"}, "offset": 300}, "length": 600},
  {"expect": {"query": {"query": "joints"}, "result": {"joints": [
    {"name": "Joint1", "a": {"on": "mount", "path": ["O2"]}, "b": {"on": "mount", "path": ["O1"]}}]}}}
]
EOF
"$CLI" run "$WORK/timber.json" --save "$TIMBER" > "$WORK/timber.log" 2>&1 ||
  { cat "$WORK/timber.log"; ui_fail "mitcad-cli timber"; }

# expect_side SIDE TEXT END DESCRIPTION: the last "Joint highlight SIDE: "
# line since the mark starts with TEXT and ends with END.
expect_side() {
  local line
  ui_expect_new "Joint highlight $1: $2" "$4"
  line=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -F "Joint highlight $1: " | tail -1)
  [[ $line == *"$3" ]] || ui_fail "$4: '$line' does not end with '$3'"
}

# open_joints: a design taller than the browser opens folded below the
# root (mitcad#426), its Joints folder closed: opened, so that the joints'
# rows are in sight (ui_logged_at scrolls to them).
open_joints() {
  if grep -q "Browser opened folded" "$UI_LOG"; then
    ui_step "open the Joints folder"       ui_click_logged "Browser arrow Root/Joints"
  fi
}

ui_start_display
ui_start_app --open "$FILE"
ui_step "fit (F6)"                         ui_key F6
ui_expect_log "Browser joints: Joint1 placed, Joint2 placed" "the browser has both joints"
open_joints

echo "--- Joint1 from its row in the browser: a face on a face"
ui_mark
ui_step "click Joint1's row"               ui_click_logged "Browser Root/Joints/Joint1"
ui_expect_new "Selected: 1 feature [feature F6]" "Joint1 is selected"
ui_expect_new "Joint highlight a: face F4:start($RECT) of F4.b0 in Pin:1 (O2) [Joint1]" \
  "side a: the pin's bottom face, in the pin"
ui_expect_new "Joint highlight b: face F2:end($RECT) of F2.b0 in Plate:1 (O1) [Joint1]" \
  "side b: the plate's top face, in the plate"
ui_expect_new "Joint highlight frame a at 20.0,20.0,10.0 [Joint1]" "side a's frame where the faces meet"
ui_expect_new "Joint highlight frame b at 20.0,20.0,10.0 [Joint1]" "side b's frame there too"
ui_expect_new "Joint highlight occurrences: Pin:1 (O2), Plate:1 (O1) [Joint1]" "the two occurrences it joins"
ui_expect_new "Joint card: Joint1: revolute, a = Pin:1, face of Body2" "the card names the joint and side a in words"
ui_expect_new ", b = Plate:1, face of Body1" "and side b"

echo "--- Joint2 replaces it: a frame override and a circular edge"
ui_mark
ui_step "click Joint2's row"               ui_click_logged "Browser Root/Joints/Joint2"
ui_expect_new "Selected: 1 feature [feature F7]" "Joint2 is selected"
ui_expect_new "Joint highlight a: face F2:end($RECT) of F2.b0 in Plate:1 (O1), frame only [Joint2]" \
  "side a: the plate's face moved by an override shows its frame only"
ui_expect_new "Joint highlight b: edge E{F5:bottom|F5:side0} of F5.b0 in Shaft:1 (O3) [Joint2]" \
  "side b: the shaft's circular edge"
ui_expect_new "Joint highlight frame a at 5.0,5.0,10.0 [Joint2]" "the override's frame"
ui_expect_new "Joint highlight occurrences: Plate:1 (O1), Shaft:1 (O3) [Joint2]" "the plate and the shaft"
ui_expect_new "(its frame)" "the card says side a is its frame"
if tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -q "^Joint highlight .*\[Joint1\]$"; then
  ui_fail "Joint1 is still highlighted after Joint2 was selected"
fi
echo "ok   nothing of Joint1 is highlighted"

echo "--- Joint1 from the timeline, then cleared"
ui_mark
ui_step "click Joint1 in the timeline"     ui_click_logged "Timeline Joint1"
ui_expect_new "Joint highlight a: face F4:start($RECT) of F4.b0 in Pin:1 (O2) [Joint1]" \
  "the timeline's cell highlights it as its row does"
ui_mark
ui_step "click on nothing in the view"     ui_view_click 3 50
ui_expect_new "Joint highlight: none" "deselecting clears the highlight"

echo "--- The pointer over Joint2's row previews it"
ui_mark
ui_step "point at Joint2's row"            xdotool mousemove $(ui_logged_at "Browser Root/Joints/Joint2")
ui_expect_new "Joint hover preview: Joint2" "the row under the pointer is previewed"
if tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -q "^Selected: "; then
  ui_fail "the hover selected something"
fi
ui_mark
ui_step "point at the view"                xdotool mousemove $(ui_view_at 3 50)
ui_expect_new "Joint hover preview: none" "the preview goes with the pointer"
ui_stop_app

echo "--- A BUILD joint: a member on a post's side"
ui_start_app --open "$TIMBER"
ui_step "fit (F6)"                         ui_key F6
ui_expect_log "Browser joints: Joint1 " "the browser has the member's joint"
open_joints
ui_mark
ui_step "click the member's joint"         ui_click_logged "Browser Root/Joints/Joint1"
expect_side b "mount frame face_xp of F1 in " " (O1), frame and part [Joint1]" \
  "side b: the post's side frame, with the post"
expect_side a "mount frame " " (O2), frame and part [Joint1]" "side a: the member's own frame, with the member"
ui_expect_new "Joint highlight frame a at " "side a's frame is drawn"
ui_expect_new "Joint highlight frame b at " "side b's frame is drawn"
occurrences=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -F "Joint highlight occurrences: " | tail -1)
[[ $occurrences == *" (O2), "*" (O1) [Joint1]" ]] || ui_fail "the joined parts: $occurrences"
echo "ok   the two parts: $occurrences"
ui_expect_new "Joint card: Joint1: " "the card names the BUILD joint"

ui_stop_app
ui_finish "the joint highlight"
