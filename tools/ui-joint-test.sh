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
#   - Animate Joint (mitcad#1701): every frame from the joint solver, no
#     preview computed per frame, the whole animation within 20 % of its
#     intended time; also on the reported design of GitHub #256 when
#     MITCAD_MITCAD_REPORTED has it.
#   - Joint Origin on the cap's top face in the cap's component (activated
#     with its radio button), 5 mm up; As-Built Joint of the cap to the
#     plate; Rigid Group of the pin and the cap, picked in the browser.
#   - mitcad#1745: a cylindrical joint whose origin B is on the side that
#     moves (a collar a move places, joined to the grounded plate): the
#     collar goes where its origin meets the plate's at the offset, and an
#     edited Offset moves it.
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

# animation_timed joint: Animate Joint's last "Animated <joint>: 36 frames
# in N ms (intended M ms): S shown, P previews" (mitcad#1701): no frame
# computed as a preview (no recompute or meshing per frame) and the whole
# animation within 20 % of its intended time.
animation_timed() {
  local line
  line=$(grep -F "Animated $1: " "$UI_LOG" | tail -1)
  [[ $line =~ :\ ([0-9]+)\ frames\ in\ ([0-9]+)\ ms\ \(intended\ ([0-9]+)\ ms\):\ ([0-9]+)\ shown,\ ([0-9]+)\ previews$ ]] ||
    ui_fail "Animate $1 logged no frames and time: '$line'"
  local frames=${BASH_REMATCH[1]} took=${BASH_REMATCH[2]} intended=${BASH_REMATCH[3]}
  local shown=${BASH_REMATCH[4]} previews=${BASH_REMATCH[5]}
  [ "$previews" = 0 ] || ui_fail "Animate $1 computed $previews of its frames as previews"
  [ "$shown" -ge 1 ] && [ "$shown" -le "$frames" ] || ui_fail "Animate $1 showed $shown of $frames frames"
  [ $((took * 10)) -ge $((intended * 8)) ] && [ $((took * 10)) -le $((intended * 12)) ] ||
    ui_fail "Animate $1 took $took ms, intended $intended ms"
  echo "ok   Animate $1: $frames frames in $took ms (intended $intended ms), $shown shown, no previews"
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

echo "--- Animate Joint: Joint1 through its limits, the document unchanged"
ui_mark
ui_step "animate (search)"               ui_command "Animate Joint"
ui_expect_new "Animating Joint1 rz: 36 frames from 45 deg to 45 deg in 1440 ms, frames by the joint solver" \
  "from 45 degrees to the limits and back, every frame from the joint solver"
ui_expect_new "Animated Joint1: 36 frames in " "the animation ended" 30
animation_timed "Joint1"
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
# A design longer than the browser opened folded below the root (mitcad#426):
# Cap:1 is opened to show its new Joints folder.
if grep -q "Browser opened folded" "$UI_LOG"; then
  ui_step "scroll the browser to Cap:1"  ui_browser_reveal "Browser Root/Cap:1"
  ui_step "open Cap:1"                   ui_click_logged "Browser arrow Root/Cap:1"
fi
# The tree is taller than the panel by now: the new row may be below the rows in sight.
ui_step "scroll the browser to the new row"  ui_browser_reveal "Browser Root/Cap:1/Joints/JointOrigin1"
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

# mitcad#1745 (GitHub #296): a joint whose origin B is on the side that
# moves. Collar (C4, Collar:1 O4): a cylinder of diameter 20 and height 10,
# placed aside by a move in the timeline, added by mitcad-cli to the saved
# design. A cylindrical joint from the grounded plate's top (A) to the
# collar's top (B), Offset -30 mm: the preview moves the collar so that its
# origin meets the plate's at the offset (not only onto the axis where it
# was nearest), and an edited Offset moves it by the change.
echo "--- Joint: the collar onto the grounded plate, cylindrical"
ui_stop_app
cat > "$WORK/collar.json" << 'EOF'
[
  {"cmd": "activate_component", "component": "Root"},
  {"cmd": "create_component", "name": "Collar"},
  {"cmd": "add_feature", "def": {"type": "cylinder", "plane": "xy", "center": [0, 0],
    "diameter": 20, "height": 10, "operation": "new_body"}},
  {"cmd": "activate_component", "component": "Root"},
  {"cmd": "add_feature", "def": {"type": "move_occurrence", "occurrences": ["O4"],
    "transform": {"type": "translate_xyz", "x": 120, "y": 0, "z": 0}}},
  {"expect": {"occurrence": "Collar:1", "body": "Body4", "center": [120, 0, 5], "tolerance": 1e-6}}
]
EOF
"$CLI" run "$WORK/collar.json" --open "$FILE" --save "$WORK/collar.mitcad" > "$WORK/collar.log" 2>&1 ||
  { cat "$WORK/collar.log"; ui_fail "the collar added by mitcad-cli"; }
FILE=$WORK/collar.mitcad
COLLAR=$(python3 -I -c '
import json, sys
design = json.load(open(sys.argv[1], encoding="utf-8"))
component = [c["uid"] for c in design["components"] if c["name"] == "Collar"][0]
print([f["uid"] for f in design["features"] if f.get("component") == component and f["type"] == "cylinder"][0])
' "$FILE")
[ -n "$COLLAR" ] || ui_fail "the collar's cylinder"
UI_WINDOW_WAIT=60 ui_start_app --open "$FILE"
for _ in $(seq 1 300); do
  grep -qF -- "Opened $FILE" "$UI_LOG" && break
  sleep 0.2
done
ui_expect_log "Opened $FILE" "the design with the collar opened"
ui_wait_idle
ui_step "fit (F6)"                       ui_key F6
ui_mark
ui_step "joint (J)"                      ui_key j
ui_expect_new "Command Joint started" "Joint started"
ui_step "pick the plate's top face"      ui_click_pick "face F2.b0/F2:end("
ui_expect_log "Joint Origin A: 1 face [face F2:end($RECT) of F2.b0 in O1]" "origin A on the grounded plate"
ui_mark
ui_step "pick the collar's top face"     ui_click_pick "face $COLLAR.b0/$COLLAR:top"
ui_expect_log "Joint Origin B: 1 face [face $COLLAR:top of $COLLAR.b0 in O4]" "origin B on the collar"
ui_expect_new "Preview Joint moves O4" "the collar is the side that moves"
ui_step "cylindrical"                    ui_choose "Panel Joint input kind" 3
ui_expect_log "Joint: Type = Cylindrical" "cylindrical"
ui_mark
ui_step "offset -30"                     ui_type_in "Panel Joint input offset" "-30"
ui_expect_new "Preview Joint moves O4" "the preview moves the collar to the offset"
ui_expect_log "Manipulator Joint offset at" "the offset's arrow"
# The preview with the handles on the plate's top, the arrow reaching the
# collar's top (beside the screenshot asked for, "-collar").
[ -n "$SHOT" ] && ui_capture "${SHOT%.png}-collar.png"
ui_mark
ui_step "OK (Enter)"                     ui_key Return
ui_expect_new "Added joint Joint2 (cylindrical): placed" "the collar placed by the joint"
save
check "the collar's top 30 mm above the plate's top, on its middle" \
  '{"expect": {"occurrence": "Collar:1", "body": "Body4", "center": [20, 20, 35], "tolerance": 1e-6}}' \
  '{"expect": {"query": {"query": "joints"}, "result": {"joints": [{"name": "Joint1"}, {"name": "AsBuiltJoint1"}, {"name": "Joint2", "kind": "cylindrical", "state": "placed", "values": {"tz": 0, "rz": 0}}]}}}'
ui_mark
ui_step "double-click Joint2"            ui_double_click_logged "Timeline Joint2"
ui_expect_new "Editing $(grep -o 'Added Joint as F[0-9]*' "$UI_LOG" | tail -1 | sed 's/Added Joint as //') with Joint" "editing Joint2"
ui_step "offset -50"                     ui_type_in "Panel Joint input offset" "-50"
ui_mark
ui_step "OK (Enter)"                     ui_key Return
ui_expect_new "Edited F" "Joint2 edited"
save
check "a new offset moves the collar by its change" \
  '{"expect": {"occurrence": "Collar:1", "body": "Body4", "center": [20, 20, 55], "tolerance": 1e-6}}'
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"

# What holds the sides, in words and painted (mitcad#1745): the collar
# grounded too, a new joint of the plate's top on the collar's top cannot
# move either; the panel says so with the components' names, and the
# pointer over the message paints both occurrences.
echo "--- Joint: neither side can move"
ui_mark
ui_step "right-click Collar:1"           ui_click_logged "Browser Root/Collar:1" 3
ui_step "choose Ground"                  ui_menu_choose "Ground"
ui_expect_new "Ground Collar:1" "the collar grounded"
ui_mark
ui_step "joint (J)"                      ui_key j
ui_expect_new "Command Joint started" "Joint started"
ui_step "pick the plate's top face"      ui_click_pick "face F2.b0/F2:end("
ui_expect_log "Joint Origin A: 1 face [face F2:end($RECT) of F2.b0 in O1]" "origin A on the plate"
ui_step "pick the collar's top face"     ui_click_pick "face $COLLAR.b0/$COLLAR:top"
ui_expect_log "Joint Origin B: 1 face [face $COLLAR:top of $COLLAR.b0 in O4]" "origin B on the collar"
ui_expect_log "neither side can move (Plate:1 is grounded, and Collar:1 is grounded) and the joint does not hold" \
  "the panel says what holds each side"
grep -E "^Panel Joint message box: .*(O[0-9]|F[0-9]+\.b|E[{]|F[0-9]+:)" "$UI_LOG" &&
  ui_fail "the panel's message names ids"
ui_expect_log "Panel Joint message at" "the message is shown"
ui_mark
ui_step "point at the message"           xdotool mousemove $(ui_logged_at "Panel Joint message")
ui_expect_new "Panel Joint message hovered: 2 places" "the message has the two occurrences' places"
ui_expect_new "Notice hover highlighted: panel: occurrence O1, occurrence O4 (2 drawn)" \
  "the pointer over it paints the plate and the collar"
[ -n "$SHOT" ] && ui_capture "${SHOT%.png}-held.png"
ui_mark
ui_step "cancel (Esc)"                   ui_key Escape
ui_expect_new "Command Joint cancelled" "Joint cancelled"

# The reported design of GitHub #256 (mitcad#1701; under
# MITCAD_MITCAD_REPORTED, never committed): a small mechanism whose Animate
# was slow; its first driven revolute joint animated from the browser's
# joint menu, every frame from the joint solver and on time.
REPORTED_ZIP=""
if [ -n "${MITCAD_MITCAD_REPORTED:-}" ]; then
  REPORTED_ZIP=$(find "$MITCAD_MITCAD_REPORTED/github-256" -maxdepth 1 -name '*.zip' 2> /dev/null | sort | head -1)
fi
if [ -n "$REPORTED_ZIP" ] && command -v unzip > /dev/null && command -v python3 > /dev/null; then
  echo "--- The reported design of GitHub #256: Animate Joint on its driven revolute joint"
  mkdir -p "$WORK/reported"
  unzip -o -q "$REPORTED_ZIP" -d "$WORK/reported" || ui_fail "the reported design's zip"
  REPORTED=$(find "$WORK/reported" -name '*.mitcad' | sort | head -1)
  [ -n "$REPORTED" ] || ui_fail "no design in the reported zip"
  # The first revolute joint (or as-built joint) driven by a position.
  JOINT=$(python3 -I -c '
import json, sys
design = json.load(open(sys.argv[1], encoding="utf-8"))
for f in design.get("features", []):
    if f.get("type") in ("joint", "as_built_joint") and f.get("kind") == "revolute" and "rz" in (f.get("position") or {}):
        print(f["name"])
        break
' "$REPORTED")
  [ -n "$JOINT" ] || ui_fail "the reported design has no driven revolute joint"
  ui_stop_app
  UI_WINDOW_WAIT=180 ui_start_app --open "$REPORTED"
  for _ in $(seq 1 900); do
    grep -qF -- "Opened $REPORTED" "$UI_LOG" && break
    sleep 0.2
  done
  ui_expect_log "Opened $REPORTED" "the reported design opened"
  ui_wait_idle
  ui_sync
  # A design longer than the browser opens folded below the root (mitcad#426).
  ui_step "scroll the browser to the joint"  ui_browser_reveal "Browser Root/Joints/$JOINT"
  if ! ui_place_valid "Browser Root/Joints/$JOINT"; then
    ui_browser_reveal "Browser arrow Root/Joints"
    ui_step "open the Joints folder"     ui_click_logged "Browser arrow Root/Joints"
    ui_step "scroll the browser to the joint"  ui_browser_reveal "Browser Root/Joints/$JOINT"
  fi
  ui_expect_log "Browser Root/Joints/$JOINT at" "the joint's row in the browser"
  ui_mark
  ui_step "joint's menu (right click)"   ui_click_logged "Browser Root/Joints/$JOINT" 3
  ui_expect_new "Context menu: Edit $JOINT | Drive $JOINT | Animate $JOINT" "the joint's menu offers Animate"
  ui_step "animate"                      ui_menu_choose "Animate $JOINT"
  ui_expect_new "Animating $JOINT rz: 36 frames from " "the animation starts"
  ui_expect_new "in 1440 ms, frames by the joint solver" "every frame from the joint solver"
  ui_expect_new "Animated $JOINT: 36 frames in " "the animation ended" 60
  animation_timed "$JOINT"
else
  echo "skip the reported design of GitHub #256 (MITCAD_MITCAD_REPORTED, unzip, python3)"
fi

ui_finish "UI joint test"
