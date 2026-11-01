#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/PulleyCommands.cpp app/framework/CommandSession.cpp
# check-all sources: core/model/src/features/pulley.rs core/model/src/features/pulley_profile.rs core/model-api/src/pulleys.rs
# check-all sources: app/commands/PlaceInputs.cpp core/model/src/features/seat.rs core/model-api/src/seats.rs tools/cli
# check-all sources: core/model/src/features/place_at.rs core/model/src/features/drives.rs core/model-api/src/drives.rs
# check-all sources: app/framework/ManipulatorOverlay.cpp app/commands/DriveCommands.cpp
# Pulley (mitcad#562) through the real UI; the Windows counterpart is
# tools/ui-windows-pulley-test.ps1 (ctest app.ui-windows-pulley):
#   1. Pulley in an empty design, placed by a click on the XY plane and
#      both distances 0 (it waits for a pick, mitcad#656): a GT2 2 mm
#      pulley of 20 teeth; its note shows the pitch diameter (20 x 2 / pi =
#      12.7324 mm). In the preview: flanges 24 across, then 30 teeth (pitch
#      diameter 60 / pi = 19.0986 mm), then a flange on one side only. OK
#      makes Pulley1 (F1.b0).
#   2. Pulley1 edited: it opens with its flange on one side and its 30
#      teeth; 20 teeth again, OK.
#   3. Pulley again (20 teeth), 80 mm from Pulley1 along X (mitcad#656):
#      Distance 1 typed in the dimension's value box in the view; Check
#      Belt, under Place On, a face of Pulley1: its rows show the belt
#      round both (two equal pulleys: 2 x 80 + 20 x 2 = 200 mm, 100 teeth)
#      and the view draws its pitch line; the library's 200 mm GT2 belt,
#      6 mm wide, fits as the pulleys are (0 mm of travel). OK makes
#      F2.b0, which keeps the check (belt_check).
#   4. On a shaft (mitcad#578), in a design mitcad-cli makes (an 8 mm
#      cylinder 30 high, Cylinder1, F1.b0): Pulley with Place On the
#      cylinder's top edge: the Bore Clearance starts as the printed-fit
#      parameter shaft_bore_clearance (0.2 mm, mitcad#623), so the note
#      shows the bore the shaft gives as 8.2 mm; an offset of 2 mm and a
#      typed bore clearance of 0.3 mm make it 8.3 mm; OK makes the pulley
#      on the shaft (F2.b0).
#   5. Belt Drive (mitcad#656), in a design with two 8 mm shafts 60 mm
#      apart: the shafts' top edges picked as the two axes, ratio 2, GT2
#      2 mm: the rows list the tooth counts with their belts, the least
#      tension travel first; the first previewed as new bodies; OK makes
#      both pulleys.
#   6. Check Belt with a shaft in another component's occurrence
#      (mitcad#754), in a design with an 8 mm shaft 30 high in Motor:1
#      (F1.b0, the occurrence 80 mm along X) and a 20-tooth pulley in the
#      root at the origin, 30 mm up and facing down (F2.b0): Pulley (20
#      teeth) on the shaft's top edge where the occurrence shows it; its
#      Offset handle sits on that shaft, not at the root's origin; Check
#      Belt with the root's pulley gives the belt for the pulleys 80 mm
#      apart (2 x 80 + 20 x 2 = 200 mm); OK makes the pulley there.
#   7. A value box closed while its preview computes (mitcad#888), in a
#      design in inches, with the previews made slow
#      (MITCAD_TEST_RECOMPUTE_DELAY_MS): Distance 1 typed in the view and
#      Enter pressed while the preview of the typed value computes (in a
#      busy machine's check, a key the test sent while the app was idle
#      arrived after a preview's refinement had started). The box gives the
#      value the design's unit without reading the document (the window's
#      last snapshot has it), the value waits for the job's end (3 in), and
#      its preview follows.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-pulley-test.sh [screenshot.png]

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-pulley.XXXXXX)
trap 'ui_cleanup; rm -rf "$WORK"' EXIT

cat > "$WORK/shaft.json" << 'EOF'
[
  {"cmd": "add_feature", "def": {"type": "cylinder", "plane": "xy", "center": [0, 0],
    "diameter": 8, "height": 30, "operation": "new_body"}}
]
EOF
"$CLI" run "$WORK/shaft.json" --save "$WORK/shaft.mitcad" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }
cat > "$WORK/shafts.json" << 'JSON'
[
  {"cmd": "add_feature", "def": {"type": "cylinder", "plane": "xy", "center": [0, 0],
    "diameter": 8, "height": 30, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "cylinder", "plane": "xy", "center": [60, 0],
    "diameter": 8, "height": 30, "operation": "new_body"}}
]
JSON
"$CLI" run "$WORK/shafts.json" --save "$WORK/shafts.mitcad" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }
cat > "$WORK/motor.json" << 'JSON'
[
  {"cmd": "create_component", "name": "Motor", "transform": {"translation": [80, 0, 0]}},
  {"cmd": "add_feature", "def": {"type": "cylinder", "plane": "xy", "center": [0, 0],
    "diameter": 8, "height": 30, "operation": "new_body"}},
  {"cmd": "activate_component", "component": "Root"},
  {"cmd": "add_feature", "def": {"type": "pulley", "kind": "toothed", "profile": "gt2_2mm",
    "teeth": 20, "belt_width": 6, "operation": "new_body",
    "at": {"point": {"point": [0, 0, 30]}, "plane": "xy", "flip": true}}}
]
JSON
"$CLI" run "$WORK/motor.json" --save "$WORK/motor.mitcad" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }
cat > "$WORK/inch.json" << 'JSON'
[
  {"cmd": "set_units", "length": "in"}
]
JSON
"$CLI" run "$WORK/inch.json" --save "$WORK/inch.mitcad" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

ui_start_display
ui_start_app

echo "--- A GT2 pulley on XY, its teeth and flanges changed in the preview"
ui_mark
ui_step "Pulley"                           ui_command "Pulley"
ui_expect_new "Command Pulley started" "the panel opened" 30
ui_place_on_xy Pulley
ui_expect_new "Panel Pulley note: Pitch diameter 12.7324 mm" "its pitch diameter shown"
ui_mark
ui_step "flange diameter 24"               ui_type_in "Panel Pulley input flange_diameter" "24"
ui_expect_new "Preview Pulley: ok" "the wider flanges previewed" 30
ui_mark
ui_step "teeth 30"                         ui_type_in "Panel Pulley input teeth" "30"
ui_expect_new "Panel Pulley note: Pitch diameter 19.0986 mm" "the pitch diameter of 30 teeth" 30
ui_mark
ui_step "flanges: One"                     ui_choose "Panel Pulley input flanges" -1
ui_expect_new "Preview Pulley: ok" "one flange previewed" 30
[ -n "$SHOT" ] && ui_capture "$SHOT"
ui_mark
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Added pulley (Toothed), GT2 2 mm, 30 teeth" "the pulley added" 30
ui_expect_new "(F1.b0): volume " "its body made" 30

echo "--- Pulley1 edited back to 20 teeth"
ui_mark
ui_step "double-click Pulley1"             ui_double_click_logged "Timeline Pulley1"
ui_expect_new "Editing F1 with Pulley" "Pulley1 opens for editing" 30
ui_expect_new "flanges=one" "with its flange on one side"
ui_expect_new "Panel Pulley note: Pitch diameter 19.0986 mm" "and its 30 teeth" 30
ui_mark
ui_step "teeth 20"                         ui_type_in "Panel Pulley input teeth" "20"
ui_expect_new "Panel Pulley note: Pitch diameter 12.7324 mm" "the edit previewed" 30
ui_mark
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Edited F1" "Pulley1 edited" 30


echo "--- A second pulley 80 mm along X, the belt round both checked (mitcad#656)"
# No fit (F6) here: fitted to Pulley1 alone, the view would show the XY
# plane only under the pulley, with no place where a click picks it.
ui_mark
ui_step "Pulley"                           ui_command "Pulley"
ui_place_on_xy Pulley
ui_expect_new "Value box Pulley at_offset1 at " "Distance 1 is a dimension in the view" 30
ui_mark
ui_step "Distance 1's value box"           ui_click_logged "Value box Pulley at_offset1"
ui_expect_new "Value box Pulley at_offset1 opened: 0" "a field over the view edits it"
ui_step "type 80"                          xdotool type --delay 40 "80"
ui_step "Enter keeps it"                   ui_key Return
ui_expect_new "Value box Pulley at_offset1 closed: 80" "the distance typed in the view"
ui_expect_new "Preview Pulley: ok" "previewed 80 mm along X" 30
grep -q "Added pulley" <(tail -n +"$UI_MARK" "$UI_LOG") && ui_fail "Enter in the value box confirmed the command"
ui_mark
ui_step "Check Belt"                       ui_click_logged "Panel Pulley input check_belt"
ui_expect_new "Pick places:" "the picks are logged"
ui_expect_new "Pick face F1.b0/F1:" "a face of Pulley1 can be picked"
face=$(sed -n 's/^Pick face F1\.b0\/\(F1:[^ ]*\) at [0-9]*,[0-9]*$/\1/p' "$UI_LOG" | tail -1)
[ -n "$face" ] || ui_fail "no face of Pulley1 to pick"
ui_mark
ui_step "click $face"                      ui_click_pick "face F1.b0/$face at"
ui_expect_new "Pulley belt check with F1: Belt length 200 mm (100 teeth)" "the belt round both, in its own rows" 30
ui_expect_new "Lines Pulley: 1" "the belt's pitch line drawn" 30
ui_expect_new "Nearest belt GT2 200 mm, 6 mm wide, Travel 0 mm, fits" "the library's belt, its travel and its verdict" 30
grep "^Pulley belt check with " "$UI_LOG" | tail -1 | sed 's/^/     /'
ui_mark
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Added pulley (Toothed), GT2 2 mm, 20 teeth" "the second pulley added" 30
ui_expect_new "(F2.b0): volume " "its body made" 30
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
ui_mark
ui_step "double-click Pulley2"             ui_double_click_logged "Timeline Pulley2"
ui_expect_new "Editing F2 with Pulley" "Pulley2 opens for editing" 30
ui_expect_new "Pulley belt check with F1: Belt length" "its belt check kept with it (belt_check)" 30
ui_step "Cancel (Escape)"                  ui_key Escape

echo "--- A pulley on a shaft (mitcad#578)"
ui_stop_app
ui_start_app --open "$WORK/shaft.mitcad"
ui_step "fit (F6)"                         ui_key F6
ui_expect_log "Bodies shown: F1.b0" "the shaft shown"
ui_mark
ui_step "Pulley"                           ui_command "Pulley"
ui_expect_new "Command Pulley started" "the panel opened, waiting for a pick" 30
ui_expect_new "Pulley Bore Clearance: shaft_bore_clearance = 0.2 mm" "the bore clearance is the printed-fit parameter"
ui_step "teeth 20"                       ui_type_in "Panel Pulley input teeth" "20"
ui_mark
ui_step "Place On"                         ui_click_logged "Panel Pulley input at"
ui_expect_new "Pick places:" "the picks are logged"
ui_expect_new "Pick edge F1.b0/E{F1:side0|F1:top} at" "the shaft's top edge can be picked"
ui_mark
ui_step "click the shaft's top edge"       ui_click_pick "edge F1.b0/E{F1:side0|F1:top} at"
ui_expect_new "Place Pulley: axis edge E{F1:side0|F1:top} of F1.b0" "placed on the shaft's end"
ui_expect_new "bore 8.2 mm (shaft 8 mm)" "the bore the shaft gives, with the printed fit" 30
ui_mark
ui_step "More"                             ui_click_logged "Panel Pulley input at_more"
ui_step "offset 2"                         ui_type_in "Panel Pulley input at_offset" "2"
ui_expect_new "Preview Pulley: ok" "2 mm down the shaft previewed" 30
ui_mark
ui_step "bore clearance 0.3"               ui_type_in "Panel Pulley input bore_clearance" "0.3"
ui_expect_new "bore 8.3 mm (shaft 8 mm)" "a typed clearance replaces the parameter" 30
grep "^Panel Pulley note: " "$UI_LOG" | tail -1 | sed 's/^/     /'
ui_mark
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Added pulley (Toothed), GT2 2 mm, 20 teeth" "the pulley on the shaft added" 30
ui_expect_new "(F2.b0): volume " "its body made" 30
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"

echo "--- 5. Belt Drive between two shafts (mitcad#656)"
ui_stop_app
ui_start_app --open "$WORK/shafts.mitcad"
ui_step "fit (F6)"                         ui_key F6
ui_expect_log "Bodies shown: F1.b0, F2.b0" "the shafts shown"
ui_mark
ui_step "Belt Drive"                       ui_command "Belt Drive"
ui_expect_new "Command Belt Drive started" "the panel opened" 30
ui_expect_new "Pick edge F1.b0/E{F1:side0|F1:top} at" "the first shaft's top edge can be picked"
ui_mark
ui_step "the first shaft"                  ui_click_pick "edge F1.b0/E{F1:side0|F1:top} at"
ui_expect_new "Pick edge F2.b0/E{F2:side0|F2:top} at" "the second shaft's top edge can be picked"
ui_mark
ui_step "the second shaft"                 ui_click_pick "edge F2.b0/E{F2:side0|F2:top} at"
ui_expect_new "Belt Drive candidates: " "the model's candidates" 30
ui_expect_new "center distance 60 mm" "the axes 60 mm apart"
ui_expect_new "Panel Belt Drive options candidate: " "the rows listed" 30
grep "^Panel Belt Drive options candidate: " "$UI_LOG" | tail -1 | cut -c1-160 | sed 's/^/     /'
ui_mark
ui_step "operation: New Body"              ui_choose "Panel Belt Drive input operation" 1
ui_expect_new "Preview Belt Drive: ok" "the drive previewed" 30
ui_expect_new "Panel Belt Drive note: Belt length " "the belt as values" 30
ui_mark
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Added belt drive (gt2_2mm), " "both pulleys added" 30
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"

echo "--- 6. Check Belt with a shaft in another component's occurrence (mitcad#754)"
ui_stop_app
ui_start_app --open "$WORK/motor.mitcad"
ui_step "fit (F6)"                         ui_key F6
ui_expect_log "Body F1.b0 in O1 at " "the motor's shaft shown in its occurrence"
ui_mark
ui_step "Pulley"                           ui_command "Pulley"
ui_expect_new "Command Pulley started" "the panel opened, waiting for a pick" 30
ui_step "teeth 20"                         ui_type_in "Panel Pulley input teeth" "20"
ui_mark
ui_step "Place On"                         ui_click_logged "Panel Pulley input at"
ui_expect_new "Pick places:" "the picks are logged"
ui_expect_new "Pick edge F1.b0/E{F1:side0|F1:top} at" "the motor's shaft top edge can be picked"
ui_expect_new "Pick face F2.b0/" "the root's pulley can be picked"
# Where the shaft (80 mm along X) and the root's pulley (at the origin)
# are on the screen.
read -r shaft_x _ <<< "$(ui_pick_at "edge F1.b0/E{F1:side0|F1:top} at")"
read -r root_x _ <<< "$(ui_pick_at "face F2.b0/")"
ui_mark
ui_step "click the shaft's top edge"       ui_click_pick "edge F1.b0/E{F1:side0|F1:top} at"
ui_expect_new "Place Pulley: axis edge E{F1:side0|F1:top} of F1.b0" "placed on the motor's shaft"
# The handle shows with the placement (logged since the click); More
# shows the rows Check Belt is in.
ui_expect_new "Manipulator Pulley at_offset at " "the Offset handle shown" 30
ui_step "More"                             ui_click_logged "Panel Pulley input at_more"
read -r handle_x _ <<< "$(ui_logged_at "Manipulator Pulley at_offset")"
# The handle on the shaft where its occurrence puts it, not where the
# shaft lies in Motor's own coordinates (the root's origin).
[ $(( (handle_x - shaft_x) * (handle_x - shaft_x) )) -lt $(( (handle_x - root_x) * (handle_x - root_x) )) ] ||
  ui_fail "the Offset handle at x $handle_x is not on the shaft (x $shaft_x) but nearer the origin (x $root_x)"
echo "ok   the Offset handle on the motor's shaft (x $handle_x, the shaft $shaft_x, the origin $root_x)"
ui_mark
ui_step "Check Belt"                       ui_click_logged "Panel Pulley input check_belt"
ui_expect_new "Pick places:" "the picks are logged"
ui_expect_new "Pick face F2.b0/F2:" "a face of the root's pulley can be picked"
face=$(sed -n 's/^Pick face F2\.b0\/\(F2:[^ ]*\) at [0-9]*,[0-9]*$/\1/p' "$UI_LOG" | tail -1)
[ -n "$face" ] || ui_fail "no face of the root's pulley to pick"
ui_mark
ui_step "click $face"                      ui_click_pick "face F2.b0/$face at"
ui_expect_new "Pulley belt check with F2: Belt length 200 mm (100 teeth)" "the belt for the pulleys 80 mm apart" 30
grep "^Pulley belt check with " "$UI_LOG" | tail -1 | sed 's/^/     /'
ui_mark
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Added pulley (Toothed), GT2 2 mm, 20 teeth" "the pulley on the motor's shaft added" 30
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"

echo "--- 7. A value box closed while its preview computes (mitcad#888)"
ui_stop_app
# In a design in inches: the unit the box gives is the design's.
MITCAD_TEST_RECOMPUTE_DELAY_MS=0,preview=2500 ui_start_app --open "$WORK/inch.mitcad"
ui_mark
ui_step "Pulley"                           ui_command "Pulley"
ui_expect_new "Command Pulley started" "the panel opened" 30
ui_show_origin
ui_mark
ui_step "Place On: the XY plane"           ui_click_logged "Datum xy"
ui_expect_new "Place Pulley: face plane xy, " "placed on the XY plane" 30
ui_step "Distance 1: 0"                    ui_type_in "Panel Pulley input at_offset1" "0"
ui_step "Distance 2: 0"                    ui_type_in "Panel Pulley input at_offset2" "0"
ui_expect_new "Pulley Distance 2: 0 = 0 in" "the distances typed, in inches" 30
ui_expect_new "Preview Pulley: ok" "the slow preview at the origin" 30
ui_expect_new "Value box Pulley at_offset1 at " "Distance 1 is a dimension in the view" 30
ui_mark
ui_step "Distance 1's value box"           ui_click_logged "Value box Pulley at_offset1"
ui_expect_new "Value box Pulley at_offset1 opened: 0" "a field over the view edits it"
ui_step "type 3"                           xdotool type --delay 40 "3"
ui_expect_new "Computing preview started" "the typed value's preview computes" 30
# Not through ui_step, which waits until the app is idle: the key goes
# while the job runs, as a key typed at once after a value does.
typed_at=$(wc -l < "$UI_LOG")
ui_key Return
ui_sync
ui_crashed && ui_fail "crashed by Enter in the value box during the job"
echo "ok   Enter while it computes"
ui_expect_new "Value box Pulley at_offset1 closed: 3" "the box closed during the job" 30
# The box closed before the job's end: Enter came while it computed.
awk -v from="$typed_at" 'NR <= from { next } /Computing preview done/ { exit 1 }
  /Value box Pulley at_offset1 closed: 3/ { exit 0 }' "$UI_LOG" ||
  ui_fail "the preview ended before Enter: the box was not closed during the job"
ui_expect_new "Pulley Distance 1: 3 in = 3 in" "the value kept with the design's unit once the job ended" 30
ui_expect_new "Preview Pulley: ok" "the preview of 3 in follows" 30
grep -q "The document was read while a job computes it" "$UI_LOG" && ui_fail "the document was read during a job"
ui_step "Cancel (Escape)"                  ui_key Escape

ui_finish "UI pulley test"
