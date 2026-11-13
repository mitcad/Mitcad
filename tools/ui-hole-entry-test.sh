#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/CommandPanel.cpp app/framework/CommandSession.cpp
# check-all sources: app/framework/ManipulatorOverlay.cpp app/commands/HoleCommands.cpp app/commands/PlaceInputs.cpp
# check-all sources: core/model/src/features/hole.rs tools/cli
# Hole's keyboard, its dimensions in the view (mitcad#1181) and its Reference
# rows (mitcad#1285), through the real UI on the demo block (60 x 40 x 20,
# Body1, F2.b0); the Windows counterpart is
# tools/ui-windows-hole-entry-test.ps1 (ctest app.ui-windows-hole-entry):
#   1. In the Floating and the Docked layout: Hole on the top face,
#      Distance 1 and Distance 2 typed (15, 20). Tab in Distance 2 moves
#      the keyboard on to Diameter, which takes what is typed next (10);
#      Shift+Tab goes back to Distance 2, and again to Distance 1.
#   2. Floating: the dimensions in the view take typed values. Distance
#      2's value clicked, 25 typed and Esc: the box closes and 20 is back.
#      Depth's value clicked, 12 and Enter: Depth is 12 and the preview
#      follows. Depth's again, "abc": the box turns to the error colour,
#      Enter is refused (the box stays, the command too), Esc puts 12 back.
#      Depth's again, 10 and Enter. OK makes the hole 15 and 20 from the
#      edges, 10 across and 10 deep (the volume of ui-create-test's hole).
#   3. Floating (mitcad#1285): Hole at the top face's center, 10 across;
#      the edge across the face picked through the Reference 2 row: no
#      failure notice, the hole where it was, Distance 1 and 2 evaluated,
#      Distance 2 the distance from the new edge. Then Distance 1 typed 12
#      and the edge across from Reference 1 picked through its row (or,
#      when the view hides that edge, the same with Reference 2's first
#      edge): the hole stays, the distance is the rest of the face's width.
#      OK makes the hole.
#   4. Print Shape with and without a print bed face (mitcad#1744), on a
#      60 x 40 x 20 block that mitcad-cli makes (Sketch1 F1, Extrude1 F2)
#      with a 6 mm hole 10 deep lying in it, started on its side at y 0
#      along +Y (Hole1 F3, `plane`). Without a print bed face: Hole1 edited
#      shows Print Shape, Teardrop chosen, OK and saved: the saved design's
#      hole has two roof faces whose outward normals point down and
#      sideways, so the roof points to +Z. With the block's top as its
#      print bed face: the same, the roof pointing away from it, to -Z.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-hole-entry-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

RECT='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'
ROOT=$(cd "$(dirname "$0")/.." && pwd)
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-hole-entry.XXXXXX")
trap 'ui_cleanup; rm -rf "$WORK"' EXIT

# hole_on_top layout [app arguments]: the demo in that layout, Hole on the
# block's top face with its distances 15 and 20 typed, the keyboard in
# Distance 2.
hole_on_top() {
  echo "=== Hole, $1"
  shift
  ui_start_app --demo "$@"
  ui_step "fit (F6)"                       ui_key F6
  ui_mark
  ui_step "hole (H)"                       ui_key h
  ui_expect_new "Command Hole started" "hole started"
  ui_expect_new "Pick places:" "the picks are logged"
  ui_step "click the top face"             ui_click_pick "face F2.b0/F2:end($RECT)"
  ui_expect_new "Hole Place On: 1 face" "the face picked"
  ui_expect_new "Place Hole: face face F2:end(" "measured from two of its edges"
  ui_step "distance 1: 15"                 ui_type_in "Panel Hole input at_offset1" "15"
  ui_step "distance 2: 20"                 ui_type_in "Panel Hole input at_offset2" "20"
}

# tab_steps: Tab from Distance 2 to Diameter, which takes the typed 10;
# Shift+Tab back to Distance 2 and on to Distance 1.
tab_steps() {
  local from typed
  echo "--- Tab and Shift+Tab between the fields"
  ui_mark
  from=$(wc -l < "$UI_LOG")
  ui_step "Tab"                            ui_key Tab
  ui_expect_new "Panel Hole Tab: at_offset2 -> diameter" "Tab goes on from Distance 2 to Diameter"
  ui_step "type 10"                        xdotool type --delay 40 -- "10"
  ui_sync
  typed=$(ui_typed_since "$from" "Panel field diameter: ")
  [ "$typed" = "10" ] || ui_fail "Diameter has '$typed' after Tab and 10 typed"
  echo "ok   Diameter took what was typed"
  ui_expect_new "Hole Diameter: 10" "the diameter evaluated"
  grep -q "^Panel field at_offset2: 2010$\|^Panel field at_offset2: 2010 mm$" <(tail -n +"$from" "$UI_LOG") &&
    ui_fail "the typed 10 went into Distance 2"
  ui_mark
  ui_step "Shift+Tab"                      ui_key shift+Tab
  ui_expect_new "Panel Hole Shift+Tab: diameter -> at_offset2" "Shift+Tab back to Distance 2"
  ui_mark
  ui_step "Shift+Tab again"                ui_key shift+Tab
  ui_expect_new "Panel Hole Shift+Tab: at_offset2 -> at_offset1" "and on to Distance 1"
  grep -q "Command Hole cancelled\|Done Hole" <(tail -n +"$from" "$UI_LOG") && ui_fail "Tab ended the command"
  echo "ok   the command is still open"
}

ui_start_display

hole_on_top "Floating" --chrome=floating
tab_steps

echo "--- The dimensions in the view take typed values"
ui_expect_log "Value box Hole at_offset2 at " "Distance 2 is a dimension in the view"
ui_expect_log "Value box Hole depth at " "and Depth"
ui_mark
ui_step "Distance 2's value"               ui_click_logged "Value box Hole at_offset2"
ui_expect_new "Value box Hole at_offset2 opened: 20" "a field over the view edits it"
ui_mark
ui_step "type 25"                          xdotool type --delay 40 -- "25"
ui_sync
ui_step "Esc puts it back"                 ui_key Escape
ui_expect_new "Value box Hole at_offset2 closed" "the box closed"
ui_expect_new "Hole Distance 2: 20" "Distance 2 is 20 again"
grep -q "Command Hole cancelled" <(tail -n +"$UI_MARK" "$UI_LOG") && ui_fail "Esc in the box cancelled the command"

ui_mark
ui_step "Depth's value"                    ui_click_logged "Value box Hole depth"
ui_expect_new "Value box Hole depth opened: 10" "a field over the view edits the depth"
ui_step "type 12"                          xdotool type --delay 40 -- "12"
ui_sync
ui_step "Enter keeps it"                   ui_key Return
ui_expect_new "Value box Hole depth closed" "the box closed"
ui_expect_new "Hole Depth: 12 mm = 12 mm" "Depth is 12"
ui_expect_new "Preview Hole: ok" "the preview followed" 30
grep -q "^Done Hole\|^Added 1 hole" <(tail -n +"$UI_MARK" "$UI_LOG") && ui_fail "Enter in the box ended the command"

ui_mark
ui_step "Depth's value again"              ui_click_logged "Value box Hole depth"
ui_expect_new "Value box Hole depth opened: 12" "the box opens with 12"
ui_step "type abc"                         xdotool type --delay 40 -- "abc"
ui_sync
ui_expect_new "Value box Hole depth invalid" "the box shows the error"
ui_mark
ui_step "Enter"                            ui_key Return
ui_expect_new "Value box Hole depth refused: abc" "Enter refused"
ui_sync
grep -q "Value box Hole depth closed\|^Done Hole\|^Added 1 hole" <(tail -n +"$UI_MARK" "$UI_LOG") &&
  ui_fail "Enter closed the box or ended the command with abc"
echo "ok   the box stays open"
ui_mark
ui_step "Esc puts 12 back"                 ui_key Escape
ui_expect_new "Value box Hole depth closed" "the box closed"
ui_expect_new "Hole Depth: 12" "Depth is 12 again"

ui_mark
ui_step "Depth's value once more"          ui_click_logged "Value box Hole depth"
ui_expect_new "Value box Hole depth opened: 12" "the box opens with 12"
ui_step "type 10"                          xdotool type --delay 40 -- "10"
ui_sync
ui_step "Enter keeps it"                   ui_key Return
ui_expect_new "Value box Hole depth closed" "the box closed"
ui_expect_new "Hole Depth: 10 mm = 10 mm" "Depth is 10"
ui_expect_new "Preview Hole: ok" "previewed" 30
ui_step "OK (Enter)"                       ui_key Return
ui_expect_log "Added 1 hole(s) (simple), diameter 10" "hole added"
# The hole of ui-create-test.sh (hole_simple_distance): 864.0506 mm3.
ui_expect_volume "Body Body1 (F2.b0): volume 48000.000 -> \([0-9.]*\) mm3" \
  "$(awk 'BEGIN { printf "%.6f", 47135.94936549554 }')" "one hole, 10 deep"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
ui_stop_app

hole_on_top "Docked" --chrome=docked
tab_steps
ui_step "Esc (cancel)"                     ui_key Escape
ui_stop_app

echo "=== A reference picked in its row (mitcad#1285)"
# The sketch line of a top edge (c1 along X at y 0, c2 at x 60, c3 at y 40,
# c4 at x 0), the one across the face from it, and the face's center's
# distance from it.
side_of() { sed -n 's/.*F2:side(\(c[1-4]\)\[.*/\1/p' <<< "$1"; }
token_of() {
  case $1 in
    c1) echo "c1[c4,c2]" ;;
    c2) echo "c2[c1,c3]" ;;
    c3) echo "c3[c2,c4]" ;;
    *)  echo "c4[c3,c1]" ;;
  esac
}
opposite_of() { case $1 in c1) token_of c3 ;; c2) token_of c4 ;; c3) token_of c1 ;; *) token_of c2 ;; esac; }
center_from() { case $1 in c1 | c3) echo 20 ;; *) echo 30 ;; esac; }
# The top edge of the side `cN[neighbours]` among the newest pick places;
# empty when it cannot be clicked (behind the panel, say).
top_edge() {
  local start
  start=$(grep -n "^Pick places:" "$UI_LOG" | tail -1 | cut -d: -f1)
  tail -n +"${start:-1}" "$UI_LOG" | grep "^Pick edge F2\.b0/E{" | grep -F "F2:end(" | grep -F "F2:side($1)" |
    sed -n 's/^Pick edge F2\.b0\/\(E{[^ ]*}\) at .*/\1/p' | tail -1
}
ui_start_app --demo --chrome=floating
ui_step "fit (F6)"                         ui_key F6
ui_mark
ui_step "hole (H)"                         ui_key h
ui_expect_new "Command Hole started" "hole started"
ui_expect_new "Pick places:" "the picks are logged"
ui_step "click the top face"               ui_click_pick "face F2.b0/F2:end($RECT)"
ui_expect_new "Hole Place On: 1 face" "the face picked"
ui_expect_new "Place Hole: face face F2:end(" "measured from two of its edges"
refs=$(grep "^Place Hole: face face " "$UI_LOG" | tail -1 |
  sed -n 's/^Place Hole: face face [^ ]* of F2\.b0, [0-9.-]* mm from edge \(E{[^ ]*}\) of F2\.b0, [0-9.-]* mm from edge \(E{[^ ]*}\) of F2\.b0 (point.*/\1 \2/p')
read -r ref1 ref2 <<< "$refs"
[ -n "$ref2" ] || ui_fail "no references logged"
side1=$(side_of "$ref1")
side2=$(side_of "$ref2")
opposite=$(opposite_of "$side2")
d1=$(center_from "$side1")
d2=$(center_from "$side2")
echo "     references $side1 and $side2: the face's center $d1 and $d2 from them"
ui_step "distance 1: $d1"                  ui_type_in "Panel Hole input at_offset1" "$d1"
ui_step "distance 2: $d2"                  ui_type_in "Panel Hole input at_offset2" "$d2"
ui_step "diameter 10"                      ui_type_in "Panel Hole input diameter" "10"
ui_expect_new "Preview Hole: ok" "the hole previewed at the face's center" 30
ui_step "the Reference 2 row"              ui_click_logged "Panel Hole input at_edge2"
other=$(top_edge "$opposite")
[ -n "$other" ] || ui_fail "no pick place logged for the edge across from Reference 2"
# For the second pick: the edge across from Reference 1, or else Reference
# 2's first edge, whichever the view lets a click pick.
across1=$(top_edge "$(opposite_of "$side1")")
first2=$(top_edge "$(token_of "$side2")")
ui_mark
ui_step "click the edge across the face"   ui_click_pick "edge F2.b0/$other at"
ui_expect_new "Hole Reference 2: 1 edge [edge $other of F2.b0]" "the edge across the face is Reference 2"
ui_expect_new ", $d2 mm from edge $other of F2.b0 (point 30, 20, 20)" \
  "the hole kept at the center, $d2 mm from the new edge"
ui_expect_new "Hole Distance 1: $d1 mm = $d1 mm" "Distance 1 evaluated"
ui_expect_new "Hole Distance 2: $d2 mm = $d2 mm" "Distance 2 evaluated"
ui_expect_new "Preview Hole: ok" "previewed where it was" 30
grep -q "^Panel Hole error: " <(tail -n +"$UI_MARK" "$UI_LOG") &&
  ui_fail "a failure notice: $(grep "^Panel Hole error: " "$UI_LOG" | tail -1)"
echo "ok   no failure notice"

# Off the center: 12 mm from a row's reference, then the edge across from
# it picked through that row measures the rest of the face's width.
if [ -n "$across1" ]; then
  row=1 now=$side1 edge=$across1
elif [ -n "$first2" ]; then
  row=2 now=$(side_of "F2:side($opposite)") edge=$first2
else
  ui_fail "neither the edge across from Reference 1 nor Reference 2's first edge can be picked"
fi
case $now in
  c1) width=40 at="30, 12, 20" ;;
  c3) width=40 at="30, 28, 20" ;;
  c2) width=60 at="48, 20, 20" ;;
  *)  width=60 at="12, 20, 20" ;;
esac
rest=$((width - 12))
ui_mark
ui_step "distance $row: 12"                ui_type_in "Panel Hole input at_offset$row" "12"
ui_expect_new "Preview Hole: ok" "previewed 12 mm from Reference $row" 30
ui_step "the Reference $row row"           ui_click_logged "Panel Hole input at_edge$row"
ui_mark
ui_step "click the edge across from it"    ui_click_pick "edge F2.b0/$edge at"
ui_expect_new "Hole Reference $row: 1 edge [edge $edge of F2.b0]" "the edge across the face is Reference $row"
ui_expect_new "$rest mm from edge $edge of F2.b0" "$rest mm from the new edge"
grep "^Place Hole: " "$UI_LOG" | tail -1 | grep -qF "(point $at)" ||
  ui_fail "the hole moved: $(grep "^Place Hole: " "$UI_LOG" | tail -1)"
echo "ok   the hole kept at $at"
ui_expect_new "Hole Distance $row: $rest mm = $rest mm" "Distance $row evaluated"
ui_expect_new "Preview Hole: ok" "previewed where it was" 30
grep -q "^Panel Hole error: " <(tail -n +"$UI_MARK" "$UI_LOG") &&
  ui_fail "a failure notice: $(grep "^Panel Hole error: " "$UI_LOG" | tail -1)"
echo "ok   no failure notice"
ui_mark
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Added 1 hole(s) (simple), diameter 10" "the hole added" 30
ui_expect_volume "Body Body1 (F2.b0): volume 48000.000 -> \([0-9.]*\) mm3" \
  "$(awk 'BEGIN { printf "%.6f", 47135.94936549554 }')" "one hole, 10 deep"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
ui_stop_app

echo "=== Print Shape with and without a print bed face (mitcad#1744)"
# The block with a 6 mm hole lying in it, along +Y from its side at y 0;
# a copy with the block's top as its print bed face.
lying_block() {
  cat << EOF
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 60, "height": 40},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$RECT"}],
    "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "hole", "diameter": 6,
    "extent": {"type": "distance", "depth": 10},
    "placement": {"type": "plane", "plane": {"origin": [0, 0, 0], "normal": [0, -1, 0]},
      "points": [[30, 0, 10]]}}}
EOF
}
{ echo "["; lying_block; echo "]"; } > "$WORK/free.json"
{
  echo "["
  lying_block
  echo "  ,{\"cmd\": \"set_print_bed_face\", \"uid\": \"F2.b0\", \"face\": \"F2:end($RECT)\"}"
  echo "]"
} > "$WORK/bed.json"
for name in free bed; do
  "$CLI" run "$WORK/$name.json" --save "$WORK/$name.mitcad" > "$WORK/cli.log" 2>&1 ||
    { cat "$WORK/cli.log"; ui_fail "mitcad-cli $name"; }
done

# check description expectations...: the expectations (JSON objects) hold
# for the saved design.
check() {
  local description=$1 file=$2
  shift 2
  local IFS=,
  printf '[%s]\n' "$*" > "$WORK/check.json"
  "$CLI" run "$WORK/check.json" --open "$file" > "$WORK/check.log" 2>&1 ||
    { cat "$WORK/check.log"; ui_fail "$description"; }
  echo "ok   $description"
}

# roof_faces x z count: the block's planar faces whose outward normal is
# (x, 0, z), the number expected. A teardrop's roof along Y has two sides
# at 45 degrees; their normals point back into the hole, down from a roof
# that points up.
roof_faces() {
  printf '{"expect": {"query": {"query": "faces", "body": "F2.b0", "normal": [%s, 0, %s], "normal_tolerance": 0.001}, "result": {"total": %s}}}' \
    "$1" "$2" "$3"
}

# teardrop file: Hole1 of the design edited to a Teardrop and saved.
teardrop() {
  ui_start_app --open "$1"
  ui_step "fit (F6)"                       ui_key F6
  ui_expect_log "Bodies shown: F2.b0" "the design shown"
  ui_mark
  ui_step "double-click Hole1"             ui_double_click_logged "Timeline Hole1"
  ui_expect_new "Editing F3 with Hole" "Hole1 opens for editing" 30
  ui_expect_new "Hole placement kept: plane" "its plane kept as it is"
  ui_expect_new "Panel Hole input print_shape at" "Print Shape listed"
  grep "Panel Hole input print_shape at" <(tail -n +"$UI_MARK" "$UI_LOG") | tail -1 | grep -q "(hidden)" &&
    ui_fail "Print Shape is hidden"
  echo "ok   Print Shape shown"
  ui_mark
  ui_step "Teardrop"                       ui_choose "Panel Hole input print_shape" 1
  ui_expect_new "Hole: Print Shape = Teardrop" "Teardrop chosen"
  ui_expect_new "Preview Hole: ok" "previewed" 30
  ui_mark
  ui_step "OK (Enter)"                     ui_key Return
  ui_expect_new "Edited F3" "Hole1 edited" 30
  grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
  ui_mark
  ui_step "save (Ctrl+S)"                  ui_key ctrl+s
  ui_expect_new "Saved $1" "saved"
  ui_stop_app
}

echo "--- Without a print bed face: the design's -Z is down"
teardrop "$WORK/free.mitcad"
check "a teardrop without a print bed face, its roof toward +Z" "$WORK/free.mitcad" \
  '{"expect": {"feature": "F3", "status": "ok"}}' \
  '{"expect": {"query": {"query": "feature", "uid": "F3"}, "result": {"def": {"print_shape": "teardrop"}}}}' \
  "$(roof_faces 1 -1 1)" "$(roof_faces -1 -1 1)" "$(roof_faces 1 1 0)" "$(roof_faces -1 1 0)"

echo "--- With the block's top as its print bed face"
teardrop "$WORK/bed.mitcad"
check "a teardrop with the top as the print bed face, its roof away from it, toward -Z" "$WORK/bed.mitcad" \
  '{"expect": {"feature": "F3", "status": "ok"}}' \
  '{"expect": {"query": {"query": "feature", "uid": "F3"}, "result": {"def": {"print_shape": "teardrop"}}}}' \
  "$(roof_faces 1 1 1)" "$(roof_faces -1 1 1)" "$(roof_faces 1 -1 0)" "$(roof_faces -1 -1 0)"

ui_finish "UI Hole entry test"
