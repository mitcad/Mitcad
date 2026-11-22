#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/ManipulatorOverlay.cpp app/framework/ManipulatorOverlay.hpp
# check-all sources: app/framework/ViewHintPlace.hpp app/framework/FloatingLayout.cpp app/view/ModeBanner.cpp
# check-all sources: app/commands/BuildCommands.cpp app/OcctViewer.cpp
# Where a command's view hint is drawn (mitcad#2190), through the real UI
# with Build's Profile and a synthetic profile made here (mitcad-cli; no
# supplier's files):
#   - Docked: the hint's pill 16 px below the view's top, where it always
#     was ("View hint at x,y WxH" against "View area x y w h");
#   - Floating: the first part's hint, and the corner hint of a placement
#     turned at a profile's end ("A corner: Miter Shared miters both
#     components on the bisector"), below the rows floating over the view's
#     top ("Floating cards: ... floating rows bottom N"), not under them.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-view-hint-place-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

ROOT=$(cd "$(dirname "$0")/.." && pwd)
CLI=${UI_CLI:-$(dirname "$UI_APP")/../tools/cli/mitcad-cli}
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-view-hint.XXXXXX")
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
export MITCAD_LIBRARIES_DIR=$WORK/cache
LIB=$WORK/test-profiles
g() { git -C "$1" -c user.name=T -c user.email=t@example.invalid "${@:2}" > /dev/null; }

# A 20 x 20 profile of slot 5 with the frames of a cut-to-length part, as
# tools/ui-build-test.sh makes it (docs/libraries.md, "BUILD parts").
RECT='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'
frame() {
  printf '  {"cmd": "add_feature", "name": "mount:%s", "def": {"type": "joint_origin", "geometry": "origin",
    "frame_override": {"origin": %s, "z_axis": %s, "x_axis": %s}}},\n' "$1" "$2" "$3" "$4"
}
"$CLI" library init "$LIB" --id test-profiles --name "Test profiles" --license MIT > /dev/null ||
  ui_fail "library init"
mkdir -p "$LIB/profiles"
{ printf '[\n  {"cmd": "add_parameter", "name": "L", "expression": "500 mm"},\n'
  printf '  {"cmd": "sketch.create", "plane": "xy"},\n'
  printf '  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [-10, -10], "width": 20, "height": 20},\n'
  printf '  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "%s"}],
    "extent": {"type": "distance", "distance": "L"}, "operation": "new_body"}},\n' "$RECT"
  frame end_a "[0, 0, 0]" "[0, 0, -1]" "[1, 0, 0]"
  printf '  {"cmd": "add_feature", "name": "mount:end_b", "def": {"type": "joint_origin",
    "geometry": {"body": "F2.b0", "face": "F2:end(%s)"}}},\n' "$RECT"
  frame "slot_x+_1" "[10, 0, 0]" "[1, 0, 0]" "[0, 0, 1]"
  frame "slot_x-_1" "[-10, 0, 0]" "[-1, 0, 0]" "[0, 0, 1]"
  frame "slot_y+_1" "[0, 10, 0]" "[0, 1, 0]" "[0, 0, 1]"
  frame "slot_y-_1" "[0, -10, 0]" "[0, -1, 0]" "[0, 0, 1]"
  printf '  {"expect": {"bodies": 1}}\n]\n'; } > "$WORK/p20-test.json"
"$CLI" run "$WORK/p20-test.json" --save "$LIB/profiles/p20-test.mitcad" > /dev/null || ui_fail "the profile"
SYSTEM='{"type": "I", "slot": 5, "grid": 20}'
cat > "$LIB/mitcad-library.json" <<EOF
{
  "format": "mitcad-library", "version": 1, "id": "test-profiles", "name": "Test profiles",
  "description": "A synthetic BUILD profile of the UI test.", "library_version": "1.0.0", "license": "MIT",
  "authors": ["Mitcad UI test"], "units": "mm",
  "categories": [{"id": "slot", "name": "Slot profile"}, {"id": "profiles", "name": "Profiles", "parent": "slot"}],
  "components": [
    {"id": "p20-test", "path": "profiles/p20-test.mitcad", "name": "Profile 20 test", "category": "profiles",
     "supplier": "Test supplier", "part_number": "P20-T", "designation": "Profile 20 test L={L}",
     "build": {"role": "profile", "system": $SYSTEM,
       "length": {"parameter": "L", "min": "10 mm", "max": "3000 mm", "stock": ["1000 mm"]},
       "section": {"width": "20 mm", "height": "20 mm"},
       "slot": {"depth": "6 mm", "opening": "5.2 mm", "inner_width": "11 mm"}, "mass_per_metre": "0.4 kg",
       "frames": [{"name": "end_a", "kind": "end"}, {"name": "end_b", "kind": "end"},
                  {"name": "slot_x+_1", "kind": "slot"}, {"name": "slot_x-_1", "kind": "slot"},
                  {"name": "slot_y+_1", "kind": "slot"}, {"name": "slot_y-_1", "kind": "slot"}]}}
  ]
}
EOF
g "$LIB" add -A
g "$LIB" commit -q -m "Test parts"
g "$LIB" tag v1.0.0
"$CLI" library fetch "$LIB" > /dev/null || ui_fail "library fetch"

# The body the placement logged last ("New body <name> (<uid>): volume ...").
placed_body() { sed -n 's/^New body .* (\(F[0-9]*\.b[0-9]*\)): volume.*/\1/p' "$UI_LOG" | tail -1; }
# faces_of BODY TEXT: the faces of a body a click can pick now (after the
# last "Pick places:"), whose names contain the text.
faces_of() {
  local start
  start=$(grep -n "Pick places:" "$UI_LOG" | tail -1 | cut -d: -f1)
  tail -n +"${start:-1}" "$UI_LOG" | grep -F "Pick face $1/" |
    sed -n 's/^Pick face [^/]*\/\(.*\) at [0-9]*,[0-9]*$/\1/p' | grep -F -- "$2"
}
# marker_at LABEL: the screen position of Profile's last preview marker
# LABEL (an extended regex): "Preview marker Profile <label> world x,y,z at x,y".
marker_at() {
  local at px py
  at=$(grep -E "^Preview marker Profile (> )?$1 world " "$UI_LOG" | tail -1 |
    sed -n 's/.* at \([0-9]*\),\([0-9]*\)$/\1 \2/p')
  [ -n "$at" ] || ui_fail "no preview marker '$1' of Profile logged"
  read -r px py <<< "$at"
  echo $((X + px * UI_SCALE)) $((Y + py * UI_SCALE))
}
# hint_top: the window y of the hint's pill as the overlay logged it last
# ("View hint at x,y WxH"; logged right after the hint's text).
hint_top() { grep -o '^View hint at [0-9]*,[0-9]*' "$UI_LOG" | tail -1 | sed 's/.*,//'; }
# hint_below_rows DESCRIPTION: in the Floating layout, the hint's pill below
# the rows over the view's top.
hint_below_rows() {
  local rows top
  rows=$(grep '^Floating cards: ' "$UI_LOG" | tail -1 | sed -n 's/.*floating rows bottom \([0-9]*\);.*/\1/p')
  top=$(hint_top)
  [ -n "$rows" ] && [ -n "$top" ] || ui_fail "$1: no rows' bottom ($rows) or hint place ($top) logged"
  [ "$top" -ge "$rows" ] || ui_fail "$1: the hint at y $top is under the floating rows (bottom $rows)"
  echo "ok   $1: the hint below the rows over the view (y $top, rows end at $rows)"
}

ui_start_display

echo "=== Docked"
ui_start_app --no-native-dialogs
ui_mark
ui_step "Profile (command search)"         ui_command "Profile"
ui_expect_new "Command Profile started" "Profile started"
ui_expect_new "View hint Profile: Click a plane or a face, or press Enter" "the view's hint for the first part"
ui_expect_new "View hint at " "the hint placed"
area=$(grep -o "View area [0-9]* [0-9]* [0-9]* [0-9]*" "$UI_LOG" | tail -1)
[ -n "$area" ] || ui_fail "the app did not log its view area"
read -r _ _ _ vy _ _ <<< "$area"
top=$(hint_top)
[ "$top" -eq $((vy + 16)) ] || ui_fail "Docked: the hint at y $top, not 16 px below the view's top ($vy)"
echo "ok   Docked: the hint 16 px below the view's top (y $top)"
ui_step "Close (Escape)"                   ui_key Escape
ui_crashed && ui_fail "the application crashed"
ui_stop_app

echo "=== Floating"
ui_start_app --no-native-dialogs --chrome=floating
ui_expect_log "Floating cards: " "the floating cards laid out"
ui_mark
ui_step "Profile (command search)"         ui_command "Profile"
ui_expect_new "Command Profile started" "Profile started"
ui_expect_new "View hint Profile: Click a plane or a face, or press Enter" "the view's hint for the first part"
ui_expect_new "View hint at " "the hint placed"
hint_below_rows "Floating, the first part's hint"
ui_step "60 mm long"                       ui_type_in "Panel Profile input length" "60"
ui_mark
ui_step "standing (Z)"                     ui_click_logged "Panel Profile input axis=z"
ui_expect_new "; along z" "the preview standing on its end"
ui_mark
ui_step "Place without a pick (Enter)"     ui_key Return
ui_expect_new "Placed Profile 20 test as " "placed at the origin by Enter" 30
B1=$(placed_body)
[ -n "$B1" ] || ui_fail "no body of the first profile"
ui_expect_new "View hint Profile: Click a profile's side or end" "the view's hint for the next one"
ui_step "fit the view (F6)"                ui_key F6
ui_step "Profile's pick again"             ui_click_logged "Panel Profile input at"
TOP0=$(faces_of "$B1" ":end_b" | head -1)
[ -n "$TOP0" ] || ui_fail "no end face of the first profile ($B1) to pick"
# The end face's pick place is in its inner half: straight on; its zone
# near an edge turns a corner (mitcad#417).
ui_mark
ui_step "its end face, in the middle"      ui_click_pick "$B1/$TOP0 at"
ui_expect_new "BUILD end click: straight towards " "a click in the middle of the end continues straight"
ui_expect_new "View hint Profile: Click near an edge of the end to turn a corner" "the straight placement's hint"
TURN=$(marker_at "turn .*")
ui_sync
ui_mark
ui_step "a click near an edge of the end"  xdotool mousemove $TURN click 1
ui_expect_new "BUILD end click: edge towards " "a click near an edge turns the part"
ui_expect_new "View hint Profile: A corner: Miter Shared miters both components on the bisector" "the corner's hint"
hint_below_rows "Floating, the corner's hint"
ui_step "Close (Escape)"                   ui_key Escape
ui_crashed && ui_fail "the application crashed"

ui_finish "UI view hint place test"
