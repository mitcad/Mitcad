#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/build app/commands/BuildCommands.cpp app/framework app/library app/OcctViewer.cpp
# check-all sources: app/MainWindowContextMenu.cpp core/model/src/build core/model-build core/model-api/src core/vcs/src/library
# check-all sources: tools/cli tools/ui-timber-lib.sh
# The TIMBER group's Floor / Roof (mitcad#479, mitcad#520) through the real
# UI, with a small library of synthetic parts (tools/ui-timber-lib.sh: a 48
# x 98 timber member and a wood screw):
#   1. Floor / Roof starts as a floor (its Kind's two buttons, no wall's
#      inputs): a floor 4000 x 3000 from the origin, rim joists all round
#      and joists at 400; Blocking adds a row; placed; Timber Frame (MODIFY)
#      on one of its joists shows the floor's values.
#   2. In a new design, three walls with Wall: Wall 1 along +X, Wall 2 and
#      Wall 3 flush with its ends, square to it and parallel. A plate of
#      Wall 2 and one of Wall 3 selected, the view's context menu offers
#      Floor / Roof first under BUILD (the selection names two walls); it
#      starts as a roof on them (the kind and the walls from the selection),
#      a gable roof previewed with its rafters and ridge board, and placed.
#      The next Floor / Roof start is a roof again (the kind used last).
#      Each frame a component of its own in Component1 (mitcad#522): Component1
#      holds none of their joints, the roof its nails'; Timber Frame on a
#      plate of Wall 1 lays it out again in the same component; the cut
#      list by frame. The roof with ring nails (the owner's case,
#      mitcad#521): every nail's shaft through both members of its
#      junction, its head on the entry face, none beside the roof.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-timber-floor-roof-test.sh

source "$(dirname "$0")/ui-test-lib.sh"
source "$(dirname "$0")/ui-timber-lib.sh"

timber_library ui-timber-floor-roof nail

# choose_in_picker CMD INPUT PART: the input's tile opens the part picker,
# a click on the part's tile there chooses it, Escape closes the picker.
choose_in_picker() {
  ui_mark
  ui_step "the $2 tile"                    ui_click_logged "Panel $1 input $2"
  ui_expect_new "Part picker opened: " "the part picker for $1's $2"
  ui_expect_new "Part picker tile $3 at " "$3's tile in the picker"
  ui_focus_dialog "^Component Picker - "
  ui_mark
  ui_step "$3 in the picker"               ui_click_logged "Part picker tile $3"
  ui_expect_new "Part picker chose $3" "$3 chosen in the picker"
  ui_step "close the picker (Escape)"      ui_key Escape
  ui_focus_main
}

ui_start_display
# A new design (mitcad#394): Component1 active.
ui_start_app --no-native-dialogs

echo "--- 1. Floor / Roof: a floor"
ui_mark
ui_step "the BUILD tab"                    ui_click_logged "Ribbon tab BUILD"
ui_expect_new "Ribbon BUILD/TIMBER at" "its TIMBER group"
ui_expect_new "Ribbon button build.floor_roof at" "the Floor / Roof button"
ui_mark
ui_step "Floor / Roof (its button)"        ui_click_logged "Ribbon button build.floor_roof"
ui_expect_new "Command Floor / Roof started" "Floor / Roof started"
ui_expect_new "BUILD floor_roof kind floor" "a floor the first time"
ui_expect_new "Panel Floor / Roof input frame_kind=roof at" "the Kind's Roof button"
ui_expect_log "Panel Floor / Roof input frame_kind=floor at" "the Kind's Floor button"
! tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qE "^Panel Floor / Roof input (frame_height|frame_corner|frame_top) at [0-9]+,[0-9]+$" ||
  ui_fail "Floor / Roof shows a wall's inputs"
echo "ok   no wall's inputs"
expect_inspect "Floor / Roof" "frame floor Floor 1, floor 4000 × 3000 mm" "4 rim joist" \
  "a floor from the origin: rim joists all round, joists between them"
ui_mark
ui_step "Blocking"                         ui_click_logged "Panel Floor / Roof input frame_blocking"
expect_inspect "Floor / Roof" "frame floor Floor 1, floor 4000 × 3000 mm" " blocking" "a row of blocking"
ui_mark
ui_step "Place (Enter)"                    ui_key Return
ui_expect_new "Placed frame Floor 1 (floor, lead C" "the floor placed" 60
# Its longest members (rim joists of one length): one a click picks is
# chosen when Timber Frame's Member takes picks.
JOISTS=$(largest_bodies)
[ -n "$JOISTS" ] || ui_fail "no body of the floor"
ui_step "close Floor / Roof (Escape)"      ui_key Escape
ui_mark
ui_step "Timber Frame (command search)"    ui_command "Timber Frame"
ui_expect_new "Command Timber Frame started" "Timber Frame started"
ui_step "fit (F6)"                         ui_key F6
ui_step "its Member"                       ui_click_logged "Panel Timber Frame input part"
ui_sync
JOIST=$(pickable "$JOISTS")
[ -n "$JOIST" ] || ui_fail "no face of the floor's members ($JOISTS) to pick"
ui_mark
ui_step "a member of the floor"            ui_click_pick "$JOIST/"
expect_inspect "Timber Frame" "frame Floor 1 laid out again: " "rim joist" "the floor's values in Timber Frame"
ui_expect_log "Panel Timber Frame input frame_blocking at" "the floor's Blocking"
ui_step "Cancel (Escape)"                  ui_key Escape

echo "--- 2. A roof on two walls from the view's context menu"
ui_stop_app
ui_start_app --no-native-dialogs
start_wall
ui_mark
ui_step "no fasteners"                     ui_click_logged "Panel Wall input frame_fasteners"
expect_inspect Wall "frame wall Wall 1, wall 3600 × 2400 mm" "fasteners no" "a wall from the origin, no fasteners"
ui_mark
ui_step "Place (Enter)"                    ui_key Return
ui_expect_new "Placed frame Wall 1 (wall, lead C" "Wall 1 placed" 60
# Each frame a component of its own in Component1 (mitcad#522).
ui_expect_new "Frame component Wall 1 (C" "Wall 1 a component of its own (mitcad#522)"
WALL1C=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | sed -n 's/^Frame component Wall 1 (\(C[0-9]*\)) in .*/\1/p' | tail -1)
ui_expect_new "Joints by component: Component1 0" "no joint of it in Component1"
# Wall 1's plates: the largest bodies made with it (both 3600 long).
PLATES1=$(largest_bodies)
STUDS1=$(stud_bodies)
start_picks Wall
ui_mark
wall_from "Wall 2" "$(stud_face "$STUDS1" first)" 3000 "from Wall 1's side, flush with end A (butt)" butt
ui_expect_new "Frame component Wall 2 (C" "Wall 2 a component of its own"
# Wall 2's plates are the longest new members (Wall 1 gets backing studs).
PLATE2=$(largest_body)
start_picks Wall
ui_mark
wall_from "Wall 3" "$(stud_face "$STUDS1" last)" 3000 "from Wall 1's side, flush with end B (butt)" butt
ui_expect_new "Frame component Wall 3 (C" "Wall 3 a component of its own"
PLATE3=$(largest_body)
[ -n "$PLATE2" ] && [ -n "$PLATE3" ] || ui_fail "no plates of Wall 2 and Wall 3"
ui_step "close Wall (Escape)"              ui_key Escape
# Timber Frame (MODIFY) on a member of Wall 1 lays it out again in place:
# in the same component (mitcad#522).
ui_mark
ui_step "Timber Frame (command search)"    ui_command "Timber Frame"
ui_expect_new "Command Timber Frame started" "Timber Frame started"
ui_step "fit (F6)"                         ui_key F6
ui_step "its Member"                       ui_click_logged "Panel Timber Frame input part"
ui_sync
# Of Wall 1's plates the one a click picks now (the bottom plate may be out
# of sight).
PLATE1=$(pickable "$PLATES1")
[ -n "$PLATE1" ] || ui_fail "no plate of Wall 1 ($PLATES1) to pick"
ui_mark
ui_step "a plate of Wall 1"                ui_click_pick "$PLATE1/"
expect_inspect "Timber Frame" "frame Wall 1 laid out again: " "Wall 1's values in Timber Frame"
ui_mark
ui_step "Spacing 400"                      ui_type_in "Panel Timber Frame input frame_spacing" "400 mm"
expect_inspect "Timber Frame" "frame Wall 1 laid out again: " "the studs at 400"
ui_mark
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Timber frame Wall 1: " "Wall 1 laid out again" 60
ui_expect_new "Frame component Wall 1 ($WALL1C) in Component1" "in its own component, the same one"
ui_expect_new "Joints by component: Component1 0" "still no joint in Component1"
ui_step "close Timber Frame (Escape)"      ui_key Escape
# Where a click picks a face of each plate: Offset Face's pick places.
ui_step "fit (F6)"                         ui_key F6
ui_step "offset face (search)"             ui_command "Offset Face"
ui_expect_log "Pick face $PLATE2/" "where a face of Wall 2's plate is picked"
ui_expect_log "Pick face $PLATE3/" "where a face of Wall 3's plate is picked"
AT2=$(ui_pick_at "face $PLATE2/")
AT3=$(ui_pick_at "face $PLATE3/")
ui_step "Escape (cancel it)"               ui_key Escape
ui_step "click Wall 2's plate"             xdotool mousemove $AT2 click 1
ui_step "Ctrl+click Wall 3's plate"        xdotool mousemove $AT3 keydown ctrl click 1 keyup ctrl
ui_mark
ui_step "right-click it"                   xdotool mousemove $AT3 click 3
ui_expect_new "Context menu Build: " "the context menu's BUILD"
menu=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep "^Context menu Build: " | tail -1)
[[ $menu == "Context menu Build: Floor / Roof"* || $menu == *"| Floor / Roof |"* ]] ||
  ui_fail "BUILD does not offer Floor / Roof first for two walls' members: $menu"
echo "ok   BUILD offers Floor / Roof for two walls: $menu"
ui_mark
ui_step "Build > Floor / Roof"             ui_menu_choose "Build" "Floor / Roof"
ui_expect_new "Command Floor / Roof started" "Floor / Roof started"
ui_expect_new "BUILD floor_roof kind roof, 2 item(s) selected before" "a roof on the two walls selected"
expect_inspect "Floor / Roof" "frame roof Roof 1, gable roof" "rafter" "ridge board" \
  "a gable roof on Wall 2 and Wall 3: rafters and a ridge board"
# The owner's case (mitcad#521): Fasteners on, the ring nail, sized from
# the members, two at each junction.
ui_sync
if tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -F "Inspect Floor / Roof: " | tail -1 | grep -q "fasteners no"; then
  ui_mark
  ui_step "Fasteners on"                   ui_click_logged "Panel Floor / Roof input frame_fasteners"
  expect_inspect "Floor / Roof" "frame roof Roof 1" "fasteners yes" "the roof with fasteners"
fi
choose_in_picker "Floor / Roof" fastener test-timber/ring-nail-test
ui_mark
ui_step "two at each junction"             ui_type_in "Panel Floor / Roof input fastener_count" "2"
expect_inspect "Floor / Roof" "frame roof Roof 1, gable roof" "fasteners yes" "fastener 2 × Ring nail test" \
  "the gable roof with two ring nails at each junction"
ui_mark
ui_step "Place (Enter)"                    ui_key Return
ui_expect_new "Placed frame Roof 1 (roof, lead C" "the roof placed" 60
line=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep "^Placed frame Roof 1 " | tail -1)
fastened=$(sed -n 's/.*member(s), \([0-9]*\) junction(s) fastened$/\1/p' <<< "$line")
[ "${fastened:-0}" -gt 0 ] || ui_fail "no junction of the roof fastened: $line"
ui_expect_new "Frame component Roof 1 (C" "Roof 1 a component of its own (mitcad#522)"
ui_expect_new "Joints by component: Component1 0, " "Component1 holds no joint, the roof its nails' joints"
joints=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep "^Joints by component: " | tail -1)
[[ $joints == *"Roof 1 $((2 * fastened))"* ]] || ui_fail "the roof's joints not in Roof 1: $joints"
echo "ok   $joints"
# No nail beside the roof (mitcad#521): each one's shaft through both
# members of its junction, its head on the entry face.
ui_expect_new "Fastener shaft " "the nails' shafts measured"
shafts=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep "^Fastener shaft ")
[ "$(printf '%s\n' "$shafts" | wc -l)" -eq $((2 * fastened)) ] || ui_fail "not two nails at each of $fastened junction(s): $shafts"
! printf '%s\n' "$shafts" | grep -v " through 2 of 2 junction member(s)$" ||
  ui_fail "a nail not through both members of its junction"
seats=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep '^Timber fastener .*: head top ')
! printf '%s\n' "$seats" | grep -v 'head top 0 mm from entry face$' || ui_fail "a nail's head off the entry face"
echo "ok   $((2 * fastened)) nails, each through both members of its junction, flush"
ui_step "close Floor / Roof (Escape)"      ui_key Escape
ui_step "Escape (clear the selection)"     ui_key Escape
# The cut list by frame (mitcad#522): each frame's members apart.
ui_mark
ui_step "Cut List (command search)"        ui_command "Cut List"
ui_focus_dialog '^Cut List$'
ui_expect_new "Cut list row: " "the cut list's rows"
for name in "Wall 1" "Wall 2" "Wall 3" "Roof 1"; do
  tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -q "^Cut list row: .*, frame $name$" ||
    ui_fail "no cut list row of $name: $(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep '^Cut list row: ')"
done
echo "ok   the cut list by frame: Wall 1, Wall 2, Wall 3, Roof 1"
ui_step "close the lists (Escape)"         ui_key Escape
ui_focus_main
ui_mark
ui_step "Floor / Roof (command search)"    ui_command "Floor"
ui_expect_new "Command Floor / Roof started" "Floor / Roof again"
ui_expect_new "BUILD floor_roof kind roof" "the kind used last: a roof"
ui_expect_new "View hint Floor / Roof: Click a member of each of the two walls the roof stands on" \
  "waiting for the walls"
ui_step "close Floor / Roof (Escape)"      ui_key Escape

ui_finish "UI timber floor and roof test"
