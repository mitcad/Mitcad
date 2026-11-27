#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# The annotations' look against the reference drawing (mitcad#902): a
# one-off visual check, not a recurring test (its name is not ui-*-test.sh,
# so no check picks it up). It models a synthetic part like the reference
# (a 120 x 70 x 10 plate with four counterbored through holes, a 30 x 30
# boss 35 high with a tapped hole M8x1.25 - 6H in its top, a Ø36 disc 22
# high), annotates it as the reference does — two lengths (the plate's
# length and width), the disc's diameter, two hole callouts (the four
# counterbored holes, the tapped hole) with a position frame under the
# first, a perpendicularity frame under the second, two datums (A on the
# plate's front face, B on the disc) and a note on the boss's side face
# (its leader ending in a dot) — all in Auto, added as seen from the
# default isometric view, and saves pictures of the screen:
#   <dir>/light.png           white background, nothing selected
#   <dir>/light-selected.png  white background, Offset1 selected
#   <dir>/dark.png            dark background, nothing selected
#   <dir>/dark-selected.png   dark background, Offset1 selected
# each with <dir>/<name>-view.png, the 3D view cut out of that same
# picture, and the design itself as <dir>/look.mitcad, the app's log as
# <dir>/look.log.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-annotations-look.sh <dir>

source "$(dirname "$0")/ui-test-lib.sh"

DIR=${1:?usage: tools/ui-annotations-look.sh <dir>}
mkdir -p "$DIR"
DIR=$(cd "$DIR" && pwd)
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
trap 'cp -f "$UI_LOG" "$DIR/look.log" 2> /dev/null; ui_cleanup' EXIT
R='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'
VIEW='{"direction": [-1, 1, -1], "up": [0, 0, 1]}'

# F1/F2: the plate (F2.b0); F3: four counterbored through holes in it;
# F4/F5: the boss, joined; F6/F7: the disc, joined; F8: the tapped hole in
# the boss's top.
cat > "$DIR/look.json" << EOF
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 120, "height": 70},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$R"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "hole",
    "placement": {"type": "face", "body": "F2.b0", "face": "F2:end($R)",
                  "points": [[10, 10, 10], [110, 10, 10], [10, 60, 10], [110, 60, 10]]},
    "diameter": 6.6, "extent": {"type": "through_all"},
    "kind": {"type": "counterbore", "diameter": 11, "depth": 6.8}}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F4", "corner": [70, 10], "width": 30, "height": 30},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F4", "region": "$R"}],
    "extent": {"type": "distance", "distance": 35}, "operation": "join"}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_circle", "sketch": "F6", "center": [35, 35], "diameter": 36},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F6", "region": "r{c1}"}],
    "extent": {"type": "distance", "distance": 22}, "operation": "join"}},
  {"cmd": "add_feature", "def": {"type": "hole",
    "placement": {"type": "face", "body": "F2.b0", "face": "F5:end($R)", "points": [[85, 25, 35]]},
    "diameter": 6.8, "extent": {"type": "distance", "depth": 15},
    "thread": {"designation": "M8x1.25", "class": "6H"}}},
  {"cmd": "add_annotation", "name": "Offset1", "kind": "offset", "view": $VIEW,
    "a": {"body": "F2.b0", "face": "F2:side(c4[c3,c1])"}, "b": {"body": "F2.b0", "face": "F2:side(c2[c1,c3])"}},
  {"cmd": "add_annotation", "name": "Offset2", "kind": "offset", "view": $VIEW,
    "a": {"body": "F2.b0", "face": "F2:side(c1[c4,c2])"}, "b": {"body": "F2.b0", "face": "F2:side(c3[c2,c4])"}},
  {"cmd": "add_annotation", "name": "Diameter1", "kind": "diameter", "view": $VIEW,
    "a": {"body": "F2.b0", "face": "F7:side(c1)"}},
  {"cmd": "add_annotation", "name": "HoleCallout1", "kind": "hole_callout", "view": $VIEW,
    "a": {"body": "F2.b0", "face": "F3:hole0.cbore_wall"}, "at": [4.7, 8.6, 10]},
  {"cmd": "add_annotation", "name": "HoleCallout2", "kind": "hole_callout", "view": $VIEW,
    "a": {"body": "F2.b0", "face": "F8:hole0.wall"}, "at": [88.4, 25, 35]},
  {"cmd": "add_annotation", "name": "DatumA", "letter": "A", "view": $VIEW,
    "a": {"body": "F2.b0", "face": "F2:side(c1[c4,c2])"}, "at": [40, 0, 5]},
  {"cmd": "add_annotation", "name": "DatumB", "letter": "B", "view": $VIEW,
    "a": {"body": "F2.b0", "face": "F7:side(c1)"}, "at": [22.3, 22.3, 16]},
  {"cmd": "add_annotation", "name": "Note1", "kind": "note", "text": "LAPPED", "view": $VIEW,
    "a": {"body": "F2.b0", "face": "F5:side(c2[c1,c3])"}, "at": [100, 25, 24]},
  {"cmd": "add_annotation", "name": "Position", "a": {"body": "F2.b0", "face": "F3:hole0.cbore_wall"},
    "on": "HoleCallout1", "frame": {"characteristic": "position", "value": 0.2, "diameter": true,
    "datums": ["A", "B"]}},
  {"cmd": "add_annotation", "name": "Square", "a": {"body": "F2.b0", "face": "F8:hole0.wall"},
    "on": "HoleCallout2", "frame": {"characteristic": "perpendicularity", "value": 0.05,
    "diameter": true, "datums": ["A"]}}
]
EOF
"$CLI" run "$DIR/look.json" --save "$DIR/look.mitcad" > "$DIR/cli.log" 2>&1 ||
  { cat "$DIR/cli.log"; ui_fail "mitcad-cli (the part)"; }

# away: the mouse off the window (no hover, no tooltip).
away() { xdotool mousemove 5000 5000; }

# shot name: the whole screen once the view has settled, the mouse away and
# a tooltip gone, and the 3D view cut out of that same picture.
shot() {
  local dump from to
  away
  ui_wait_idle
  ui_sync
  sleep 2
  dump="$DIR/$1.xwd"
  xwd -root -silent > "$dump"
  python3 "$(dirname "$0")/xwd2png.py" "$dump" "$DIR/$1.png"
  read -r -a from <<< "$(ui_view_at 0 0)"
  read -r -a to <<< "$(ui_view_at 100 100)"
  python3 "$(dirname "$0")/xwd2png.py" "$dump" "$DIR/$1-view.png" \
    "${from[0]}" "${from[1]}" $((to[0] - from[0])) $((to[1] - from[1]))
  rm -f "$dump"
  echo "ok   $DIR/$1.png $DIR/$1-view.png"
}

# unselect: nothing highlighted (the Annotations window opened and closed
# takes a highlight away).
unselect() {
  if grep -F "Annotations highlighted: " "$UI_LOG" | tail -1 | grep -qvF "Annotations highlighted: none"; then
    ui_mark
    ui_step "Annotations (search)"         ui_command "Annotations"
    ui_expect_new "Annotations window opened" "the window opened"
    ui_focus_dialog '^Annotations$'
    ui_step "close the window (Escape)"    ui_key Escape
    ui_focus_main
    ui_expect_new "Annotations highlighted: none" "nothing highlighted"
  fi
  away
  ui_sync
}

# select_offset: Offset1 selected by a click on its text in the view.
select_offset() {
  ui_mark
  ui_step "select Offset1 (a click on its text)" ui_click_logged "Annotation label Offset1"
  ui_expect_new "Annotations highlighted: Offset1" "Offset1 selected"
  away
  ui_sync
}

ui_start_display
ui_start_app --open "$DIR/look.mitcad"
ui_expect_log "Annotations drawn: Offset1 = " "the annotations drawn"
grep -F "Annotations: " "$UI_LOG" | tail -1 | grep -q "broken" &&
  ui_fail "an annotation is broken: $(grep -F "Annotations: " "$UI_LOG" | tail -1)"
ui_step "fit (F6)"                         ui_key F6
# A step out, so that the annotations around the part are in the view too.
ui_step "zoom out"                         xdotool mousemove $(ui_view_at 50 50) click 5 click 5 click 5
# Off the part, so that nothing is hovered.
ui_step "the mouse off the part"           xdotool mousemove $(ui_view_at 3 3)

echo "--- The white background"
ui_mark
ui_step "white background (search)"        ui_command "White Background"
ui_expect_new "Background " "the background white"
unselect
shot light
select_offset
shot light-selected
unselect

echo "--- The dark background"
ui_mark
ui_step "dark background (search)"         ui_command "Dark Background"
ui_expect_new "Background " "the background dark"
unselect
shot dark
select_offset
shot dark-selected
unselect

grep -F "Annotations drawn: " "$UI_LOG" | tail -1
grep -F "Annotation lettering: " "$UI_LOG" | head -1
ui_finish "Annotations look pictures in $DIR"
