#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# A Fillet's and a Chamfer's picked edges drawn where they were
# (mitcad#957): a one-off visual check, not a recurring test (its name is
# not ui-*-test.sh, so no check picks it up). It models two blocks side by
# side, 30 x 20 x 15: the left one with its four vertical edges rounded
# (Fillet1, r 4), the right one with its four top edges bevelled
# (Chamfer1, 3 mm), selects each feature on the timeline and saves
# pictures of the screen, from the front (the isometric view) and from the
# back (the back view turned up and to the side), on the light and the
# dark background:
#   <dir>/fillet-front-light.png   <dir>/chamfer-front-light.png
#   <dir>/fillet-back-light.png    <dir>/chamfer-back-light.png
#   <dir>/fillet-front-dark.png    <dir>/chamfer-front-dark.png
#   <dir>/fillet-back-dark.png     <dir>/chamfer-back-dark.png
# each with <dir>/<name>-view.png, the 3D view cut out of that same
# picture; the design as <dir>/look.mitcad, the app's log as
# <dir>/look.log (the last screen dump stays as <dir>/screen.xwd).
#
# What to look for: the picked edges as lines in the selection colour
# where the edges were before the feature (the fillet's at the block's
# rounded corners, the chamfer's along its bevelled top), the parts the
# solid hides dashed through it, the feature's own faces in amber apart
# from them; the lines clearly visible on both backgrounds.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-feature-inputs-look.sh <dir>

source "$(dirname "$0")/ui-test-lib.sh"

DIR=${1:?usage: tools/ui-feature-inputs-look.sh <dir>}
mkdir -p "$DIR"
DIR=$(cd "$DIR" && pwd)
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
trap 'cp -f "$UI_LOG" "$DIR/look.log" 2> /dev/null; ui_cleanup' EXIT
R='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'

# F1/F2: the left block (F2.b0); F3: Fillet1 on its four vertical edges.
# F4/F5: the right block (F5.b0); F6: Chamfer1 on its four top edges.
cat > "$DIR/look.json" << EOF
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 30, "height": 20},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$R"}],
    "extent": {"type": "distance", "distance": 15}, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "fillet", "body": "F2.b0",
    "edges": ["E{F2:side(c1[c4,c2])|F2:side(c2[c1,c3])}", "E{F2:side(c2[c1,c3])|F2:side(c3[c2,c4])}",
              "E{F2:side(c3[c2,c4])|F2:side(c4[c3,c1])}", "E{F2:side(c4[c3,c1])|F2:side(c1[c4,c2])}"],
    "radius": 4}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F4", "corner": [50, 0], "width": 30, "height": 20},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F4", "region": "$R"}],
    "extent": {"type": "distance", "distance": 15}, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "chamfer", "body": "F5.b0",
    "edges": ["E{F5:end($R)|F5:side(c1[c4,c2])}", "E{F5:end($R)|F5:side(c2[c1,c3])}",
              "E{F5:end($R)|F5:side(c3[c2,c4])}", "E{F5:end($R)|F5:side(c4[c3,c1])}"],
    "size": {"type": "equal_distance", "distance": 3}}}
]
EOF
"$CLI" run "$DIR/look.json" --save "$DIR/look.mitcad" > "$DIR/cli.log" 2>&1 ||
  { cat "$DIR/cli.log"; ui_fail "mitcad-cli (the blocks)"; }

# away: the mouse off the window (no hover, no tooltip).
away() { xdotool mousemove 5000 5000; }

# shot name: the whole screen once the view has settled, the mouse away and
# a tooltip gone, and the 3D view cut out of that same picture.
shot() {
  local dump="$DIR/screen.xwd" from to
  away
  ui_wait_idle
  ui_sync
  sleep 2
  xwd -root -silent > "$dump"
  python3 "$(dirname "$0")/xwd2png.py" "$dump" "$DIR/$1.png"
  read -r -a from <<< "$(ui_view_at 0 0)"
  read -r -a to <<< "$(ui_view_at 100 100)"
  python3 "$(dirname "$0")/xwd2png.py" "$dump" "$DIR/$1-view.png" \
    "${from[0]}" "${from[1]}" $((to[0] - from[0])) $((to[1] - from[1]))
  echo "ok   $DIR/$1.png $DIR/$1-view.png"
}

# select Name edges: a click on the feature's timeline cell, its picked
# edges logged as drawn.
select_feature() {
  ui_mark
  ui_step "click $1 on the timeline"         ui_click_logged "Timeline $1"
  ui_expect_new "Feature inputs highlighted: $1 edges $2" "$1's $2 edges drawn where they were"
}

unselect() {
  ui_mark
  ui_step "Esc"                              ui_key Escape
  ui_expect_new "Feature inputs highlighted: none" "nothing drawn"
}

# front: the isometric view, fitted.
front() {
  # (The design opens in it, so the camera need not change: nothing to
  # wait for in the log.)
  ui_step "isometric view (search)"          ui_command "Isometric View"
  ui_sync
  ui_step "fit (F6)"                         ui_key F6
}

# back: the back view, turned up and to the side (Shift + middle drag
# orbits), fitted.
back() {
  ui_mark
  ui_step "back view (search)"               ui_command "Back View"
  ui_expect_new "Camera direction" "the back view" 8
  ui_mark
  xdotool keydown shift
  ui_step "orbit (Shift + middle drag)"      ui_view_drag 2 50 50 40 60
  xdotool keyup shift
  ui_expect_new "Camera direction" "turned up and to the side" 8
  ui_step "fit (F6)"                         ui_key F6
}

# pictures background: both features from the front and from the back.
pictures() {
  local view
  for view in front back; do
    "$view"
    select_feature Fillet1 4
    shot "fillet-$view-$1"
    select_feature Chamfer1 4
    shot "chamfer-$view-$1"
    unselect
  done
}

ui_start_display
ui_start_app --open "$DIR/look.mitcad"
ui_expect_log "Timeline Chamfer1 at" "the timeline is logged"
grep -q "Recompute failed\|Failed features: [^n]" "$UI_LOG" && ui_fail "a feature failed"

echo "--- The light background"
ui_mark
ui_step "light background (search)"        ui_command "Light Background"
ui_expect_new "Background " "the background light"
pictures light

echo "--- The dark background"
ui_mark
ui_step "dark background (search)"         ui_command "Dark Background"
ui_expect_new "Background " "the background dark"
pictures dark

ui_finish "Feature inputs look pictures in $DIR"
