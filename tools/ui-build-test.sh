#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/build app/commands/BuildCommands.cpp app/framework app/library app/OcctViewer.cpp
# check-all sources: core/model/src/build core/model-build core/model-api/src core/vcs/src/library tools/cli
# The BUILD tab (mitcad#375) through the real UI, with a small library of
# synthetic BUILD parts made here (mitcad-cli: a 20 x 20 profile of I-type
# slot 5, a corner block, a panel, a rail, a carriage and a slider; no supplier's
# files), fetched from a local git repository:
#   1. The BUILD tab beside SOLID, its groups.
#   2. A new design, Component1 active (mitcad#394): Profile adds to the root
#      without activating it, no policy message; the panel starts with the
#      part, its length (the stock length) and the X / Y / Z buttons, the
#      view's hint; the first profile by a pick on the root's XY plane,
#      then (undone) by Enter at the origin, 60 mm long; then only
#      profiles' faces are picked and the planes are hidden; a click on its
#      side places one flush with an end and fills Offset and From (no
#      check box); a click in the middle of its end face continues it
#      straight (Mitre: cut the new part), the hover near an edge previews
#      nothing (mitcad#508), the click there turns it (a
#      corner on the end face with square ends, its mitre Auto: none;
#      Shared chosen mitres it at offset 0) and places it (mitcad#417,
#      #427); a brace on its side by Tilt (its foot cut by the tilt,
#      mitcad#444; the Tilt row only on a side); Close shows the planes
#      again.
#      The same panel in the Floating chrome (Placement's icon buttons once
#      a part is there, mitcad#427; a Shift + click on a side places the
#      next one at once, mitcad#451).
#   3. Profile on a design with only its root: the first profile on the
#      XY plane (typed length); the mouse on its side names only the face
#      a click picks, nothing previewed before the click, and once a side
#      is picked the preview stays there while the mouse moves along it,
#      off the parts and back (no inspection, its end A unchanged), a click
#      on another face moves it once (mitcad#508); the click's body as
#      long as its length (mitcad#431); two on its side flush with its ends by
#      clicks at the flush snaps (From End A and End B, Offset 0; for the
#      one at end B a typed offset stays from end B and From changed keeps
#      the part in place, mitcad#417); the fourth on the side of the
#      second at its far end, its length bound to the third (Tab steps
#      face, centre, through; the box at the cursor shows it; the preview's
#      body as long as the length in each, mitcad#431) - a frame of four.
#   4. Connector at a junction (shown on the hover, the chosen one
#      always), from the fitting connectors, previewed in place.
#   5. Panel in the frame from an inner face.
#   6. Linear Part: a rail on the first profile, a carriage on the rail,
#      a slider on two sides of a profile, each on the side
#      picked (mitcad#448; its hover names the side and previews nothing,
#      mitcad#508) (each chosen in the
#      part picker from the Part tile);
#      Ends: an end of the third profile mitred and bored (end machining).
#   7. Save, reopen: the parts are there (their sources in the design);
#      Cut List: its rows (one with a 45 degree end), Copy as CSV, Save as
#      PDF.
#   8. The part picker (mitcad#418), with a 30 profile of another system
#      in the library's next version: opened from Profile's Part tile
#      (mitcad#427), the groups as a tree (mitcad#527: Slot profile with its
#      Profiles, listing them, folded and unfolded by its arrow), a group,
#      a search, a click choosing the 30 profile that
#      the preview then uses, a star (first in its group), pinned it follows
#      Connector, where a click chooses the bracket; Profile again starts
#      with the 30 profile and opens the picker filtered to its system,
#      Favourites has the starred profile, In this design lists it after
#      its placement (on a plane; after Place nothing is previewed at the
#      origin on top of it, the panel waits for a click, mitcad#513).
#   9. The part a command starts with (mitcad#452), with a second 30
#      profile of the same system in the library's next version, starred
#      too: a new design starts with the first favourite (the picker's
#      first tile), the design that placed the other one with that one, a
#      new design again with the first favourite, which the picker shows.
#  10. Shift + click places at once (mitcad#451): on the XY plane the first
#      profile without Place, on its side the next one; a plain click only
#      previews; a Shift + click whose placement fails makes nothing and
#      shows the reason.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-build-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

ROOT=$(cd "$(dirname "$0")/.." && pwd)
CLI=${UI_CLI:-$(dirname "$UI_APP")/../tools/cli/mitcad-cli}
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-build.XXXXXX")
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
export MITCAD_LIBRARIES_DIR=$WORK/cache
LIB=$WORK/test-profiles
FILE=$WORK/frame.mitcad
PDF=$WORK/cut-list.pdf
g() { git -C "$1" -c user.name=T -c user.email=t@example.invalid "${@:2}" > /dev/null; }

RECT='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'
# frame NAME ORIGIN Z X: a mount frame (docs/libraries.md, "BUILD parts").
frame() {
  printf '  {"cmd": "add_feature", "name": "mount:%s", "def": {"type": "joint_origin", "geometry": "origin",
    "frame_override": {"origin": %s, "z_axis": %s, "x_axis": %s}}},\n' "$1" "$2" "$3" "$4"
}
# bar SIZE: a square bar of SIZE mm centred on the Z axis, extruded by L
# from z = 0, with the frames of a cut-to-length part.
bar() {
  local half
  half=$(awk -v s="$1" 'BEGIN { print s / 2 }')
  printf '[\n  {"cmd": "add_parameter", "name": "L", "expression": "500 mm"},\n'
  printf '  {"cmd": "sketch.create", "plane": "xy"},\n'
  printf '  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [-%s, -%s], "width": %s, "height": %s},\n' \
    "$half" "$half" "$1" "$1"
  printf '  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "%s"}],
    "extent": {"type": "distance", "distance": "L"}, "operation": "new_body"}},\n' "$RECT"
  frame end_a "[0, 0, 0]" "[0, 0, -1]" "[1, 0, 0]"
  printf '  {"cmd": "add_feature", "name": "mount:end_b", "def": {"type": "joint_origin",
    "geometry": {"body": "F2.b0", "face": "F2:end(%s)"}}},\n' "$RECT"
}
# block X Y Z (and more frames after): a box from the origin.
block() {
  printf '[\n  {"cmd": "add_feature", "def": {"type": "box", "plane": {"type": "origin", "plane": "xy"},
    "corner": [0, 0], "length": %s, "width": %s, "height": %s, "operation": "new_body"}},\n' "$1" "$2" "$3"
}
design() { # design FILE: the script on stdin saved as a design
  local script=$WORK/$(basename "$1" .mitcad).json
  cat > "$script"
  mkdir -p "$(dirname "$1")"
  "$CLI" run "$script" --save "$1" > /dev/null || ui_fail "mitcad-cli run $(basename "$script") failed"
}

"$CLI" library init "$LIB" --id test-profiles --name "Test profiles" --license MIT > /dev/null ||
  ui_fail "library init"
{ bar 20
  frame "slot_x+_1" "[10, 0, 0]" "[1, 0, 0]" "[0, 0, 1]"
  frame "slot_x-_1" "[-10, 0, 0]" "[-1, 0, 0]" "[0, 0, 1]"
  frame "slot_y+_1" "[0, 10, 0]" "[0, 1, 0]" "[0, 0, 1]"
  frame "slot_y-_1" "[0, -10, 0]" "[0, -1, 0]" "[0, 0, 1]"
  printf '  {"expect": {"bodies": 1}}\n]\n'; } | design "$LIB/profiles/p20-test.mitcad"
{ block 18 18 18
  frame mount_a "[9, 9, 0]" "[0, 0, -1]" "[1, 0, 0]"
  frame mount_b "[0, 9, 9]" "[-1, 0, 0]" "[0, 0, 1]"
  printf '  {"expect": {"bodies": 1}}\n]\n'; } | design "$LIB/connectors/bracket-20-test.mitcad"
{ printf '[\n  {"cmd": "add_parameter", "name": "W", "expression": "100 mm"},
  {"cmd": "add_parameter", "name": "H", "expression": "100 mm"},
  {"cmd": "add_parameter", "name": "t", "expression": "3 mm"},\n'
  # Centred on its XY origin: width along X, height along Y, thickness
  # along Z (commands.md, "Joints BUILD makes"). A box's corner takes
  # numbers only: the box from the origin (F1), moved back by half its
  # width and height (F2), so that W and H still size it about the origin.
  block 0 0 0 | tail -n +2 | sed 's/"length": 0, "width": 0, "height": 0/"length": "W", "width": "H", "height": "t"/'
  printf '  {"cmd": "add_feature", "def": {"type": "move", "bodies": ["F1.b0"],
    "transform": {"type": "translate_xyz", "x": "-W / 2", "y": "-H / 2", "z": 0}}},\n'
  frame face "[0, 0, 0]" "[0, 0, -1]" "[1, 0, 0]"
  printf '  {"expect": {"bodies": 1}}\n]\n'; } | design "$LIB/panels/panel-test.mitcad"
{ bar 15
  frame rail "[0, 7.5, 0]" "[0, 1, 0]" "[0, 0, 1]"
  # Its underside on a profile's slot: its first frame of kind face.
  frame base "[0, -7.5, 0]" "[0, -1, 0]" "[0, 0, 1]"
  printf '  {"expect": {"bodies": 1}}\n]\n'; } | design "$LIB/linear/rail-test.mitcad"
{ block 30 20 20
  frame rail "[15, 10, 0]" "[0, 0, -1]" "[1, 0, 0]"
  printf '  {"expect": {"bodies": 1}}\n]\n'; } | design "$LIB/linear/carriage-test.mitcad"
# A slider that slides in a slot (mitcad#448): its frame on its underside.
{ block 20 10 5
  frame slot "[10, 5, 0]" "[0, 0, -1]" "[1, 0, 0]"
  frame top "[10, 5, 5]" "[0, 0, 1]" "[1, 0, 0]"
  printf '  {"expect": {"bodies": 1}}\n]\n'; } | design "$LIB/linear/slider-test.mitcad"

SYSTEM='{"type": "I", "slot": 5, "grid": 20}'
cat > "$LIB/mitcad-library.json" <<EOF
{
  "format": "mitcad-library", "version": 1, "id": "test-profiles", "name": "Test profiles",
  "description": "Synthetic BUILD parts of the UI test.", "library_version": "1.0.0", "license": "MIT",
  "authors": ["Mitcad UI test"], "units": "mm",
  "categories": [{"id": "slot", "name": "Slot profile"}, {"id": "profiles", "name": "Profiles", "parent": "slot"},
                 {"id": "connectors", "name": "Connectors", "parent": "slot"},
                 {"id": "panels", "name": "Slot profile/Panels"}, {"id": "linear", "name": "Motion/Linear parts"}],
  "components": [
    {"id": "p20-test", "path": "profiles/p20-test.mitcad", "name": "Profile 20 test", "category": "profiles",
     "supplier": "Test supplier", "part_number": "P20-T", "designation": "Profile 20 test L={L}",
     "build": {"role": "profile", "system": $SYSTEM,
       "length": {"parameter": "L", "min": "10 mm", "max": "3000 mm", "stock": ["1000 mm"]},
       "section": {"width": "20 mm", "height": "20 mm"},
       "slot": {"depth": "6 mm", "opening": "5.2 mm", "inner_width": "11 mm"}, "mass_per_metre": "0.4 kg",
       "frames": [{"name": "end_a", "kind": "end"}, {"name": "end_b", "kind": "end"},
                  {"name": "slot_x+_1", "kind": "slot"}, {"name": "slot_x-_1", "kind": "slot"},
                  {"name": "slot_y+_1", "kind": "slot"}, {"name": "slot_y-_1", "kind": "slot"}]}},
    {"id": "bracket-20-test", "path": "connectors/bracket-20-test.mitcad", "name": "Bracket 20 test",
     "category": "connectors", "supplier": "Test supplier", "part_number": "B20-T",
     "build": {"role": "connector", "systems": [$SYSTEM],
       "frames": [{"name": "mount_a", "kind": "face"}, {"name": "mount_b", "kind": "face"}],
       "connector": {"junctions": ["L", "T"], "space": "inner_corner",
                     "mates": [{"frame": "mount_a", "to": "slot"}, {"frame": "mount_b", "to": "slot"}],
                     "fasteners": [], "machining": []}}},
    {"id": "panel-test", "path": "panels/panel-test.mitcad", "name": "Panel test", "category": "panels",
     "build": {"role": "panel", "frames": [{"name": "face", "kind": "face"}],
               "panel": {"mount": "slot", "thickness": "t", "width": "W", "height": "H", "clearance": "0.5 mm"}}},
    {"id": "rail-test", "path": "linear/rail-test.mitcad", "name": "Rail 15 test", "category": "linear",
     "supplier": "Test supplier", "part_number": "R15-T", "designation": "Rail 15 test L={L}",
     "build": {"role": "linear", "systems": [$SYSTEM],
       "length": {"parameter": "L", "min": "20 mm", "max": "2000 mm", "stock": ["1000 mm"]},
       "section": {"width": "15 mm", "height": "15 mm"},
       "frames": [{"name": "end_a", "kind": "end"}, {"name": "end_b", "kind": "end"}, {"name": "rail", "kind": "rail"},
                  {"name": "base", "kind": "face"}],
       "linear": {"kind": "rail"}}},
    {"id": "carriage-test", "path": "linear/carriage-test.mitcad", "name": "Carriage 15 test", "category": "linear",
     "supplier": "Test supplier", "part_number": "C15-T",
     "build": {"role": "linear", "frames": [{"name": "rail", "kind": "rail"}],
       "linear": {"kind": "carriage", "mate": {"frame": "rail", "on": "rail", "joint": "slider", "axis": "x"}}}},
    {"id": "slider-test", "path": "linear/slider-test.mitcad", "name": "Slider 5 test", "category": "linear",
     "supplier": "Test supplier", "part_number": "S5-T",
     "build": {"role": "linear", "systems": [$SYSTEM],
       "frames": [{"name": "slot", "kind": "slot"}, {"name": "top", "kind": "face"}],
       "linear": {"kind": "carriage", "mate": {"frame": "slot", "on": "slot", "joint": "slider", "axis": "x"}}}}
  ]
}
EOF
g "$LIB" add -A
g "$LIB" commit -q -m "Test parts"
g "$LIB" tag v1.0.0
"$CLI" library fetch "$LIB" > /dev/null || ui_fail "library fetch"
# A design with only its root, the active component.
printf '[]\n' > "$WORK/empty.json"
"$CLI" run "$WORK/empty.json" --save "$WORK/start.mitcad" > /dev/null || ui_fail "the empty design"

# Placed parts as the log names them: "Placed <part> as <name> (<occurrence>), ...".
placed_occurrence() { sed -n 's/^Placed .* as .* (\([O0-9/]*\)), length.*/\1/p' "$UI_LOG" | tail -1; }
placed_name() { sed -n 's/^Placed .* as \(.*\) ([O0-9/]*), length.*/\1/p' "$UI_LOG" | tail -1; }
# The body the placement logged last ("New body <name> (<uid>): volume ...").
placed_body() { sed -n 's/^New body .* (\(F[0-9]*\.b[0-9]*\)): volume.*/\1/p' "$UI_LOG" | tail -1; }
# faces_of BODY TEXT: the faces of a body a click can pick now (after the
# last "Pick places:"), whose names contain the text, one per line: a
# derived part's sides are "<F>:part(F2:side(c1[c4,c2]))", its ends
# "<F>:end_a", "<F>:end_b".
faces_of() {
  local start
  start=$(grep -n "Pick places:" "$UI_LOG" | tail -1 | cut -d: -f1)
  tail -n +"${start:-1}" "$UI_LOG" | grep -F "Pick face $1/" |
    sed -n 's/^Pick face [^/]*\/\(.*\) at [0-9]*,[0-9]*$/\1/p' | grep -F -- "$2"
}
pick_face() { ui_click_pick "$1/$2 at"; }
# shift_click X Y: a Shift + left click there (mitcad#451: the pick and
# Place in one); shift_pick_face BODY FACE likewise on a face.
shift_click() { xdotool mousemove "$1" "$2" keydown shift click 1 keyup shift; }
shift_pick_face() { shift_click $(ui_pick_at "$1/$2 at"); }
# no_placement DESCRIPTION: nothing placed since the mark, a moment later.
no_placement() {
  sleep 1
  ui_sync
  ! tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -q "^Placed " ||
    ui_fail "$1: $(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep "^Placed " | head -1)"
  echo "ok   $1"
}
# refresh_picks COMMAND: the view fitted to the parts, and the places a
# click picks logged again (the panel's pick input activated again).
refresh_picks() {
  ui_step "fit the view (F6)"              ui_key F6
  ui_step "$1's pick again"                ui_click_logged "Panel $1 input at"
}
SIDE=":part(F2:side("
# expect_row INPUT shown|hidden description: Profile's panel row INPUT as
# the panel laid it out last ("Panel Profile input <id> at x,y", with
# " (hidden)" when hidden).
expect_row() {
  local line=""
  for _ in $(seq 1 30); do
    line=$(grep -F "Panel Profile input $1 at " "$UI_LOG" | tail -1)
    if [ "$2" = hidden ]; then
      [[ $line == *"(hidden)" ]] && { echo "ok   $3"; return; }
    else
      [ -n "$line" ] && [[ $line != *"(hidden)" ]] && { echo "ok   $3"; return; }
    fi
    sleep 0.2
  done
  ui_fail "$3: Panel Profile input $1 is not $2 (${line:-never laid out})"
}
# choose_in_picker COMMAND PART: the panel's Part tile opens the part picker
# (mitcad#427), a click on the part's tile there chooses it, and Escape
# closes the picker again (it would cover the view's clicks).
choose_in_picker() {
  ui_mark
  ui_step "the Part tile"                  ui_click_logged "Panel $1 input part"
  ui_expect_new "$1: Part button" "the Part tile opens the part picker"
  ui_expect_new "Part picker opened: " "the part picker for $1"
  ui_expect_new "Part picker tile $2 at " "$2's tile in the picker"
  ui_focus_dialog "^Part Picker - $1\$"
  ui_mark
  ui_step "$2 in the picker"               ui_click_logged "Part picker tile $2"
  ui_expect_new "Part picker chose $2" "$2 chosen in the picker"
  ui_step "close the picker (Escape)"      ui_key Escape
  ui_focus_main
}
# marker_at LABEL: the screen position of Profile's last preview marker
# LABEL (an extended regex; also the taken one, "> LABEL"): "Preview marker
# Profile flush B world x,y,z at x,y". Its point is on the face picked, so
# a click there clicks that face (mitcad#417).
marker_at() {
  local at px py
  at=$(grep -E "^Preview marker Profile (> )?$1 world " "$UI_LOG" | tail -1 |
    sed -n 's/.* at \([0-9]*\),\([0-9]*\)$/\1 \2/p')
  [ -n "$at" ] || ui_fail "no preview marker '$1' of Profile logged"
  read -r px py <<< "$at"
  echo $((X + px * UI_SCALE)) $((Y + py * UI_SCALE))
}
# pick_markers X Y: the mouse moved out of the view to window point X, Y
# (above it, over the ribbon), and the pick's markers logged (the hover
# previews nothing, mitcad#508: the markers are the pick's wherever the
# mouse is).
pick_markers() {
  local inspect
  xdotool mousemove $((X + $1 * UI_SCALE)) $((Y + $2 * UI_SCALE))
  ui_sync >&2
  inspect=$(grep -n "^Inspect Profile: " "$UI_LOG" | tail -1 | cut -d: -f1)
  inspect=${inspect:-1}
  # The markers come after the inspection's line.
  for _ in $(seq 1 25); do
    tail -n +"$inspect" "$UI_LOG" | grep -q "^Preview marker Profile " && return
    sleep 0.1
  done
}
# marker_in_middle LABEL: as marker_at, once the marker lies in the middle
# half of the view, the view zoomed out step by step at its centre until
# then: fitted to the parts, a profile's end can come under the orientation
# cube in the view's corner, where a click turns the view instead of
# picking. Window coordinates of the view: "View area x y w h".
marker_in_middle() {
  local area vx vy vw vh cx cy at px py off last="" button=5 i
  area=$(grep -o "View area [0-9]* [0-9]* [0-9]* [0-9]*" "$UI_LOG" | tail -1)
  [ -n "$area" ] || ui_fail "the app did not log its view area"
  read -r _ _ vx vy vw vh <<< "$area"
  cx=$((vx + vw / 2))
  cy=$((vy + vh / 2))
  for i in $(seq 1 16); do
    pick_markers "$cx" $((vy / 2))
    at=$(grep -E "^Preview marker Profile (> )?$1 world " "$UI_LOG" | tail -1 |
      sed -n 's/.* at \([0-9]*\),\([0-9]*\)$/\1 \2/p')
    [ -n "$at" ] || ui_fail "no preview marker '$1' of Profile logged"
    read -r px py <<< "$at"
    # The larger of its offsets from the centre, in per mille of the view.
    off=$(( (px > cx ? px - cx : cx - px) * 1000 / vw ))
    [ $(( (py > cy ? py - cy : cy - py) * 1000 / vh )) -gt "$off" ] &&
      off=$(( (py > cy ? py - cy : cy - py) * 1000 / vh ))
    [ "$off" -lt 250 ] && { echo $((X + px * UI_SCALE)) $((Y + py * UI_SCALE)); return; }
    # A step that moved it away zoomed in: the other way then.
    if [ -n "$last" ] && [ "$off" -gt "$last" ]; then
      button=$((button == 5 ? 4 : 5))
    fi
    last=$off
    xdotool mousemove $((X + cx * UI_SCALE)) $((Y + cy * UI_SCALE)) click $button
    # The zoomed view drawn, its markers logged and the camera at rest: the
    # view's navigation and rest timers are watched (mitcad#870: a fixed
    # 0.5 s more before).
    ui_sync >&2 # (its note would end up in the position)
  done
  ui_fail "Profile's marker '$1' did not come into the middle of the view"
}
# expect_inspect TEXT... DESCRIPTION: an "Inspect Profile:" line since the
# mark has every TEXT.
expect_inspect() {
  local description=${!#} line text ok
  for _ in $(seq 1 30); do
    while IFS= read -r line; do
      ok=1
      for text in "${@:1:$#-1}"; do
        [[ $line == *"$text"* ]] || ok=0
      done
      [ $ok = 1 ] && { echo "ok   $description"; return; }
    done < <(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep "^Inspect Profile: ")
    sleep 0.2
  done
  ui_fail "$description: no 'Inspect Profile:' line since the mark with: ${*:1:$#-1}"
}
# expect_body_length DESCRIPTION: the last "Inspect Profile:" line since
# the mark that logs the preview body's length ("; body <n> mm",
# mitcad#431) has it equal to the length shown ("length <n> mm").
expect_body_length() {
  local line="" length body
  for _ in $(seq 1 30); do
    line=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep "^Inspect Profile: .*; body " | tail -1)
    [ -n "$line" ] && break
    sleep 0.2
  done
  [ -n "$line" ] || ui_fail "$1: no 'Inspect Profile:' line with the body's length since the mark"
  length=$(sed -n 's/^Inspect Profile: place [a-z]*, length \([0-9.]*\) mm.*/\1/p' <<< "$line")
  body=$(sed -n 's/.*; body \([0-9.]*\) mm.*/\1/p' <<< "$line")
  [ -n "$length" ] && [ "$length" = "$body" ] || ui_fail "$1: length ${length:-?} mm, body ${body:-?} mm ($line)"
  echo "ok   $1: $body mm"
}
# off_view: the mouse out of the view, over the ribbon above its middle
# (window coordinates of the view: "View area x y w h").
off_view() {
  local area vx vy vw vh
  area=$(grep -o "View area [0-9]* [0-9]* [0-9]* [0-9]*" "$UI_LOG" | tail -1)
  [ -n "$area" ] || ui_fail "the app did not log its view area"
  read -r _ _ vx vy vw vh <<< "$area"
  xdotool mousemove $((X + (vx + vw / 2) * UI_SCALE)) $((Y + vy / 2 * UI_SCALE))
}
# no_inspect COMMAND DESCRIPTION: a moment after the mouse moved, no
# "Inspect COMMAND:" line since the mark: the hover runs no placement and
# no inspection (mitcad#508).
no_inspect() {
  sleep 1.5
  ui_sync
  ! tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -q "^Inspect $1: " ||
    ui_fail "$2: $(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep "^Inspect $1: " | head -1)"
  echo "ok   $2"
}
# last_end_a COMMAND: where the last "Inspect COMMAND:" line puts the
# preview's end A ("; end A at x, y, z").
last_end_a() {
  grep "^Inspect $1: " "$UI_LOG" | tail -1 | sed -n 's/.*; end A at \([^;]*\);.*/\1/p'
}
# end_a_since COMMAND: the places of the preview's end A in the "Inspect
# COMMAND:" lines since the mark, each once.
end_a_since() {
  tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep "^Inspect $1: " | sed -n 's/.*; end A at \([^;]*\);.*/\1/p' | sort -u
}
# moved_once COMMAND WAS DESCRIPTION: after a click, the "Inspect COMMAND:"
# lines since the mark put the preview's end A elsewhere than WAS, all at
# one place: the preview moved once, to the click (mitcad#508).
moved_once() {
  local places=""
  for _ in $(seq 1 30); do
    places=$(end_a_since "$1")
    [ -n "$places" ] && break
    sleep 0.2
  done
  # A moment more: a preview still following the mouse would add places.
  sleep 1
  ui_sync
  places=$(end_a_since "$1")
  [ -n "$places" ] || ui_fail "$3: no 'Inspect $1:' line with end A since the mark"
  [ "$(printf '%s\n' "$places" | wc -l)" -eq 1 ] || ui_fail "$3: end A at more than one place: $(tr '\n' ' ' <<< "$places")"
  [ "$places" != "$2" ] || ui_fail "$3: end A still at $2"
  echo "ok   $3: end A at $places"
}
# shot NAME: with UI_BUILD_SHOTS set, a screenshot of the test display
# there as NAME.png.
shot() {
  [ -n "${UI_BUILD_SHOTS:-}" ] || return 0
  mkdir -p "$UI_BUILD_SHOTS"
  ui_sync
  if command -v import > /dev/null; then
    import -window root "$UI_BUILD_SHOTS/$1.png"
  else
    ui_capture "$UI_BUILD_SHOTS/$1.png"
  fi
  echo "note: screenshot $UI_BUILD_SHOTS/$1.png"
}
# no_policy_message: no design-policy refusal reached the log.
no_policy_message() {
  ! grep -q "does not allow" "$UI_LOG" || ui_fail "a policy refusal: $(grep "does not allow" "$UI_LOG" | head -1)"
  echo "ok   no policy message"
}

ui_start_display
# A new design (mitcad#394): Component1 active, the root with the Assembly policy.
ui_start_app --no-native-dialogs

echo "--- 1. The BUILD tab"
ui_expect_log "Ribbon tab BUILD at" "the BUILD tab beside SOLID"
ui_mark
ui_step "the BUILD tab"                    ui_click_logged "Ribbon tab BUILD"
ui_expect_new "Ribbon BUILD/PROFILE at" "its PROFILE group"
ui_expect_new "Ribbon BUILD/OUTPUT at" "its OUTPUT group"

echo "--- 2. The first profiles of a new design, Component1 active"
ui_mark
ui_step "Profile (command search)"         ui_command "Profile"
ui_expect_new "Command Profile started" "Profile started"
ui_expect_new "BUILD parts profile: 1 (0 in the design)" "the picker lists the library's profile"
ui_expect_new "BUILD profile adds to Component1 (a ground placement)" "BUILD adds to the active Component1 (Hybrid, mitcad#421)"
ui_expect_new "View hint Profile: Click a plane or a face, or press Enter" "the view's hint for the first part"
ui_expect_new "Inspect Profile: place grounded, length 1000.00 mm (typed), square ends; adds to Component1" \
  "at the origin with its stock length, adding to Component1"
expect_row axis=z shown "the Direction's X, Y and Z buttons"
ui_expect_new "Panel Profile tile part: Profile 20 test" "the Part tile shows the chosen profile (mitcad#427)"
expect_row search hidden "no Search row: the part picker has it"
expect_row system hidden "no System row: the part picker has it"
expect_row mitre hidden "no Mitre before an end pick"
expect_row offset hidden "no Offset before a side pick"
expect_row rotate hidden "no Rotate before another part is picked"
expect_row place hidden "no Placement before a part to go on (mitcad#427)"
expect_row tilt hidden "no Tilt before a side pick (mitcad#444)"
ui_mark
ui_step "on the root's XY plane"           ui_click_logged "Datum xy"
ui_expect_new "Inspect Profile: place grounded, length 1000.00 mm (typed), square ends; adds to Component1" \
  "on the XY plane, the root's plane picked from Component1: grounded at the click"
ui_step "Place (Enter)"                    ui_key Return
ui_expect_new "Placed Profile 20 test as " "placed by a pick on the XY plane" 30
no_policy_message
ui_step "Close (Escape)"                   ui_key Escape
ui_mark
ui_step "undo it (Ctrl+Z)"                 ui_key ctrl+z
ui_sync
ui_mark
ui_step "Profile again"                    ui_command "Profile"
ui_expect_new "BUILD profile adds to Component1 (a ground placement)" "a new design again"
ui_step "60 mm long"                       ui_type_in "Panel Profile input length" "60"
ui_mark
ui_step "along X"                          ui_click_logged "Panel Profile input axis=x"
ui_expect_new "Profile: Direction = Along X" "the X button"
expect_inspect "; along x" "; end A at 0.0, 0.0, 10.0" "the preview along X, resting on the XY plane (mitcad#492)"
ui_mark
ui_step "standing (Z)"                     ui_click_logged "Panel Profile input axis=z"
expect_inspect "; along z" "; end A at 0.0, 0.0, 0.0" "the preview standing on its end at the origin"
ui_mark
ui_step "Place without a pick (Enter)"     ui_key Return
ui_expect_new "Placed Profile 20 test as " "placed at the origin by Enter" 30
ui_expect_new "Applied Profile (1)" "the panel stays open"
no_policy_message
B1=$(placed_body)
[ -n "$B1" ] || ui_fail "no body of the first profile"
ui_expect_new "Profile Place On takes now: faces" "only a profile's faces once one is there"
ui_expect_new "Construction geometry hidden for the command" "the planes hidden for side and end placements"
ui_expect_new "View hint Profile: Click a profile's side or end" "the view's hint for the next one"
expect_row place=auto shown "Placement's icon buttons once a part is there (mitcad#427)"
expect_row length_mode=face shown "the length mode's icon buttons (mitcad#427)"
refresh_picks "Profile"
SIDE0=$(faces_of "$B1" "$SIDE" | head -1)
[ -n "$SIDE0" ] || ui_fail "no side of the first profile ($B1) to pick"
ui_mark
ui_step "its side, 60 mm long"             pick_face "$B1" "$SIDE0"
ui_expect_new "Inspect Profile: place side" "on the side"
ui_expect_new "; snap end_" "flush with the end near the click"
ui_expect_new "BUILD side click: From End " "the click fills From with the end it is flush with"
expect_row offset shown "Offset for a side pick (no check box)"
expect_row from shown "From for a side pick"
expect_row mitre hidden "no Mitre for a side pick"
expect_row rotate shown "Rotate against another part"
expect_row tilt shown "Tilt for a side pick (mitcad#444)"
ui_step "Place (Enter)"                    ui_key Return
ui_expect_new "Placed Profile 20 test as " "the second profile placed flush" 30
refresh_picks "Profile"
TOP0=$(faces_of "$B1" ":end_b" | head -1)
[ -n "$TOP0" ] || ui_fail "no end face of the first profile ($B1) to pick"
# The end face's pick place is in its inner half: straight on (mitcad#417).
ui_mark
ui_step "its end face, in the middle"      pick_face "$B1" "$TOP0"
ui_expect_new "Inspect Profile: place end" "on the end"
ui_expect_new "BUILD end click: straight towards " "a click in the middle of the end continues straight"
expect_inspect "; end straight towards " "; least snap " "the preview straight on, with the click's snap distance"
expect_row direction shown "Direction for an end pick"
expect_row mitre shown "Mitre for an end pick"
expect_row offset hidden "no side Offset for an end pick"
expect_row corner_offset hidden "no corner Offset straight on"
expect_row tilt hidden "no Tilt for an end pick (mitcad#444)"
ui_mark
ui_step "mitre the new part"               ui_choose "Panel Profile input mitre" 3
ui_expect_new "ends A 45 deg" "the new part's end mitred"
ui_mark
ui_step "the mitre Auto again"             ui_choose "Panel Profile input mitre" -3
expect_inspect "; end straight towards " "mitre none" "Auto: no mitre straight on"
# Near an edge of the end face (its zone's marker): the hover previews
# nothing, the part stays straight on (mitcad#508); the click there turns
# it, a corner (mitcad#417).
TURN=$(marker_at "turn .*")
[ -n "$TURN" ] || ui_fail "no zone of the end face marked"
ui_sync
ui_mark
ui_step "the mouse near an edge of the end" xdotool mousemove $TURN
no_inspect "Profile" "the hover near the edge previews and inspects nothing"
ui_mark
ui_step "a click there"                    xdotool click 1
ui_expect_new "BUILD end click: edge towards " "a click near an edge turns the part"
expect_inspect "; end edge towards " "mitre none, offset 0.0 mm" "square ends" \
  "a square corner on the end face, flush with the outer side: its mitre Auto: none (mitcad#427)"
expect_row corner_offset shown "the corner's Offset"
ui_mark
ui_step "the corner's offset 10"           ui_type_in "Panel Profile input corner_offset" "10"
expect_inspect "; end edge towards " "mitre none, offset 10.0 mm" "at another offset the mitre is none"
ui_mark
ui_step "the corner's offset 0"            ui_type_in "Panel Profile input corner_offset" "0"
expect_inspect "; end edge towards " "mitre none, offset 0.0 mm" "square ends" "on the end face again at offset 0"
ui_mark
ui_step "Mitre: Shared"                    ui_choose "Panel Profile input mitre" 2
expect_inspect "; end edge towards " "mitre shared, offset 0.0 mm" "ends A 45 deg" "Shared chosen: both on the bisector"
ui_step "Place (Enter)"                    ui_key Return
ui_expect_new "Placed Profile 20 test as " "the third profile placed at the corner, mitred" 30
no_policy_message
# A brace on the first one's side (mitcad#444): Tilt, its foot cut by the
# tilt to lie on the side.
refresh_picks "Profile"
SIDE0=$(faces_of "$B1" "$SIDE" | head -1)
[ -n "$SIDE0" ] || ui_fail "no side of the first profile ($B1) to pick for the brace"
ui_mark
ui_step "its side again"                   pick_face "$B1" "$SIDE0"
ui_expect_new "Inspect Profile: place side" "on the side"
expect_row tilt shown "Tilt for a side pick again"
ui_mark
ui_step "tilt 30 deg"                      ui_type_in "Panel Profile input tilt" "30 deg"
expect_inspect "Inspect Profile: place side, " "ends A 30 deg" "; tilt 30.00 deg" \
  "the preview tilted 30 deg, its foot cut by 30 deg"
ui_step "Place (Enter)"                    ui_key Return
ui_expect_new "Placed Profile 20 test as " "the brace placed" 30
no_policy_message
ui_mark
ui_step "Close (Escape)"                   ui_key Escape
ui_expect_new "Closed Profile after 4" "Close keeps them"
ui_expect_new "Construction geometry shown again" "the planes shown again when the command ends"
ui_stop_app

echo "--- 2b. The same panel in the Floating chrome"
ui_start_app --no-native-dialogs --chrome=floating
ui_mark
ui_step "Profile (command search)"         ui_command "Profile"
ui_expect_new "BUILD profile adds to Component1 (a ground placement)" "a new design: adds to Component1"
expect_row axis=x shown "the X button in a floating card"
expect_row place hidden "no Placement before a part to go on"
ui_mark
ui_step "Place without a pick (Enter)"     ui_key Return
ui_expect_new "Placed Profile 20 test as " "placed at the origin by Enter" 30
no_policy_message
expect_row place=side shown "Placement's icon buttons in a floating card (no More, mitcad#427)"
B2B=$(placed_body)
[ -n "$B2B" ] || ui_fail "no body of the profile placed in the floating card"
refresh_picks "Profile"
SIDE2B=$(faces_of "$B2B" "$SIDE" | head -1)
[ -n "$SIDE2B" ] || ui_fail "no side of the profile ($B2B) to pick in the floating card"
ui_mark
ui_step "Shift+click on its side"          shift_pick_face "$B2B" "$SIDE2B"
ui_expect_new "Profile Place On: Shift+click places at once" "a Shift+click in the floating chrome (mitcad#451)"
ui_expect_new "Placed Profile 20 test as " "placed on the side without Place" 30
ui_expect_new "Applied Profile (2)" "the floating card stays for the next one"
ui_step "Close (Escape)"                   ui_key Escape
ui_stop_app

# The rest on a design with only its root, the active component.
ui_start_app --open "$WORK/start.mitcad" --no-native-dialogs
ui_step "the BUILD tab"                    ui_click_logged "Ribbon tab BUILD"

echo "--- 3. Profile: a frame of four"
ui_mark
ui_step "Profile (command search)"         ui_command "Profile"
ui_expect_new "Command Profile started" "Profile started"
ui_expect_new "BUILD profile adds to Root (a ground placement)" "the root, the active component"
ui_step "a typed length of 400"            ui_type_in "Panel Profile input length" "400"
ui_expect_new "BUILD length typed: detached from the auto length" "typing a length detaches it"
ui_step "on the XY plane"                  ui_click_logged "Datum xy"
ui_expect_new "Inspect Profile: place " "the preview"
ui_mark
ui_step "Place (Enter)"                    ui_key Return
ui_expect_new "Placed Profile 20 test as " "the first profile placed" 30
ui_expect_new "Applied Profile (1)" "the panel stays open"
P1=$(placed_occurrence)
B1=$(placed_body)
[ -n "$P1" ] && [ -n "$B1" ] || ui_fail "no occurrence or body of the first profile"
echo "ok   the first profile is $P1 ($B1)"

# Two profiles on the first one's side, flush with its ends: offset 0 from
# A, and from B.
refresh_picks "Profile"
# The two on a side of the first one that a click can reach.
SIDE1=$(faces_of "$B1" "$SIDE" | head -1)
[ -n "$SIDE1" ] || ui_fail "no side of the first profile ($B1) to pick"
# The mouse on the side before a click names only the face a click picks:
# nothing previewed or inspected while it moves (mitcad#508).
ui_sync
ui_mark
ui_step "the mouse on the first profile's side" xdotool mousemove $(ui_pick_at "$B1/$SIDE1 at")
ui_expect_new "BUILD hover pick: face $SIDE1 of $B1" "the hover names the face a click picks"
ui_step "the mouse a little further"       xdotool mousemove_relative -- 6 0
no_inspect "Profile" "no preview and no inspection while hovering before the click"
# A click fills Offset and From (mitcad#417): a click at the flush-A snap
# (its marker) gives From End A, Offset 0. The preview's body as long as
# the length it shows (mitcad#431).
ui_mark
ui_step "the first profile's side"         pick_face "$B1" "$SIDE1"
ui_expect_new "Inspect Profile: place side" "on the side"
ui_expect_new "BUILD side click: From " "the click fills From and Offset"
expect_body_length "the click's body as long as its length"
# Once the place is picked the preview stays there (mitcad#508): the mouse
# along the side, off the parts and back runs no placement and no
# inspection, the preview's end A where the click put it; a click on
# another side moves it there once, a click on the first place back again.
ui_sync
END_A1=$(last_end_a "Profile")
[ -n "$END_A1" ] || ui_fail "no end A in the side pick's inspection"
ui_mark
ui_step "the mouse along the side"         xdotool mousemove_relative -- 12 0
ui_step "the mouse off the parts"          off_view
ui_expect_new "BUILD hover pick: off the parts' faces" "the hover off the parts"
ui_step "the mouse on the side again"      xdotool mousemove $(ui_pick_at "$B1/$SIDE1 at")
no_inspect "Profile" "no preview and no inspection while the mouse moves after the click"
[ "$(last_end_a "Profile")" = "$END_A1" ] || ui_fail "the preview's end A moved from $END_A1"
echo "ok   the preview's end A still at $END_A1"
OTHER1=$(faces_of "$B1" "$SIDE" | grep -vxF -- "$SIDE1" | head -1)
[ -n "$OTHER1" ] || OTHER1=$(faces_of "$B1" ":end_b" | head -1)
[ -n "$OTHER1" ] || ui_fail "no other face of the first profile ($B1) to click"
ui_mark
ui_step "a click on another face"          pick_face "$B1" "$OTHER1"
moved_once "Profile" "$END_A1" "a click on $OTHER1 moves the preview once"
ui_mark
ui_step "the first profile's side again"   pick_face "$B1" "$SIDE1"
ui_expect_new "BUILD side click: From " "the click on the side again"
ui_sync
[ "$(last_end_a "Profile")" = "$END_A1" ] || ui_fail "the click on the side again: end A not at $END_A1"
echo "ok   the click on the side again: end A at $END_A1"
expect_row offset shown "Offset for a side pick (no check box)"
expect_row from shown "From for a side pick"
ui_step "length 300"                       ui_type_in "Panel Profile input length" "300"
ui_mark
FLUSH_A=$(marker_in_middle "flush A")
[ -n "$FLUSH_A" ] || ui_fail "no flush A marker in the middle of the view"
ui_step "a click flush with end A"         xdotool mousemove $FLUSH_A click 1
ui_expect_new "BUILD side click: From End A, Offset 0 mm (end_a)" "flush with end A: From End A, Offset 0"
expect_inspect "; snap end_a at 0.0 mm from a" "the preview flush with end A"
ui_step "Place (Enter)"                    ui_key Return
ui_expect_new "Placed Profile 20 test as " "the second profile placed" 30
P2=$(placed_occurrence)
B2=$(placed_body)
refresh_picks "Profile"
# Flush with end B by a click, then typed offsets stay from end B; From
# changed keeps the part where it is (L 400, width 20).
ui_mark
ui_step "the first profile's side again"   pick_face "$B1" "$SIDE1"
ui_expect_new "Inspect Profile: place side" "on the side"
ui_mark
FLUSH_B=$(marker_in_middle "flush B")
[ -n "$FLUSH_B" ] || ui_fail "no flush B marker in the middle of the view"
ui_step "a click flush with end B"         xdotool mousemove $FLUSH_B click 1
ui_expect_new "BUILD side click: From End B, Offset 0 mm (end_b)" "flush with end B: From End B, Offset 0"
expect_inspect "; snap end_b at 0.0 mm from b" "the preview flush with end B"
ui_mark
ui_step "offset 30"                        ui_type_in "Panel Profile input offset" "30"
ui_expect_new "BUILD side offset typed: from end B" "a typed offset keeps From"
expect_inspect " at 30.0 mm from b" "30 mm from end B"
ui_mark
ui_step "from end A"                       ui_choose "Panel Profile input from" -1
ui_expect_new "BUILD From End A keeps the part in place: Offset 350 mm" "From End A keeps the part in place"
expect_inspect " at 350.0 mm from a" "350 mm from end A: the same place"
ui_mark
ui_step "from the middle"                  ui_choose "Panel Profile input from" 2
ui_expect_new "BUILD From Middle keeps the part in place: Offset 160 mm" "From Middle keeps the part in place"
expect_inspect " at 160.0 mm from middle" "160 mm from the middle: the same place"
ui_mark
ui_step "from end B again"                 ui_choose "Panel Profile input from" -1
ui_expect_new "BUILD From End B keeps the part in place: Offset 30 mm" "From End B again"
ui_mark
ui_step "offset 0"                         ui_type_in "Panel Profile input offset" "0"
expect_inspect " at 0.0 mm from b" "offset 0 from end B: flush with end B"
ui_step "Place (Enter)"                    ui_key Return
ui_expect_new "Placed Profile 20 test as " "the third profile placed" 30
P3=$(placed_occurrence)
B3=$(placed_body)
P3NAME=$(placed_name)
echo "ok   the second and third profiles are $P2 ($B2) and $P3 ($B3, $P3NAME), on $SIDE1"

# The fourth: on the second one's side that faces the third, at its far
# end, its length bound to the third.
ui_step "the auto length again (Tab)"      ui_key Tab
ui_expect_new "Profile: Tab steps Length" "Tab steps the auto length"
refresh_picks "Profile"
found=""
mapfile -t FACES2 < <(faces_of "$B2" "$SIDE")
for face in "${FACES2[@]}"; do
  ui_mark
  ui_step "the second profile's $face"     pick_face "$B2" "$face"
  for _ in $(seq 1 25); do
    tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -q "^Inspect Profile: " && break
    sleep 0.2
  done
  # Bound to the third: "(bound, auto: to the face to <its face> of <name>)" or "... to <name>)".
  inspect=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep "^Inspect Profile: " | tail -1)
  if [[ $inspect == *"(bound, "* && $inspect == *" $P3NAME)"* ]]; then
    found=$face
    break
  fi
done
[ -n "$found" ] || ui_fail "no side of the second profile a click reaches has its auto length to the third"
echo "ok   $found of the second profile runs to the third: bound"
# The preview's body ends at the third profile, in each mode (mitcad#431).
expect_body_length "the body to the third profile's face"
shot "431-bound-face"
ui_mark
ui_step "to the centre (Tab)"              ui_key Tab
ui_expect_new "Profile: Length = Auto: to its centre" "Tab: to the centre"
ui_expect_new "(bound, auto: to the centre to " "bound to the third profile's centre"
expect_body_length "the body to the third profile's centre"
shot "431-bound-centre"
ui_mark
ui_step "through (Tab)"                    ui_key Tab
ui_expect_new "Profile: Length = Auto: through it" "Tab: through"
ui_expect_new "(bound, auto: through to " "bound through the third profile"
expect_body_length "the body through the third profile"
shot "431-bound-through"
ui_step "to the face (Tab)"                ui_key Tab
ui_expect_new "Profile: Length = Auto: to the face it meets" "Tab: to the face again"
ui_expect_new "Cursor box Profile length: " "the auto length in the box at the cursor"
ui_expect_new "(bound, auto: to the face to " "bound to the third profile's face"
ui_step "Place (Enter)"                    ui_key Return
ui_expect_new "Placed Profile 20 test as " "the fourth profile placed, bound" 30
B4=$(placed_body)
ui_expect_new "length bound, auto: to the face" "its length is bound"
ui_mark
ui_step "Close (Escape)"                   ui_key Escape
ui_expect_new "Closed Profile after 4" "Close keeps the four profiles"

echo "--- 4. Connector"
ui_step "fit the view (F6)"                ui_key F6
ui_mark
ui_step "Connector (command search)"       ui_command "Connector"
ui_expect_new "Command Connector started" "Connector started"
ui_expect_new "BUILD connectors for junction " "the connectors of a junction asked"
ui_expect_new "Inspect Connector: junction " "the junction and its connectors"
ui_expect_new ", preview Bracket 20 test with " "the chosen connector previewed in place"
ui_expect_new "Panel Connector tile part: Bracket 20 test" "the chosen connector shown as a picture tile (mitcad#427)"
ui_expect_new "BUILD preview image of test-profiles/bracket-20-test: " "its picture asked of the library"
ui_mark
FACE3=$(faces_of "$B3" "$SIDE" | head -1)
[ -n "$FACE3" ] || ui_fail "no side of the third profile ($B3) to hover"
ui_step "the mouse over the third profile" xdotool mousemove $(ui_pick_at "$B3/$FACE3 at")
for _ in $(seq 1 25); do
  tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qE "^Preview marker Connector [^>]" && break
  sleep 0.2
done
tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qE "^Preview marker Connector [^>]" ||
  ui_fail "no junction of the third profile shown on its hover"
echo "ok   the hovered profile's junctions shown"
ui_expect_new "Preview marker Connector > " "the junctions marked, the chosen one first"
ui_mark
ui_step "Connect (Enter)"                  ui_key Return
ui_expect_new "Connected Bracket 20 test at junction " "the connector placed" 30
ui_step "Close (Escape)"                   ui_key Escape

echo "--- 5. Panel"
ui_mark
ui_step "Panel (command search)"           ui_command "Panel"
ui_expect_new "BUILD parts panel: 1" "the picker lists the panel"
ui_step "the frame's inner face"           pick_face "$B1" "$SIDE1"
ui_expect_new "Inspect Panel: panel Panel test" "the panel's preview"
ui_mark
ui_step "Place (Enter)"                    ui_key Return
ui_expect_new "Placed panel Panel test as " "the panel placed" 30
ui_step "Close (Escape)"                   ui_key Escape

echo "--- 6. Linear Part: a rail, a carriage and a slider"
ui_mark
ui_step "Linear Part (command search)"     ui_command "Linear Part"
ui_expect_new "BUILD parts linear: 3" "the rail, the carriage and the slider"
choose_in_picker "Linear Part" "test-profiles/rail-test"
ui_step "a typed length of 200"            ui_type_in "Panel Linear Part input length" "200"
# Another side of the first profile than the frame's, else that one.
RAILFACE=$(faces_of "$B1" "$SIDE" | grep -vxF -- "$SIDE1" | head -1)
[ -n "$RAILFACE" ] || RAILFACE=$SIDE1
ui_step "the first profile's $RAILFACE"   pick_face "$B1" "$RAILFACE"
ui_expect_new "Inspect Linear Part: place " "the rail's preview"
ui_mark
ui_step "Place (Enter)"                    ui_key Return
ui_expect_new "Placed Rail 15 test as " "the rail placed" 30
BRAIL=$(placed_body)
choose_in_picker "Linear Part" "test-profiles/carriage-test"
refresh_picks "Linear Part"
RAILTOP=$(faces_of "$BRAIL" "$SIDE" | grep -F "side(c3[" | head -1)
[ -n "$RAILTOP" ] || RAILTOP=$(faces_of "$BRAIL" "$SIDE" | head -1)
[ -n "$RAILTOP" ] || ui_fail "no side of the rail ($BRAIL) to pick"
ui_mark
ui_step "the rail's $RAILTOP"              pick_face "$BRAIL" "$RAILTOP"
ui_expect_new "Inspect Linear Part: " "the carriage's preview"
ui_step "Place (Enter)"                    ui_key Return
ui_expect_new "Placed Carriage 15 test as " "the carriage placed on the rail" 30
# A slider on two sides of a profile (mitcad#448): each on the side
# picked, as a profile on a side, not on the profile's first frame.
choose_in_picker "Linear Part" "test-profiles/slider-test"
refresh_picks "Linear Part"
# Two sides of a profile that a click can reach: the first profile's sides
# are mostly covered (the panel, the rail and the carriage), so of the
# profiles with two side pick places, the rail's last.
BSLIDER=""
for body in "$B2" "$B3" "$B4" "$B1"; do
  [ -n "$body" ] || continue
  SLIDERFACES=$(faces_of "$body" "$SIDE")
  if [ "$(printf '%s\n' "$SLIDERFACES" | grep -c .)" -ge 2 ]; then
    BSLIDER=$body
    break
  fi
done
[ -n "$BSLIDER" ] || ui_fail "no profile ($B1, $B2, $B3, $B4) with two sides to pick"
SLIDE1=$(printf '%s\n' "$SLIDERFACES" | sed -n 1p)
SLIDE2=$(printf '%s\n' "$SLIDERFACES" | sed -n 2p)
ui_sync
ui_mark
ui_step "the mouse on $BSLIDER's $SLIDE1"  xdotool mousemove $(ui_pick_at "$BSLIDER/$SLIDE1 at")
ui_expect_new "BUILD hover pick: face $SLIDE1 of $BSLIDER" "the hover names the side a click picks"
no_inspect "Linear Part" "the hover previews no slider (mitcad#508)"
ui_mark
ui_step "$BSLIDER's $SLIDE1"               pick_face "$BSLIDER" "$SLIDE1"
ui_expect_new "Inspect Linear Part: place side" "the slider on the side picked (On the side)"
ui_step "Place (Enter)"                    ui_key Return
ui_expect_new "Placed Slider 5 test as " "the first slider placed" 30
refresh_picks "Linear Part"
ui_mark
ui_step "$BSLIDER's $SLIDE2"               pick_face "$BSLIDER" "$SLIDE2"
ui_expect_new "Inspect Linear Part: place side" "the slider on the other side picked (On the side)"
ui_step "Place (Enter)"                    ui_key Return
ui_expect_new "Placed Slider 5 test as " "the second slider placed" 30
ui_step "Close (Escape)"                   ui_key Escape

echo "--- 6b. Ends: an end of the third profile mitred and bored"
ui_step "fit the view (F6)"                ui_key F6
ui_mark
ui_step "Ends (command search)"            ui_command "Ends"
ui_expect_new "Command Ends started" "Ends started"
END3=$({ faces_of "$B3" ":end_b"; faces_of "$B3" ":end_a"; faces_of "$B3" "$SIDE"; } | head -1)
[ -n "$END3" ] || ui_fail "no face of the third profile ($B3) to pick"
ui_step "the third profile's $END3"        pick_face "$B3" "$END3"
ui_expect_new "Inspect Ends: " "the part's ends"
ui_step "a bore in the end"                ui_choose "Panel Ends input machining" 1
ui_mark
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Ends of " "the end edited"
ui_expect_new "machining D6.8" "with its bore"

echo "--- 7. Save, reopen, Cut List"
ui_mark
ui_step "save as (Ctrl+Shift+S)"           ui_key ctrl+shift+s
ui_type_path "Save As" "$FILE"
ui_focus_main
ui_expect_new "Saved $FILE" "saved"
ui_stop_app
ui_start_app --open "$FILE" --no-native-dialogs
ui_expect_log "Opened $FILE" "reopened"
ui_mark
ui_step "Profile (command search)"         ui_command "Profile"
ui_expect_new "BUILD parts profile: 1 (1 in the design)" "the profile's source is in the design"
ui_step "Close (Escape)"                   ui_key Escape
ui_mark
ui_step "Cut List (command search)"        ui_command "Cut List"
ui_focus_dialog '^Cut List$'
ui_expect_new "Cut list row: " "the cut list's rows"
ui_expect_new "x Profile 20 test" "the profiles in it"
ui_expect_new " 45 deg (" "the mitred end"
ui_expect_new "x Rail 15 test" "the rail in it"
ui_expect_new "machining D6.8" "the bored end"
rows=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | sed -n 's/^Cut list: \([0-9]*\) row(s).*/\1/p' | tail -1)
[ "${rows:-0}" -ge 3 ] || ui_fail "the cut list has ${rows:-no} rows, not three or more"
echo "ok   $rows rows: the profiles by length and ends, and the rail"
ui_mark
ui_step "Copy as CSV (Alt+C)"              ui_key alt+c
ui_expect_new "Build list CSV: Quantity,Part,Length (mm),End A,End B,Machining,Own features,Supplier,Part number,Mass each (kg)" \
  "the CSV's header"
ui_expect_new ",Test supplier,P20-T," "the profiles' supplier and part number"
ui_expect_new "Build list copied as CSV: " "copied"
ui_step "Save as PDF (Alt+P)"              ui_key alt+p
ui_type_path "Save Cut List as PDF" "$PDF"
ui_focus_dialog '^Cut List$'
ui_expect_new "Build list PDF written: $PDF (" "the PDF written"
ui_expect_new "Cut list of frame" "with the design's name"
[ "$(head -c 5 "$PDF")" = "%PDF-" ] || ui_fail "$PDF is no PDF"
echo "ok   $PDF is a PDF"
ui_step "close (Escape)"                   ui_key Escape
ui_focus_main
ui_stop_app

echo "--- 8. The part picker (mitcad#418)"
# A 30 x 30 profile of I-type slot 6 on a 30 mm grid in the library's next
# version, fetched before the design is opened again.
{ bar 30
  frame "slot_x+_1" "[15, 0, 0]" "[1, 0, 0]" "[0, 0, 1]"
  frame "slot_x-_1" "[-15, 0, 0]" "[-1, 0, 0]" "[0, 0, 1]"
  frame "slot_y+_1" "[0, 15, 0]" "[0, 1, 0]" "[0, 0, 1]"
  frame "slot_y-_1" "[0, -15, 0]" "[0, -1, 0]" "[0, 0, 1]"
  printf '  {"expect": {"bodies": 1}}\n]\n'; } | design "$LIB/profiles/p30-test.mitcad"
cat > "$WORK/p30-entry.json" <<'EOF'
    {"id": "p30-test", "path": "profiles/p30-test.mitcad", "name": "Profile 30 test", "category": "profiles",
     "supplier": "Test supplier", "part_number": "P30-T", "designation": "Profile 30 test L={L}",
     "build": {"role": "profile", "system": {"type": "I", "slot": 6, "grid": 30},
       "length": {"parameter": "L", "min": "10 mm", "max": "3000 mm", "stock": ["1000 mm"]},
       "section": {"width": "30 mm", "height": "30 mm"},
       "slot": {"depth": "8 mm", "opening": "6.2 mm", "inner_width": "16 mm"}, "mass_per_metre": "0.8 kg",
       "frames": [{"name": "end_a", "kind": "end"}, {"name": "end_b", "kind": "end"},
                  {"name": "slot_x+_1", "kind": "slot"}, {"name": "slot_x-_1", "kind": "slot"},
                  {"name": "slot_y+_1", "kind": "slot"}, {"name": "slot_y-_1", "kind": "slot"}]}},
EOF
awk 'NR == FNR { entry = entry $0 "\n"; next }
     /"id": "bracket-20-test"/ && !done { printf "%s", entry; done = 1 }
     { print }' "$WORK/p30-entry.json" "$LIB/mitcad-library.json" |
  sed 's/"library_version": "1.0.0"/"library_version": "1.1.0"/' > "$WORK/manifest-1.1.json"
mv "$WORK/manifest-1.1.json" "$LIB/mitcad-library.json"
g "$LIB" add -A
g "$LIB" commit -q -m "A 30 profile"
g "$LIB" tag v1.1.0
"$CLI" library fetch "$LIB" > /dev/null || ui_fail "library fetch of v1.1.0"
ui_start_app --open "$FILE" --no-native-dialogs
ui_expect_log "Opened $FILE" "reopened"
ui_step "the BUILD tab"                    ui_click_logged "Ribbon tab BUILD"
# expect_picker_place TEXT description: the picker logged where TEXT is
# since the mark ("Part picker tile <part> at x,y; in the picker x,y").
expect_picker_place() { ui_expect_new "$1 at " "$2"; }

ui_mark
ui_step "Profile (command search)"         ui_command "Profile"
ui_expect_new "BUILD parts profile: 2 (1 in the design)" "the 30 profile listed beside the design's"
ui_expect_new "Panel Profile tile part: " "the Part tile (mitcad#427)"
ui_mark
ui_step "Placement: on a plane"            ui_click_logged "Panel Profile input place=plane"
expect_inspect "place grounded" "; part Profile " "a ground placement of the chosen profile"
ui_mark
ui_step "the part picker (the Part tile)"  ui_click_logged "Panel Profile input part"
ui_expect_new "Profile: Part button" "the Part tile opens the picker"
ui_expect_new "Part picker opened: 2 parts for build.profile" "the picker opened with the two profiles"
ui_expect_new "Part picker in this design: " "the design's library parts read"
ui_expect_new "test-profiles/p20-test x" "the placed 20 profiles in this design, with their count"
expect_picker_place "Part picker tile test-profiles/p30-test" "the tiles' places"
ui_focus_dialog '^Part Picker - Profile$'
# The groups as a tree (mitcad#527): Slot profile (a category the others
# name as their parent) with its Profiles; a click on it lists its
# subgroups' parts; folded, its subgroup's row has no place, and unfolded
# it has one again.
ui_expect_new "Part picker groups: " "the groups logged"
GROUPS_TREE=$(grep "^Part picker groups: " "$UI_LOG" | tail -1)
# Each with its count (mitcad#1015).
[[ $GROUPS_TREE == *"All parts (2), Slot profile (2), Slot profile/Profiles (2)" ]] ||
  ui_fail "the groups are '$GROUPS_TREE', not Slot profile with its Profiles, two profiles each"
expect_picker_place "Part picker place group fold Slot profile" "the fold's place"
ui_mark
ui_step "the group Slot profile"           ui_click_logged "Part picker group row Slot profile"
ui_expect_new "Part picker group Slot profile: 2 parts" "the main group lists its subgroup's profiles"
ui_mark
ui_step "fold Slot profile (its arrow)"    ui_click_logged "Part picker place group fold Slot profile"
ui_expect_new "Part picker group Slot profile folded" "folded"
ui_mark
# The same place as the click before: apart from it (ui_apart), or Qt makes
# the two a double-click, which the tree takes as no click on the arrow.
ui_step "unfold it (its arrow)" \
  eval 'ui_apart $(ui_logged_at "Part picker place group fold Slot profile");
        ui_click_logged "Part picker place group fold Slot profile"'
ui_expect_new "Part picker group Slot profile unfolded" "unfolded"
expect_picker_place "Part picker group row Slot profile/Profiles" "its subgroup's row again"
ui_mark
ui_step "the group Profiles"               ui_click_logged "Part picker group row Slot profile/Profiles"
ui_expect_new "Part picker group Slot profile/Profiles: 2 parts" "the group of profiles"
# The first tile's place, where the 30 profile is to be once the search
# leaves only it. The picker logs its places 150 ms after the tiles change,
# after the sync key's answer (PartPicker::logPlacesLater): a line of the
# tiles before the search (the 30 profile second) can come after the mark,
# and a click there would miss.
FIRST_TILE=$(grep -F "Part picker tile test-profiles/p20-test at " "$UI_LOG" | tail -1 |
  sed -n 's/.* at \([0-9]*,[0-9]*\);.*/\1/p')
[ -n "$FIRST_TILE" ] || ui_fail "no place of the first tile (the 20 profile)"
ui_mark
ui_step "search p30 (Alt+S)"               eval 'ui_key alt+s; ui_sync; xdotool type --delay 40 "p30"'
ui_expect_new "Part picker search 'p30'" "the picker's search"
ui_expect_new "Part picker group Slot profile/Profiles: 1 parts, first test-profiles/p30-test" "only the 30 profile"
ui_expect_new "Part picker tile test-profiles/p30-test at $FIRST_TILE;" "its tile's place, the first"
ui_mark
ui_step "a click on the 30 profile"        ui_click_logged "Part picker tile test-profiles/p30-test"
ui_expect_new "Part picker chose test-profiles/p30-test" "chosen in the picker"
expect_inspect "place grounded" "; part Profile 30 test" "the preview uses the chosen profile at once"
ui_mark
ui_step "clear the picker's search"        eval 'ui_key alt+s; ui_key ctrl+a; ui_key BackSpace'
ui_expect_new "Part picker group Slot profile/Profiles: 2 parts" "both profiles again"
# A star changes only the star (mitcad#517): the tiles and the groups stay
# where they are and the selection stays on the starred tile; the order
# applies when the list is made again.
ui_expect_new "Part picker tiles: " "the tiles' order logged"
TILES_BEFORE=$(grep "^Part picker tiles: " "$UI_LOG" | tail -1 | sed 's/^Part picker tiles: //')
# The groups by name: the kept line has no counts (mitcad#1015).
GROUPS_BEFORE=$(grep "^Part picker groups: " "$UI_LOG" | tail -1 | sed 's/^Part picker groups: //; s/ ([0-9]*)//g')
# The 20 profile (with its uses in the design) then the 30 profile.
[[ $TILES_BEFORE == "test-profiles/p20-test"*", test-profiles/p30-test" ]] ||
  ui_fail "the tiles before the star are '$TILES_BEFORE', not the 20 profile then the 30 profile"
ui_mark
ui_step "star it (Star)"                   ui_click_logged "Part picker place star"
ui_expect_new "Part picker star p30-test on" "the 30 profile starred"
ui_expect_new "Part picker tiles kept: $TILES_BEFORE; current test-profiles/p30-test" \
  "the tiles stay in their places, the selection on the starred tile"
ui_expect_new "Part picker groups kept: $GROUPS_BEFORE" "the groups stay in their places"
tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -q "^Part picker group Slot profile/Profiles: " &&
  ui_fail "a star made the tiles again"
ui_mark
ui_step "All parts (the group row)"        ui_click_logged "Part picker group row All parts"
ui_expect_new "Part picker group All parts: " "another group"
ui_mark
ui_step "Profiles again (the group row)"   ui_click_logged "Part picker group row Slot profile/Profiles"
ui_expect_new "Part picker group Slot profile/Profiles: 2 parts, first test-profiles/p30-test" "the starred profile first now"
# A group's star changes only the star too: its row stays where it is.
GROUPS_BEFORE=$(grep "^Part picker groups: " "$UI_LOG" | tail -1 | sed 's/^Part picker groups: //; s/ ([0-9]*)//g')
ui_mark
ui_step "star the group (its star)"        ui_click_logged "Part picker place group star Slot profile/Profiles"
ui_expect_new "Part picker group star slot profile/profiles on" "the group Profiles starred"
ui_expect_new "Part picker groups kept: $GROUPS_BEFORE" "the groups stay in their places"
ui_mark
ui_step "its star off again"               ui_click_logged "Part picker place group star Slot profile/Profiles"
ui_expect_new "Part picker group star slot profile/profiles off" "the group's star off"
ui_expect_new "Part picker groups kept: $GROUPS_BEFORE" "the groups stay in their places"
ui_mark
ui_step "pin the picker"                   ui_click_logged "Part picker place pin"
ui_expect_new "Part picker pinned on" "pinned"
ui_focus_main
ui_mark
ui_step "close Profile (Escape)"           ui_key Escape
ui_expect_new "Part picker stays pinned after build.profile" "the pinned picker stays when Profile ends"

ui_mark
ui_step "Connector (command search)"       ui_command "Connector"
ui_expect_new "Command Connector started" "Connector started"
ui_expect_new "Part picker follows build.connector: " "the pinned picker follows Connector"
expect_picker_place "Part picker tile test-profiles/bracket-20-test" "the fitting connector's tile"
ui_focus_dialog '^Part Picker - Connector$'
ui_mark
ui_step "a click on the bracket"           ui_click_logged "Part picker tile test-profiles/bracket-20-test"
ui_expect_new "Part picker chose test-profiles/bracket-20-test" "chosen in the picker"
ui_expect_new "Connector: Connector = Bracket 20 test" "chosen in Connector"
ui_mark
ui_step "unpin the picker"                 ui_click_logged "Part picker place pin"
ui_expect_new "Part picker pinned off" "unpinned"
ui_focus_main
ui_mark
ui_step "close Connector (Escape)"         ui_key Escape
ui_expect_new "Part picker closed with build.connector" "the picker closes with the command"

ui_mark
ui_step "Profile again"                    ui_command "Profile"
ui_expect_new "BUILD parts profile: 2 (1 in the design)" "Profile again"
ui_expect_new "Panel Profile tile part: Profile 30 test" "the profile chosen last in the Part tile"
ui_mark
ui_step "Placement: on a plane"            ui_click_logged "Panel Profile input place=plane"
expect_inspect "place grounded" "; part Profile 30 test" "a ground placement of the 30 profile"
ui_mark
ui_step "the part picker again (the tile)" ui_click_logged "Panel Profile input part"
ui_expect_new "Part picker opened: 2 parts for build.profile" "the picker opened again"
ui_expect_new "Part picker group All parts: 1 parts, first test-profiles/p30-test" \
  "filtered to the system chosen last, I6 30 (mitcad#427)"
expect_picker_place "Part picker group row Favourites" "the groups' places"
ui_focus_dialog '^Part Picker - Profile$'
ui_mark
ui_step "Favourites"                       ui_click_logged "Part picker group row Favourites"
ui_expect_new "Part picker group Favourites: 1 parts, first test-profiles/p30-test" "Favourites has the starred profile"
ui_focus_main
ui_mark
ui_step "Place (Enter)"                    ui_key Return
ui_expect_new "Placed Profile 30 test as " "the 30 profile placed" 30
# After Place no ghost (mitcad#513): Placement stays On a plane, but the
# next one is not previewed at the origin on top of the one placed there;
# the panel waits for a click.
expect_inspect "failed: Click a plane or a face where the next one goes." "after Place the panel waits for a click"
ui_expect_new "View hint Profile: Click a plane or a face for the next one" "the view asks for the next one's plane"
sleep 1.5
ui_sync
! tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -q "^Inspect Profile: place grounded" ||
  ui_fail "a profile previewed at the origin after Place: $(tail -n +$((UI_MARK + 1)) "$UI_LOG" |
    grep "^Inspect Profile: place grounded" | head -1)"
echo "ok   no profile previewed at the origin on top of the one placed"
ui_expect_new "test-profiles/p30-test x1" "In this design has it after the placement"
expect_picker_place "Part picker group row In this design" "the groups' places again"
ui_focus_dialog '^Part Picker - Profile$'
ui_mark
ui_step "In this design"                   ui_click_logged "Part picker group row In this design"
ui_expect_new "Part picker group In this design: 1 parts, first test-profiles/p30-test" \
  "In this design lists the placed profile (System I6 30)"
ui_focus_main
ui_mark
ui_step "Close (Escape)"                   ui_key Escape
ui_expect_new "Part picker closed with build.profile" "the picker closes with Profile"

ui_stop_app

echo "--- 9. A new design starts with the first favourite (mitcad#452)"
# A second profile of the 30 system in the library's next version, starred
# beside the 30 profile starred in 8: two favourites of one system.
cp "$LIB/profiles/p30-test.mitcad" "$LIB/profiles/p30h-test.mitcad"
sed 's/p30-test/p30h-test/g; s/Profile 30 test/Profile 30 heavy test/g; s/P30-T/P30H-T/' \
  "$WORK/p30-entry.json" > "$WORK/p30h-entry.json"
awk 'NR == FNR { entry = entry $0 "\n"; next }
     /"id": "bracket-20-test"/ && !done { printf "%s", entry; done = 1 }
     { print }' "$WORK/p30h-entry.json" "$LIB/mitcad-library.json" |
  sed 's/"library_version": "1.1.0"/"library_version": "1.2.0"/' > "$WORK/manifest-1.2.json"
mv "$WORK/manifest-1.2.json" "$LIB/mitcad-library.json"
g "$LIB" add -A
g "$LIB" commit -q -m "A heavy 30 profile"
g "$LIB" tag v1.2.0
"$CLI" library fetch "$LIB" > /dev/null || ui_fail "library fetch of v1.2.0"
# part_name ID: the test library's name of a 30 profile.
part_name() {
  case $1 in
    p30-test) echo "Profile 30 test" ;;
    p30h-test) echo "Profile 30 heavy test" ;;
    *) ui_fail "no 30 profile '$1'" ;;
  esac
}
# A new design (no file); the stars of 8 are kept in the settings.
ui_start_app --no-native-dialogs
ui_mark
ui_step "Profile (command search)"         ui_command "Profile"
ui_expect_new "BUILD parts profile: 3 (0 in the design)" "the three profiles of the library"
ui_expect_new "BUILD profile starts with test-profiles/p30-test: the first favourite" \
  "a new design starts with the starred profile, the only favourite"
ui_expect_new "Panel Profile tile part: Profile 30 test" "its tile"
ui_mark
ui_step "the part picker (the Part tile)"  ui_click_logged "Panel Profile input part"
ui_expect_new "Part picker opened: 3 parts for build.profile" "the picker with the three profiles"
ui_expect_new "Part picker shows the part chosen: test-profiles/p30-test" "the picker shows the part chosen"
ui_expect_new "Part picker tile test-profiles/p30h-test at " "the heavy profile's tile"
ui_focus_dialog '^Part Picker - Profile$'
ui_mark
ui_step "the heavy profile"                ui_click_logged "Part picker tile test-profiles/p30h-test"
ui_expect_new "Part picker chose test-profiles/p30h-test" "chosen in the picker"
TILES_BEFORE=$(grep "^Part picker tiles: " "$UI_LOG" | tail -1 | sed 's/^Part picker tiles: //')
ui_mark
ui_step "star it (Star)"                   ui_click_logged "Part picker place star"
ui_expect_new "Part picker star p30h-test on" "a second favourite"
# The star changes only the star (mitcad#517): the tiles stay in their
# places, the selection on the starred one; the favourites come first once
# the list is made again (Favourites, then All parts).
ui_expect_new "Part picker tiles kept: $TILES_BEFORE; current test-profiles/p30h-test" \
  "the tiles stay where they are, the selection on the starred tile"
ui_mark
ui_step "Favourites (the group row)"       ui_click_logged "Part picker group row Favourites"
ui_expect_new "Part picker group Favourites: 2 parts, first test-profiles/p30" "the two favourites"
# The first favourite in the picker's order (the catalog's among the
# starred), and the other one.
FIRST=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" |
  sed -n 's/^Part picker group Favourites: 2 parts, first test-profiles\/\(p30[a-z]*-test\)$/\1/p' | tail -1)
ui_mark
ui_step "All parts (the group row)"        ui_click_logged "Part picker group row All parts"
ui_expect_new "Part picker group All parts: 3 parts, first test-profiles/$FIRST" "the favourites first"
case $FIRST in
  p30-test) SECOND=p30h-test ;;
  p30h-test) SECOND=p30-test ;;
  *) ui_fail "the picker's first tile is no 30 profile ('$FIRST')" ;;
esac
echo "ok   the first favourite is $FIRST, the second $SECOND"
ui_step "close the picker (Escape)"        ui_key Escape
ui_focus_main
ui_step "Close (Escape)"                   ui_key Escape
ui_mark
ui_step "Profile again"                    ui_command "Profile"
ui_expect_new "BUILD profile starts with test-profiles/$FIRST: the first favourite" \
  "the design has no profile yet: the first favourite of the system"
choose_in_picker "Profile" "test-profiles/$SECOND"
ui_mark
ui_step "Place without a pick (Enter)"     ui_key Return
ui_expect_new "Placed $(part_name "$SECOND") as " "the second favourite placed" 30
ui_step "Close (Escape)"                   ui_key Escape
ui_mark
ui_step "Profile in the same design"       ui_command "Profile"
ui_expect_new "BUILD profile starts with test-profiles/$SECOND: " "the part placed last in this design"
ui_expect_new "Panel Profile tile part: $(part_name "$SECOND")" "its tile"
ui_step "Close (Escape)"                   ui_key Escape
ui_mark
ui_step "new design (Ctrl+N)"              ui_key ctrl+n
ui_focus_dialog '^MitCad$'
ui_step "don't save (D)"                   ui_key d
ui_focus_main
ui_expect_new "New document" "a new design"
ui_mark
ui_step "Profile in the new design"        ui_command "Profile"
ui_expect_new "BUILD profile starts with test-profiles/$FIRST: the first favourite" \
  "the first favourite again, not the part of the earlier design"
ui_expect_new "Panel Profile tile part: $(part_name "$FIRST")" "its tile"
ui_expect_new "View hint Profile: Click a plane or a face, or press Enter; Shift+click places at once" \
  "the view's hint names Shift+click (mitcad#451)"
ui_mark
ui_step "the part picker (the Part tile)"  ui_click_logged "Panel Profile input part"
ui_expect_new "Part picker shows the part chosen: test-profiles/$FIRST" "the picker shows and highlights it"
ui_focus_dialog '^Part Picker - Profile$'
ui_step "close the picker (Escape)"        ui_key Escape
ui_focus_main

echo "--- 10. Shift + click places at once (mitcad#451)"
ui_mark
ui_step "Shift+click on the XY plane"      shift_click $(ui_logged_at "Datum xy")
ui_expect_new "Profile Place On: Shift+click places at once" "a Shift+click on the plane"
ui_expect_new "Placed $(part_name "$FIRST") as " "the first profile placed without Place" 30
ui_expect_new "Applied Profile (1)" "the panel stays for the next one"
B10=$(placed_body)
[ -n "$B10" ] || ui_fail "no body of the profile placed by Shift+click"
refresh_picks "Profile"
SIDE10=$(faces_of "$B10" "$SIDE" | head -1)
[ -n "$SIDE10" ] || ui_fail "no side of the profile ($B10) to pick"
ui_mark
ui_step "a plain click on its side"        pick_face "$B10" "$SIDE10"
ui_expect_new "Inspect Profile: place side" "a plain click previews on the side"
no_placement "a plain click only previews"
ui_mark
ui_step "Shift+click on its side"          shift_pick_face "$B10" "$SIDE10"
ui_expect_new "Profile Place On: Shift+click places at once" "a second Shift+click"
ui_expect_new "Placed $(part_name "$FIRST") as " "the next profile placed on the side without Place" 30
ui_expect_new "Applied Profile (2)" "the panel stays for the next one"
# A placement the model refuses: a length over the part's 3000 mm.
ui_step "a typed length of 5000"           ui_type_in "Panel Profile input length" "5000"
refresh_picks "Profile"
SIDE10=$(faces_of "$B10" "$SIDE" | head -1)
[ -n "$SIDE10" ] || ui_fail "no side of the profile ($B10) to pick again"
ui_mark
ui_step "Shift+click on its side again"    shift_pick_face "$B10" "$SIDE10"
ui_expect_new "Profile Place On: Shift+click places at once" "a Shift+click that cannot place"
ui_expect_new "Profile: OK refused" "the placement refused"
ui_expect_new "Inspect Profile: failed: " "the panel shows why"
ui_expect_new "longer than the part's longest, 3000 mm" "the reason: the length is over the part's longest"
no_placement "a failed Shift+click makes nothing"
ui_mark
ui_step "Close (Escape)"                   ui_key Escape
ui_expect_new "Closed Profile after 2" "Close keeps the two"

ui_stop_app
ui_finish "the BUILD tab"
