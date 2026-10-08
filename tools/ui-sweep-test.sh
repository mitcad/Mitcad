#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Sweep, Pipe, Loft, Coil, Rib and Web (U4) through the real UI, with the
# volumes the model reports (tools/cli/tests/f3_*.json):
#   - Sweep: a 10 mm circle on YZ along a 50 mm line: a cylinder; edited
#     to half the path.
#   - Pipe: hollow, 10 mm with a 1 mm wall along a 50 mm line.
#   - Loft: a 40 mm square up to a 20 mm one 30 mm above: a frustum; again
#     with a Direction start condition (the squares' sketches, hidden since
#     the first loft uses them, shown with their light bulbs), edited to
#     weight 2; and through a rail along one corner.
#   - Coil: 3 turns of pitch 10 round a 40 mm diameter, placed by a click.
#   - Rib and Web on the bracket of ui_rib.
#   - Helix (mitcad#27): a 2 mm square turned about Z, its volume by
#     Pappus' rule (the turns times 2 pi times the square's distance from
#     the axis times its area): one made as the FreeCAD import writes a
#     helix of a height and turns (a pitch "h / t") opens with them and
#     takes other turns; a new one, 3 turns of 10, edited to a height of 25
#     at pitch 5, left-handed, narrowing 2 a turn (mitcad#83: Mitcad's
#     construction, so the square's mean distance from the axis counts),
#     opens again as that; a helix that the FreeCAD import made widening
#     keeps FreeCAD's construction when it is edited.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sweep-test.sh [screenshot.png]

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-sweep.XXXXXX)
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
RECT='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'
# expect_file file 'python expression of doc' expected what
expect_file() {
  local actual
  actual=$(python3 -c 'import json, sys; doc = json.load(open(sys.argv[1])); print(eval(sys.argv[2]))' \
    "$1" "$2") || ui_fail "$4: cannot read $1"
  [ "$actual" = "$3" ] || ui_fail "$4: $2 is '$actual', expected '$3'"
  echo "ok   $4"
}
PI=3.14159265358979

# Sketch1 (F1): the path, (0, 0)-(50, 0); Sketch2 (F2) on YZ: a 10 mm
# circle; Sketch3 (F3): the pipe's path (0, 100)-(50, 100); Sketch4 (F4): a
# 40 mm square at (100, 0); Plane1 (F5) 30 mm up; Sketch5 (F6) on it: a 20
# mm square at (110, 10); Plane2 (F7) through the squares' corners (100, 0)
# below and (110, 10) above; Sketch6 (F8) on it: the line between them, a
# rail.
cat > "$WORK/sweeps.json" << EOF
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_line", "sketch": "F1", "start": [0, 0], "end": [50, 0]},
  {"cmd": "sketch.create", "plane": "yz"},
  {"cmd": "sketch.add_circle", "sketch": "F2", "center": [0, 0], "diameter": 10},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_line", "sketch": "F3", "start": [0, 100], "end": [50, 100]},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F4", "corner": [100, 0], "width": 40, "height": 40},
  {"cmd": "add_feature", "def": {"type": "construction_plane", "definition": {"type": "offset", "plane": "xy", "distance": 30}}},
  {"cmd": "sketch.create", "plane": "F5"},
  {"cmd": "sketch.add_rectangle", "sketch": "F6", "corner": [110, 10], "width": 20, "height": 20},
  {"cmd": "add_feature", "def": {"type": "construction_plane", "definition": {"type": "fixed",
    "origin": [100, 0, 0], "x_axis": [10, 10, 30], "y_axis": [0, 0, 1]}}},
  {"cmd": "sketch.create", "plane": "F7"},
  {"cmd": "sketch.add_line", "sketch": "F8", "start": [0, 0], "end": [33.166247903554, 0]}
]
EOF
# The bracket of ui_rib (tools/cli/tests/f3_rib_web.json): a 60 x 40 x 5
# base and a 5 x 40 x 40 plate, 20000 mm3 (Body1 F2.b0); Plane1 (F5) y = 20
# with the rib's line (Sketch3 F6, c1); Plane2 (F7) z = 5 with the webs'
# lines (Sketch4 F8, c1 and c4).
cat > "$WORK/bracket.json" << EOF
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 60, "height": 40},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$RECT"}],
    "extent": {"type": "distance", "distance": 5}, "operation": "new_body"}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F3", "corner": [0, 0], "width": 5, "height": 40},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F3", "region": "$RECT"}],
    "start": {"type": "offset", "offset": 5}, "extent": {"type": "distance", "distance": 40},
    "operation": "join", "participants": ["F2.b0"]}},
  {"cmd": "add_feature", "def": {"type": "construction_plane", "definition": {"type": "offset", "plane": "xz", "distance": 20}}},
  {"cmd": "sketch.create", "plane": "F5"},
  {"cmd": "sketch.add_line", "sketch": "F6", "start": [5, -35], "end": [35, -5]},
  {"cmd": "add_feature", "def": {"type": "construction_plane", "definition": {"type": "offset", "plane": "xy", "distance": 5}}},
  {"cmd": "sketch.create", "plane": "F7"},
  {"cmd": "sketch.add_line", "sketch": "F8", "start": [15, 32], "end": [55, 32]},
  {"cmd": "sketch.add_line", "sketch": "F8", "start": [45, 25], "end": [45, 39]}
]
EOF
# Two 2 mm squares on XZ, 11 and 31 mm from the Z axis (Sketch1 F1,
# Sketch2 F3); the first turned about Z as a helix of the parameters h (its
# height) and t (its turns) (Helix1 F2).
cat > "$WORK/helix.json" << EOF
[
  {"cmd": "add_parameter", "name": "h", "expression": "20 mm", "unit": "mm"},
  {"cmd": "add_parameter", "name": "t", "expression": "4", "unit": ""},
  {"cmd": "sketch.create", "plane": "xz"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [10, 0], "width": 2, "height": 2},
  {"cmd": "add_feature", "def": {"type": "helix", "profiles": [{"sketch": "F1", "region": "$RECT"}],
    "axis": "z", "pitch": "h / t", "revolutions": "t", "operation": "new_body"}},
  {"cmd": "sketch.create", "plane": "xz"},
  {"cmd": "sketch.add_rectangle", "sketch": "F3", "corner": [-32, 0], "width": 2, "height": 2}
]
EOF
# A 2 mm square on XZ 50 mm from the Z axis turned 3 turns of 5 about Z,
# widening 1 a turn, built as FreeCAD builds it (as the FreeCAD import
# writes it).
cat > "$WORK/freecad.json" << EOF
[
  {"cmd": "sketch.create", "plane": "xz"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [50, 0], "width": 2, "height": 2},
  {"cmd": "add_feature", "def": {"type": "helix", "profiles": [{"sketch": "F1", "region": "$RECT"}],
    "axis": "z", "pitch": 5, "revolutions": 3, "growth": 1, "construction": "freecad",
    "operation": "new_body"}}
]
EOF
for model in sweeps bracket helix freecad; do
  "$CLI" run "$WORK/$model.json" --save "$WORK/$model.mitcad" > "$WORK/cli.log" 2>&1 ||
    { cat "$WORK/cli.log"; ui_fail "mitcad-cli $model"; }
done

ui_start_display

echo "=== Sweep, Pipe, Loft, Coil"
ui_start_app --open "$WORK/sweeps.mitcad"
ui_step "fit (F6)"                         ui_key F6

echo "--- Sweep: the circle along the line"
ui_step "sweep (search)"                   ui_command "Sweep"
ui_step "the profiles input"               ui_click_logged "Panel Sweep input profiles"
ui_step "take the newest profile away"     ui_click_pick "profile F6/"
ui_step "pick the circle"                  ui_click_pick "profile F2/"
ui_expect_log "Sweep Profiles: 1 profile [profile r{c1} of F2]" "circle picked"
ui_step "the path input"                   ui_click_logged "Panel Sweep input path"
ui_step "pick the line"                    ui_click_logged "Sketch entity F1/c1"
ui_expect_log "Sweep Path: 1 sketch curve [sketch curve c1 of F1]" "path picked"
ui_expect_log "Preview Sweep: ok" "previewed"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_log "Added sweep (New Body) along 1 curve(s)" "sweep added"
ui_expect_volume "New body Body1 (F9.b0): volume \([0-9.]*\) mm3" \
  "$(awk -v pi=$PI 'BEGIN { printf "%.6f", pi * 25 * 50 }')" "cylinder" 1e-4

echo "--- Sweep1 edited to half the path"
ui_step "double-click Sweep1"              ui_double_click_logged "Timeline Sweep1"
ui_expect_log "Editing F9 with Sweep" "editing Sweep1"
ui_step "Partial"                          ui_choose "Panel Sweep input extent" 1
ui_step "half"                             ui_type_in "Panel Sweep input fraction" "0.5"
ui_expect_log "Sweep Distance 1: 0.5 = 0.5" "half of the path"
ui_expect_log "Preview Sweep: ok" "previewed"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_log "Edited F9" "Sweep1 edited"
ui_expect_volume "Body Body1 (F9.b0): volume [0-9.]* -> \([0-9.]*\) mm3" \
  "$(awk -v pi=$PI 'BEGIN { printf "%.6f", pi * 25 * 25 }')" "half the cylinder" 1e-4

echo "--- Pipe: hollow along the second line"
ui_step "pipe (search)"                    ui_command "Pipe"
ui_step "pick the line"                    ui_click_logged "Sketch entity F3/c1"
ui_expect_log "Pipe Path: 1 sketch curve [sketch curve c1 of F3]" "path picked"
ui_step "size 10"                          ui_type_in "Panel Pipe input size" "10"
ui_step "hollow"                           ui_click_logged "Panel Pipe input hollow"
ui_expect_log "Pipe: Hollow = on" "hollow"
ui_expect_log "Preview Pipe: ok" "previewed"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_volume "New body Body2 (F10.b0): volume \([0-9.]*\) mm3" 1413.7166941154069 "hollow pipe"

echo "--- Loft: the two squares in order"
ui_step "loft (search)"                    ui_command "Loft"
ui_step "the lower square"                 ui_click_pick "profile F4/"
ui_step "the upper square"                 ui_click_pick "profile F6/"
ui_expect_log "Loft Profiles: 2 profiles [profile $RECT of F4; profile $RECT of F6]" "two sections in order"
ui_expect_log "Preview Loft: ok" "previewed"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_log "Added loft (New Body) through 2 sections" "loft added"
# loft_two_squares: 30 / 3 (40^2 + 20^2 + 40 * 20).
ui_expect_volume "New body Body3 (F11.b0): volume \([0-9.]*\) mm3" 28000 "frustum"

echo "--- Loft2: the squares taking off along the lower one's normal"
# Loft1 uses the squares' sketches, so they are hidden (mitcad#7): their
# light bulbs show them again, for this loft and the next.
ui_mark
ui_step "show Sketch4 (Browser)"           ui_click_logged "Browser eye Root/Sketches/Sketch4"
ui_expect_new "Visibility Root/Sketches/Sketch4: shown" "the lower square's sketch shown"
ui_step "show Sketch5 (Browser)"           ui_click_logged "Browser eye Root/Sketches/Sketch5"
ui_expect_new "Visibility Root/Sketches/Sketch5: shown" "the upper square's sketch shown"
ui_step "loft (search)"                    ui_command "Loft"
ui_step "the lower square"                 ui_click_pick "profile F4/"
ui_step "the upper square"                 ui_click_pick "profile F6/"
ui_step "Direction at the start"           ui_choose "Panel Loft input start_condition" 1
ui_expect_log "Loft: Start Condition = Direction" "direction chosen"
ui_expect_log "Panel Loft input start_weight at" "its weight shown"
ui_expect_log "Preview Loft: ok" "previewed"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_log "Added loft (New Body) through 2 sections" "loft added"
# The half width 20 - 10 t^2 over the 30 mm: 4 * 8600.
ui_expect_volume "New body Body4 (F12.b0): volume \([0-9.]*\) mm3" 34400 "taking off along the normal" 1e-5

echo "--- Loft2 edited: its condition loads; weight 2"
ui_step "double-click Loft2"               ui_double_click_logged "Timeline Loft2"
ui_expect_log "Editing F12 with Loft" "editing Loft2"
ui_expect_log "Panel Loft choices: start_condition=direction" "the condition loaded"
ui_step "weight 2"                         ui_type_in "Panel Loft input start_weight" "2"
ui_expect_log "Loft Takeoff Weight: 2 = 2" "weight 2"
ui_expect_log "Preview Loft: ok" "previewed"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_log "Edited F12" "Loft2 edited"
# z'(0) = 60 and z'(1) = |c| / 2 at the corners, |c| = sqrt(1100) (Mitcad's
# rule above weight 1), the same half width:
# 40800 - 1280 sqrt(1100) / 21.
ui_expect_volume "Body Body4 (F12.b0): volume [0-9.]* -> \([0-9.]*\) mm3" 38778.43822302147 "a longer takeoff" 1e-5

echo "--- Loft3: the squares through a rail along a corner"
ui_step "loft (search)"                    ui_command "Loft"
ui_step "the lower square"                 ui_click_pick "profile F4/"
ui_step "the upper square"                 ui_click_pick "profile F6/"
ui_step "Rails"                            ui_choose "Panel Loft input guide" 1
ui_expect_log "Panel Loft input rails.0.path at" "a rail row shown"
ui_step "the rail input"                   ui_click_logged "Panel Loft input rails.0.path"
ui_step "pick the corner line"             ui_click_logged "Sketch entity F8/c1"
ui_expect_log "Loft Rail: 1 sketch curve [sketch curve c1 of F8]" "rail picked"
ui_expect_log "Preview Loft: ok" "previewed"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_volume "New body Body5 (F13.b0): volume \([0-9.]*\) mm3" 28000 "the frustum along its edge" 1e-5

echo "--- Coil: placed with a click on XY"
ui_step "coil (search)"                    ui_command "Coil"
ui_step "click XY"                         ui_click_logged "Datum xy"
ui_expect_log "Coil Plane: 1 plane [plane xy]" "XY picked"
ui_expect_log "Coil Center X: " "the centre where the click was"
ui_step "Revolution and Pitch"             ui_choose "Panel Coil input type" 1
ui_expect_log "Preview Coil: ok" "previewed"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_log "Added coil (New Body), diameter 40" "coil added"
# ui_coil: pi 2^2 * 3 sqrt((40 pi)^2 + 10^2).
ui_expect_volume "New body Body6 (F14.b0): volume \([0-9.]*\) mm3" 4752.386440264496 "coil" 1e-5
[ -n "$SHOT" ] && ui_capture "$SHOT"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
ui_stop_app

echo "=== Rib and Web on the bracket"
ui_start_app --open "$WORK/bracket.mitcad"
ui_step "fit (F6)"                         ui_key F6
ui_step "rib (search)"                     ui_command "Rib"
ui_step "pick the rib's line"              ui_click_logged "Sketch entity F6/c1"
ui_expect_log "Rib Curves: 1 sketch curve [sketch curve c1 of F6]" "line picked"
ui_step "thickness 4"                      ui_type_in "Panel Rib input thickness" "4"
ui_expect_log "Preview Rib: ok" "previewed"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_log "Added rib on 1 curve(s), thickness 4" "rib added"
ui_expect_volume "Body Body1 (F2.b0): volume 20000.000 -> \([0-9.]*\) mm3" 21800 "a triangle 4 thick more"

ui_step "web (search)"                     ui_command "Web"
ui_step "pick the first line"              ui_click_logged "Sketch entity F8/c1"
ui_step "pick the second line"             ui_click_logged "Sketch entity F8/c4"
ui_expect_log "Web Curves: 2 sketch curves" "two lines picked"
ui_step "Depth"                            ui_choose "Panel Web input depth_type" 1
ui_expect_log "Preview Web: ok" "previewed"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_volume "Body Body1 (F2.b0): volume 21800.000 -> \([0-9.]*\) mm3" 22840 "webs 2 thick, 10 deep"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
ui_stop_app

echo "=== Helix"
# helix turns distance: the volume a 2 mm square turned about Z sweeps.
helix() { awk -v n="$1" -v r="$2" 'BEGIN { printf "%.6f", n * 2 * 3.14159265358979 * r * 4 }'; }
ui_start_app --open "$WORK/helix.mitcad"
ui_step "fit (F6)"                         ui_key F6
ui_expect_log "Timeline Helix1 at" "the helix on the timeline"

echo "--- Helix1, a height and turns of parameters: 2 turns"
ui_step "double-click Helix1"              ui_double_click_logged "Timeline Helix1"
ui_expect_log "Editing F2 with Helix" "editing Helix1"
ui_expect_log "Panel Helix choices: type=revolutions_and_height, hand=right" "the height and the turns"
ui_expect_log "Helix Height: h = 20 mm" "the height is h"
ui_expect_log "Helix Revolutions: t = 4" "the turns are t"
ui_step "2 turns"                          ui_type_in "Panel Helix input revolutions" "2"
ui_expect_log "Helix Revolutions: 2 = 2" "2 turns typed"
ui_expect_log "Preview Helix: ok" "previewed"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_log "Edited F2" "Helix1 edited"
ui_expect_volume "Body Body1 (F2.b0): volume [0-9.]* -> \([0-9.]*\) mm3" "$(helix 2 11)" "two turns" 1e-5

echo "--- Helix2: the other square about Z, 3 turns of 10"
ui_mark
ui_step "helix (search)"                   ui_command "Helix"
ui_expect_new "Command Helix started" "helix started"
ui_expect_new "Panel Helix choices: type=revolutions_and_pitch, hand=right, operation=new_body" \
  "revolutions and pitch, right-handed, a new body"
ui_expect_new "Helix Profiles: 1 profile [profile $RECT of F3]" "the newest profile taken"
ui_expect_new "Datum z at" "the axis input takes the origin's axes"
ui_step "pick the Z axis"                  ui_click_logged "Datum z"
ui_expect_new "Helix Axis: 1 axis [axis z]" "Z picked"
ui_expect_new "Manipulator Helix pitch at" "the pitch has an arrow"
ui_expect_new "Preview Helix: ok" "previewed"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Added helix (New Body), 3 turns of 10, right-handed" "helix added"
ui_expect_volume "New body Body2 (F4.b0): volume \([0-9.]*\) mm3" "$(helix 3 31)" "three turns" 1e-5

echo "--- Helix2 edited: 25 high at pitch 5, left-handed"
ui_mark
ui_step "double-click Helix2"              ui_double_click_logged "Timeline Helix2"
ui_expect_new "Editing F4 with Helix" "editing Helix2"
ui_step "Height and Pitch"                 ui_choose "Panel Helix input type" 1
ui_expect_new "Helix: Type = Height and Pitch" "height and pitch"
ui_expect_new "Panel Helix input height at" "the height shown"
ui_step "height 25"                        ui_type_in "Panel Helix input height" "25"
ui_step "pitch 5"                          ui_type_in "Panel Helix input pitch" "5"
ui_expect_new "Manipulator Helix height at" "the height has an arrow"
ui_step "Left Hand"                        ui_choose "Panel Helix input hand" 1
ui_expect_new "Helix: Handedness = Left Hand" "left-handed"
ui_step "growth -2"                        ui_type_in "Panel Helix input growth" "-2"
ui_expect_new "Helix Growth: -2 = -2 mm" "narrowing 2 a turn"
ui_expect_new "Preview Helix: ok" "previewed"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Edited F4" "Helix2 edited"
ui_expect_new "Body Body2 (F4.b0): volume" "the helix measured"
# From 31 to 21 mm from the axis: a mean of 26.
ui_expect_volume "Body Body2 (F4.b0): volume [0-9.]* -> \([0-9.]*\) mm3" "$(helix 5 26)" "five turns, narrowing" 1e-5

echo "--- Helix2 opens as it was made"
ui_mark
ui_step "double-click Helix2"              ui_double_click_logged "Timeline Helix2"
ui_expect_new "Panel Helix choices: type=height_and_pitch, hand=left, operation=new_body" \
  "height and pitch, left-handed"
ui_expect_new "Helix Height: 25 mm" "the height of 25"
ui_expect_new "Helix Pitch: 5 mm" "the pitch of 5"
ui_expect_new "Helix Growth: -2 mm" "the growth of -2"
ui_step "cancel (Esc)"                     ui_key Escape
ui_expect_new "Command Helix cancelled" "nothing changed"
ui_mark
ui_key ctrl+s
ui_expect_new "Saved $WORK/helix.mitcad" "saved"
expect_file "$WORK/helix.mitcad" '[f.get("construction") for f in doc["features"] if f["type"] == "helix"]' \
  "[None, 'mitcad']" "Helix2 is Mitcad's growing helix"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
ui_stop_app

echo "--- A helix of FreeCAD's construction edited: 2 turns"
ui_start_app --open "$WORK/freecad.mitcad"
ui_step "fit (F6)"                         ui_key F6
ui_expect_log "Timeline Helix1 at" "the helix on the timeline"
ui_mark
ui_step "double-click Helix1"              ui_double_click_logged "Timeline Helix1"
ui_expect_new "Editing F2 with Helix" "editing Helix1"
ui_expect_new "Helix Growth: 1 mm" "the growth of 1"
ui_step "2 turns"                          ui_type_in "Panel Helix input revolutions" "2"
ui_expect_new "Preview Helix: ok" "previewed"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Edited F2" "Helix1 edited"
ui_mark
ui_key ctrl+s
ui_expect_new "Saved $WORK/freecad.mitcad" "saved"
expect_file "$WORK/freecad.mitcad" \
  '[([p["value"] for p in doc["parameters"] if p["name"] == f["revolutions"]], f.get("construction")) for f in doc["features"] if f["type"] == "helix"]' \
  "[([2.0], 'freecad')]" "still FreeCAD's construction, 2 turns"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
ui_finish "UI sweep test"
