#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/build app/commands/BuildCommands.cpp app/framework app/library app/OcctViewer.cpp
# check-all sources: core/model/src/build core/model-build core/model-api/src core/vcs/src/library tools/cli tools/ui-timber-lib.sh
# The TIMBER group's Wall (mitcad#479, mitcad#520) through the real UI,
# with a small library of synthetic parts (tools/ui-timber-lib.sh: a 48 x
# 98 timber member and a wood screw):
#   1. Wall: a wall 3600 x 2400 from the origin along +X, studs at 600,
#      previewed with its members counted (the screws found in the
#      library).
#   2. The Openings window (mitcad#519) from Wall's Openings tile: Add
#      window (at 600, 900 x 1200, sill at 900), previewed once, as a
#      partition's (the default): king studs, one flat header, a sill and
#      cripple studs, no trimmers, its opening marked in the view; Add door
#      after it; a row typed outside the wall flagged on the row, then put
#      back; Load-bearing chosen once: trimmers and headers on edge.
#   3. Enter places the frame: one build_frame, its members and two screws
#      at each junction; the window closes with Wall.
#   4. Cut List: the frame's members with the other timber, grouped by size
#      (the studs and the king studs one row of seven). Timber Frame
#      (MODIFY) on the wall opens the window with its two openings.
#   5. In a new design, Corner Butt, the default (mitcad#510): a wall; after
#      Place nothing is previewed from the origin on top of it and Enter
#      places no second one there, the view asking where the next wall
#      starts (mitcad#513); then a wall started by a click on its side near
#      its end A (its end stud's side, a few centimetres from the corner),
#      previewed there, comes flush with that end, a clean corner
#      (mitcad#514); an Offset typed moves the preview once, nothing
#      following the mouse, and 0 brings it back flush; one by a click on
#      its end B's face (the view from the right) starts from that end, a
#      butt corner.
#   6. In a new design (mitcad#515): a wall, a wall flush with each of its
#      ends running the same way, then a wall started by a click on the
#      first of these two's side, its run meeting the other: no End
#      clicked, it previews and is placed between the two, butting the
#      other's side, as long as their faces' distance.
#   7. In a new design, Corner Lapped plates: a wall started near the
#      first one's end B makes an L corner (mitcad#493); the next Wall
#      start remembers Lapped plates.
#   8. A Start on another body's face (mitcad#524, the owner's case): a
#      design of a slab and a solid wall with a door (mitcad-cli); a click on
#      the wall's side face starts a wall square from it, on the slab's top;
#      on its end face a corner with Butt, in line with Straight (the
#      Corner's icon buttons), placed in line. Sections 5 to 7 are case A
#      (a Timber wall frame's side and end) as before.
#   9. A wall across a long room (mitcad#536, the owner's case): the room's
#      walls one body (mitcad-cli); a start on a long wall's inner face,
#      Length Auto: it previews and is placed as wide as the room, ending at
#      the opposite face of the same body, its plates longer than the
#      timber's longest spliced over studs; the inspection names what the
#      run met and the length, also when a height its studs cannot have
#      refuses the preview.
# Timber's loose members on a wall are tools/ui-timber-test.sh's, floors
# and roofs tools/ui-timber-floor-roof-test.sh's.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-timber-wall-test.sh

source "$(dirname "$0")/ui-test-lib.sh"
source "$(dirname "$0")/ui-timber-lib.sh"

timber_library ui-timber-wall

ui_start_display
# A new design (mitcad#394): Component1 active.
ui_start_app --no-native-dialogs

echo "--- 1. Wall: a wall from the origin"
start_wall
expect_inspect Wall "frame wall Wall 1, wall 3600 × 2400 mm, 9 member(s)" "1 bottom plate, 7 stud, 1 top plate" \
  "fasteners yes" "the wall from the origin along +X: plates and seven studs at 600, screws at the junctions"

echo "--- 2. The Openings window (mitcad#519): a window and a door"
ui_mark
ui_step "Openings (its tile)"              ui_click_logged "Panel Wall input frame_openings_window"
ui_expect_new "Openings window opened for build.wall: 0 row(s), non-load-bearing" \
  "the Openings window, a partition by default"
ui_expect_new "Openings place add_window at " "its buttons' places"
ui_focus_dialog '^Openings$'
ui_mark
ui_step "Add window"                       ui_click_logged "Openings place add_window"
ui_expect_new "Openings added a window as row 1" "a window's row"
ui_expect_new "Openings rows: 600 900x1200 900" "the row written into Wall's openings"
expect_inspect Wall "frame wall Wall 1, wall 3600 × 2400 mm, 13 member(s)" \
  "1 bottom plate, 2 cripple stud, 1 flat header, 2 king stud, 1 sill, 5 stud, 1 top plate" \
  "; openings 1, non-load-bearing, opening 1 marked" \
  "a partition's window: king studs, one flat header, a sill and cripple studs, no trimmers; marked in the view"
ui_expect_new "Preview marker Wall > opening 1 " "the selected row's opening marked in the view"
# Once per change: the preview of the row added is computed once.
sleep 1.5
ui_sync
previews=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -F "Inspect Wall: " | grep -cF "; openings 1, non-load-bearing")
[ "$previews" -eq 1 ] || ui_fail "the window's change previewed $previews times"
echo "ok   one preview for the change"
ui_mark
ui_step "Add door"                         ui_click_logged "Openings place add_door"
ui_expect_new "Openings added a door as row 2" "a door's row"
ui_expect_new "Openings rows: 600 900x1200 900; 2100 900x2100" "the door after the window, no sill"
expect_inspect Wall "frame wall Wall 1, wall 3600 × 2400 mm, 16 member(s)" "2 flat header" "4 king stud" \
  "; openings 2, non-load-bearing, opening 2 marked" "the door framed too: its own king studs and flat header"
# A row outside the wall is flagged on the row as it is typed.
ui_mark
ui_step "row 2 at 4000"                    ui_type_in "Openings place offset.2" 4000
ui_step "commit it (Enter)"                ui_key Return
ui_expect_new "Openings row 2: outside the wall (3600 mm long)" "the row outside the wall flagged"
ui_mark
ui_step "row 2 at 2100 again"              ui_type_in "Openings place offset.2" 2100
ui_step "commit it (Enter)"                ui_key Return
ui_expect_new "Openings rows ok" "the row fits again"
# The wall's kind switched once: the members change.
ui_mark
ui_step "Load-bearing"                     ui_choose "Openings place bearing" 1
ui_expect_new "Openings bearing: load-bearing" "the wall made load-bearing"
expect_inspect Wall "frame wall Wall 1, wall 3600 × 2400 mm, 22 member(s)" "4 header" "4 trimmer stud" \
  "; openings 2, load-bearing" "load-bearing: trimmer studs and headers of two plies on edge"

echo "--- 3. Place"
ui_focus_main
ui_mark
ui_step "Place (Enter)"                    ui_key Return
ui_expect_new "Placed frame Wall 1 (wall, lead C" "the wall frame placed" 60
line=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep "^Placed frame Wall 1 " | tail -1)
[[ $line == *": 22 member(s), "* ]] || ui_fail "the frame's members: $line"
fastened=$(sed -n 's/.*member(s), \([0-9]*\) junction(s) fastened$/\1/p' <<< "$line")
[ "${fastened:-0}" -gt 0 ] || ui_fail "no junction fastened: $line"
echo "ok   22 members with the window and the door, $fastened junction(s) with two screws each"
WALL1=$(largest_bodies) # its plates (mitcad#522: one a click picks is chosen)
ui_step "close Wall (Escape)"              ui_key Escape
ui_expect_new "Openings window closed with build.wall" "the window closes with the command"

echo "--- 4. Cut List"
ui_mark
ui_step "Cut List (command search)"        ui_command "Cut List"
ui_focus_dialog '^Cut List$'
ui_expect_new "Cut list row: " "the cut list's rows"
ui_expect_new "Cut list timber: " "the timber columns and totals"
ui_expect_new "Cut list timber row: Timber 48x98 test" "the members' material and sizes"
rows=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -E "^Cut list row: 7 x Timber 48x98 test L=2304, 2304(\.0*)? mm" || true)
[ -n "$rows" ] || ui_fail "no row of the seven 2304 mm studs: $(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep '^Cut list row: ')"
echo "ok   the studs and king studs one row of seven, by size"
ui_step "close the lists (Escape)"         ui_key Escape
ui_focus_main

echo "--- 4b. Timber Frame opens the Openings window with the wall's openings (mitcad#519)"
ui_mark
ui_step "Timber Frame (command search)"    ui_command "Timber Frame"
ui_expect_new "Command Timber Frame started" "Timber Frame started"
ui_step "fit (F6)"                         ui_key F6
ui_step "its Member"                       ui_click_logged "Panel Timber Frame input part"
ui_sync
ui_mark
PLATE=$(pickable "$WALL1")
[ -n "$PLATE" ] || ui_fail "no plate of the wall ($WALL1) to pick"
ui_step "a member of the wall"             ui_click_pick "$PLATE/"
expect_inspect "Timber Frame" "frame Wall 1 laid out again: 22 member(s)" "; openings 2, load-bearing" \
  "the wall's values, load-bearing as placed"
ui_mark
ui_step "Openings (its tile)"              ui_click_logged "Panel Timber Frame input frame_openings_window"
ui_expect_new "Openings window opened for build.frame: 2 row(s), load-bearing" \
  "the window with the wall's window and door"
ui_focus_main
ui_step "Cancel (Escape)"                  ui_key Escape
ui_expect_new "Openings window closed with build.frame" "the window closes with Timber Frame"

echo "--- 5. Butting walls (mitcad#510): from Wall 1's side near its end A, from its end B's face"
# A new design, the orientation cube's places logged (the view from the
# right for the end face).
ui_stop_app
export MITCAD_LOG_ORIENTATION_CUBE=1
ui_start_app --no-native-dialogs
start_wall
expect_inspect Wall "frame wall Wall 1, wall 3600 × 2400 mm" "; corner butt" "a wall, Corner Butt by default"
ui_mark
ui_step "Place (Enter)"                    ui_key Return
ui_expect_new "Placed frame Wall 1 (wall, lead C" "the wall frame placed" 60
WALL1=$(new_bodies)
STUDS1=$(stud_bodies)
# After Place no ghost (mitcad#513): the panel keeps its values, Start and
# End are empty, and nothing is previewed from the origin on top of Wall 1
# until a click picks the next start (wall_from: previewed there).
ui_expect_new "View hint Wall: Click where the next wall starts" "the view asks where the next wall starts"
expect_inspect Wall "failed: Click where the next wall starts." "the panel waits for the next wall's start"
no_ghost Wall "frame wall " "no wall previewed at the origin on top of Wall 1 after Place"
ui_mark
ui_step "Enter without a click"            ui_key Return
ui_expect_new "Wall: OK refused" "Enter places no second wall at the origin"
no_ghost Wall "frame wall " "still nothing previewed"
start_picks Wall
# The owner's case (mitcad#514): a click on its end stud's front face, a
# few centimetres from the corner (+X to the right in the view), comes
# flush with end A: its outer face in the plane of the end face.
side=$(stud_face "$STUDS1" first)
ui_mark
ui_step "the start near Wall 1's corner"   click_place "$side"
ui_expect_new "BUILD frame start click: From End A, Offset 0 mm (side)" "the click fills From and Offset: flush"
ui_expect_new "View hint Wall: Click where the wall ends, or press Enter: Length square to the wall clicked" \
  "the end next, else Length square to the wall"
ui_mark
ui_step "3000 mm long"                     ui_type_in "Panel Wall input frame_length" 3000
expect_inspect Wall "frame wall Wall 2, wall 3000 × 2400 mm" "; Wall 2: from Wall 1's side, flush with end A (butt)" \
  "; corner butt" "; length typed" "Wall 2 previewed flush with end A, the length typed"
# An Offset typed moves the preview, once: nothing follows the mouse.
ui_mark
ui_step "Offset 600"                       ui_type_in "Panel Wall input frame_offset" 600
ui_expect_new "BUILD frame offset typed: from End A" "the offset typed"
expect_inspect Wall "; Wall 2: from Wall 1's side, 600 mm from end A (butt)" "the preview 600 mm from end A"
ui_sync
ui_mark
ui_step "the mouse over the view"          mid_view
ui_step "the mouse a little further"       xdotool mousemove_relative -- 30 20
no_inspect Wall "the preview stays where the offset put it while the mouse moves"
ui_mark
ui_step "Offset 0"                         ui_type_in "Panel Wall input frame_offset" 0
expect_inspect Wall "; Wall 2: from Wall 1's side, flush with end A (butt)" "Offset 0: flush with end A again"
ui_mark
ui_step "Place (Enter)"                    ui_key Return
ui_expect_new "Placed frame Wall 2 (wall, lead C" "Wall 2 placed" 60
ui_expect_new "Frame junction: Wall 2: from Wall 1's side, flush with end A (butt)" "Wall 2 placed flush with end A"
# Seen from the right (+X), only Wall 1's end B shows of it: the plates'
# ends (too small to pick from the front right) first, else the end stud.
ui_step "the view from the right"          ui_click_logged "Orientation cube right"
start_picks Wall
end=$( { wall_faces ":end_b"; wall_faces ""; } | head -1)
wall_from "Wall 3" "$end" 2000 "from Wall 1's end B (butt corner)" butt
ui_step "close Wall (Escape)"              ui_key Escape

echo "--- 6. A wall whose run meets the wall opposite ends there (mitcad#515)"
# A new design: Wall 1 along +X, Wall 2 and Wall 3 flush with its ends,
# both 3000 mm to -Y (its front side), 3502 mm apart (centre lines).
ui_stop_app
ui_start_app --no-native-dialogs
start_wall
expect_inspect Wall "frame wall Wall 1, wall 3600 × 2400 mm" "; corner butt" "a wall, Corner Butt"
ui_mark
ui_step "Place (Enter)"                    ui_key Return
ui_expect_new "Placed frame Wall 1 (wall, lead C" "the wall frame placed" 60
STUDS1=$(stud_bodies)
start_picks Wall
wall_from "Wall 2" "$(stud_face "$STUDS1" first)" 3000 "from Wall 1's side, flush with end A (butt)" butt
# Wall 2's studs (and Wall 1's backing stud for it).
STUDS2=$(stud_bodies)
start_picks Wall
wall_from "Wall 3" "$(stud_face "$STUDS1" last)" 3000 "from Wall 1's side, flush with end B (butt)" butt
# A click on the +X face of Wall 2's middle stud: Wall 4 starts on Wall
# 2's side towards Wall 3, no End clicked, Length Auto: it ends butting
# Wall 3's side, from Wall 2's centre line 3502 - 49 = 3453 mm long, 3404
# mm between the faces.
start_picks Wall
side=$(stud_face "$STUDS2" middle)
ui_mark
ui_step "the start on Wall 2's side"       click_place "$side"
ui_expect_new "BUILD frame start click: From " "the click fills From and Offset"
ui_expect_new "View hint Wall: Click where the wall ends, or press Enter: to Wall 3's side" \
  "the end next, else to the wall the run meets"
expect_inspect Wall "frame wall Wall 4, wall 3453 × 2400 mm" "; Wall 4: from Wall 2's side, " \
  ", to Wall 3's side (butt), 3404 mm" "; length auto" "Wall 4 previewed between Wall 2 and Wall 3, no End clicked"
ui_mark
ui_step "Place (Enter)"                    ui_key Return
ui_expect_new "Placed frame Wall 4 (wall, lead C" "Wall 4 placed" 60
ui_expect_new ", to Wall 3's side (butt), 3404 mm" "Wall 4 placed butting Wall 3"
line=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep "^Frame junction: Wall 4: from Wall 2's side, " | tail -1)
[ -n "$line" ] || ui_fail "Wall 4 placed without its junction on Wall 2"
echo "ok   $line"
ui_step "close Wall (Escape)"              ui_key Escape

echo "--- 7. Lapped plates (mitcad#493, mitcad#510): an L corner near Wall 1's end B, remembered"
ui_stop_app
unset MITCAD_LOG_ORIENTATION_CUBE
ui_start_app --no-native-dialogs
start_wall
expect_inspect Wall "frame wall Wall 1, wall 3600 × 2400 mm" "; corner butt" "a wall, Corner Butt the first time"
ui_mark
ui_step "Corner: Lapped plates"            ui_click_logged "Panel Wall input frame_corner=lapped"
expect_inspect Wall "frame wall Wall 1, wall 3600 × 2400 mm" "; corner lapped" "Corner Lapped plates"
ui_mark
ui_step "Place (Enter)"                    ui_key Return
ui_expect_new "Placed frame Wall 1 (wall, lead C" "the wall frame placed" 60
WALL1=$(new_bodies)
start_picks Wall
# Its rightmost side face is near its end B.
side=$(wall_faces ":part(F2:side(" | tail -1)
wall_from "Wall 2" "$side" 3000 "an L corner at Wall 1's end B (lapped plates)" lapped
ui_step "close Wall (Escape)"              ui_key Escape
ui_mark
ui_step "Wall (command search)"            ui_command "Wall"
ui_expect_new "Command Wall started" "Wall started again"
expect_inspect Wall "frame wall Wall 3, " "; corner lapped" "the next start remembers Lapped plates"
ui_step "close Wall (Escape)"              ui_key Escape

echo "--- 8. A Start on another body's face (mitcad#524): a solid wall with a door on a slab"
# A slab 5000 x 4000 x 200 (F1), a solid wall 3000 x 200 x 2700 standing on
# it (F2) and a door 900 x 2100 cut through the wall only (F3).
ui_stop_app
FILE=$WORK/solid-wall.mitcad
cat > "$WORK/solid-wall.json" << 'EOF'
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [-1000, -2000],
    "length": 5000, "width": 4000, "height": 200, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, 0],
    "length": 3000, "width": 200, "height": 2700, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [1000, -10],
    "length": 900, "width": 220, "height": 2100, "operation": "cut", "participants": ["F2.b0"]}}
]
EOF
"$CLI" run "$WORK/solid-wall.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }
ui_start_app --open "$FILE" --no-native-dialogs
start_wall
# solid_face TEXT DESCRIPTION: a click on each face of the solid wall (F2.b0)
# a click on Start picks, Start picked again before each, until the
# preview's inspection has TEXT.
solid_face() {
  local at line start i=0
  local -a places
  start_picks Wall
  start=$(grep -n "Pick places:" "$UI_LOG" | tail -1 | cut -d: -f1)
  mapfile -t places < <(tail -n +"${start:-1}" "$UI_LOG" |
    sed -n -E 's/^Pick face F2\.b0\/[^ ]* at ([0-9]+),([0-9]+)$/\1 \2/p')
  [ "${#places[@]}" -gt 0 ] || ui_fail "no face of the solid wall to click"
  for at in "${places[@]}"; do
    [ $i -gt 0 ] && start_picks Wall
    i=$((i + 1))
    ui_mark
    ui_step "a face of the solid wall"     click_place "$at"
    for _ in $(seq 1 15); do
      line=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -F "Inspect Wall: " | tail -1)
      if [[ $line == *"$1"* ]]; then
        echo "ok   $2: $line"
        return
      fi
      sleep 0.2
    done
  done
  ui_fail "$2: no face of the solid wall gave '$1'"
}
# A side face: square from it on its outward (visible) side, on the slab's
# top, the solid wall not linked.
solid_face "from a face of " "a wall from a face of the solid wall"
expect_inspect Wall ", base on " "(z 200 mm)" "; corner butt" "on the slab's top, the bottom plate at z 200"
# Its end face: a corner with Butt, in line with Straight (the Corner's
# icon buttons), Lapped plates not offered there.
solid_face "from the end face of " "a corner on the solid wall's end face"
expect_inspect Wall "(butt corner)" ", base on " "Butt: its side against the end face"
ui_mark
ui_step "Corner: Straight"                 ui_click_logged "Panel Wall input frame_corner=straight"
expect_inspect Wall "in line beyond the end face of " "(straight)" "; corner straight" "Straight: in line beyond it"
ui_mark
ui_step "Corner: Butt"                     ui_click_logged "Panel Wall input frame_corner=butt"
expect_inspect Wall "from the end face of " "(butt corner)" "Butt again: the corner"
ui_mark
ui_step "Corner: Straight again"           ui_click_logged "Panel Wall input frame_corner=straight"
expect_inspect Wall "in line beyond the end face of " "Straight again"
ui_mark
ui_step "Place (Enter)"                    ui_key Return
ui_expect_new "Placed frame Wall 1 (wall, lead C" "the wall placed in line with the solid wall" 60
ui_expect_new "Frame junction: Wall 1: in line beyond the end face of " "placed in line beyond its end face"
ui_step "close Wall (Escape)"              ui_key Escape

echo "--- 9. The owner's case of mitcad#536: across a long room, Length Auto, its plates spliced"
# A slab (F1) and a room's walls as one body (F2): a box 10400 x 8219 x
# 2700 with the room, 10000 x 7819 inside, cut out of it (F3). A start on
# a long wall's inner face, Length Auto: the run meets the opposite wall,
# a face of the same body, 7819 mm on; 7819 mm is longer than the timber's
# longest (6000 mm), so each plate is two pieces of at most 4200 mm (its
# longest stock length), the joint over the stud at 4200.
ui_stop_app
FILE=$WORK/long-room.mitcad
cat > "$WORK/long-room.json" << 'EOF'
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [-700, -700],
    "length": 11400, "width": 9219, "height": 200, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [-200, -200],
    "length": 10400, "width": 8219, "height": 2700, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, 0],
    "length": 10000, "width": 7819, "height": 2800, "operation": "cut", "participants": ["F2.b0"]}}
]
EOF
"$CLI" run "$WORK/long-room.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }
ui_start_app --open "$FILE" --no-native-dialogs
start_wall
# The faces of the room's body clicked in turn until one runs across the
# room (a long wall's inner face; the others run 10000 mm or meet nothing).
solid_face "frame wall Wall 1, wall 7819 × 2400 mm" "a wall from a long wall's inner face, across the room"
expect_inspect Wall "frame wall Wall 1, wall 7819 × 2400 mm" "2 bottom plate" "2 top plate" \
  "; length auto" "; Length Auto: the run met a face of " ", the wall 7819 mm" \
  "; spliced top plate 4200, bottom plate 4200" \
  "previewed across the room, the run's end named, each plate two pieces over the stud at 4200"
expect_inspect Wall ", to a face of " ", 7819 mm" ", base on " "the junction: to the opposite wall's face, on the slab"
# Refused (its studs 6904 mm, longer than the timber's longest): the panel
# still names the wall, its length and what the run met.
ui_mark
ui_step "7000 mm high"                     ui_type_in "Panel Wall input frame_height" 7000
ui_expect_new "Frame preview refused: frame wall Wall 1, wall 7819 × 7000 mm, 0 member(s)" \
  "the refused preview names the wall and the length worked out"
line=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -F "Frame preview refused: " | tail -1)
[[ $line == *"; Length Auto: the run met a face of "* ]] || ui_fail "the refused preview without the run's end: $line"
[[ $line == *"longer than the part's longest"* ]] || ui_fail "the refused preview without its reason: $line"
echo "ok   refused, with what the run met and why"
ui_mark
ui_step "2400 mm high again"               ui_type_in "Panel Wall input frame_height" 2400
expect_inspect Wall "frame wall Wall 1, wall 7819 × 2400 mm" "; spliced top plate 4200, bottom plate 4200" \
  "2400 mm high again: previewed"
ui_mark
ui_step "Place (Enter)"                    ui_key Return
ui_expect_new "Placed frame Wall 1 (wall, lead C" "the wall placed across the room" 60
ui_expect_new "Frame spliced: top plate 4200, bottom plate 4200" "placed with its plates spliced"
line=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep "^Frame junction: Wall 1: from a face of " | tail -1)
[[ $line == *", to a face of "*", 7819 mm"* ]] || ui_fail "Wall 1 placed without the face its run met: $line"
echo "ok   $line"
ui_step "close Wall (Escape)"              ui_key Escape

ui_finish "UI timber wall test"
