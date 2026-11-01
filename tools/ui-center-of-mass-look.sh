#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# The Center of Mass marker (mitcad#1094): a one-off visual check, not a
# recurring test (its name is not ui-*-test.sh, so no check picks it up).
# On the design of tools/ui-center-of-mass-test.sh (an aluminum block in
# the root, a steel plate below it in Component1:1, moved 20 mm along X so
# that the center lies off both bodies' middles), Center of Mass adds the
# marker, and pictures of the screen are saved on the light and the dark
# background, from the isometric view fitted and zoomed in (the marker
# keeps its size on screen), and with the row's light bulb off:
#   <dir>/light.png  <dir>/light-close.png  <dir>/light-hidden.png
#   <dir>/dark.png   <dir>/dark-close.png
# each with <dir>/<name>-view.png, the 3D view cut out of that same
# picture; the design as <dir>/look.mitcad and the app's log as
# <dir>/look.log (the last screen dump stays as <dir>/screen.xwd).
#
# What to look for: a crossed circle (two opposite quadrants filled in
# teal, the others and a thin ring in the background's tone) between the
# two bodies' middles, drawn over the bodies although the center lies
# inside them, equally large in the fitted and the zoomed-in pictures, and
# readable on both backgrounds; the browser's Center of Mass row right
# under Origin with its light bulb; no marker in light-hidden.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-center-of-mass-look.sh <dir>

source "$(dirname "$0")/ui-test-lib.sh"

DIR=${1:?usage: tools/ui-center-of-mass-look.sh <dir>}
mkdir -p "$DIR"
DIR=$(cd "$DIR" && pwd)
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
trap 'cp -f "$UI_LOG" "$DIR/look.log" 2> /dev/null; ui_cleanup' EXIT
R='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'

cat > "$DIR/look.json" << EOF
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 30, "height": 20},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$R"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$R"}],
    "extent": {"type": "distance", "distance": -4}, "operation": "new_component"}},
  {"cmd": "set_occurrence_transform", "occurrence": "Component1:1", "transform": {"translation": [20, 0, 0]}},
  {"cmd": "set_body_material", "uid": "F2.b0", "material": "aluminum"}
]
EOF
"$CLI" run "$DIR/look.json" --save "$DIR/look.mitcad" > "$DIR/cli.log" 2>&1 ||
  { cat "$DIR/cli.log"; ui_fail "mitcad-cli (the block and the plate)"; }

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

# pictures tone: fitted, then zoomed in at the view's middle, then fitted
# again.
pictures() {
  ui_step "isometric view (search)"        ui_command "Isometric View"
  ui_step "fit (F6)"                       ui_key F6
  shot "$1"
  xdotool mousemove $(ui_view_at 50 50)
  ui_step "zoom in (wheel)"                xdotool click --repeat 5 --delay 150 4
  shot "$1-close"
  ui_step "fit (F6)"                       ui_key F6
}

ui_start_display
ui_start_app --open "$DIR/look.mitcad"
ui_expect_log "Bodies shown: F2.b0, F3.b0 in O1" "the block and the plate shown"

ui_mark
ui_step "Center of Mass (search)"          ui_command "Center of Mass"
ui_expect_new "Center of Mass drawn: " "the marker drawn"

echo "--- The light background"
ui_mark
ui_step "light background (search)"        ui_command "Light Background"
ui_expect_new "Background " "the background light"
pictures light
ui_mark
ui_step "hide the marker (light bulb)"     ui_click_logged "Browser eye Root/Center of Mass"
ui_expect_new "Center of Mass drawn: none" "hidden"
shot light-hidden
ui_mark
ui_step "show it (light bulb)"             ui_click_logged "Browser eye Root/Center of Mass"
ui_expect_new "Center of Mass drawn: " "shown again"

echo "--- The dark background"
ui_mark
ui_step "dark background (search)"         ui_command "Dark Background"
ui_expect_new "Background " "the background dark"
ui_expect_log "on a dark background" "the marker drawn in the dark background's tone"
pictures dark

ui_finish "Center of Mass look pictures in $DIR"
