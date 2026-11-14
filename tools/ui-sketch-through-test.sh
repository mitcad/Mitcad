#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch/SketchController.cpp app/sketch/SketchOverlay.cpp app/sketch/SketchOverlay.hpp
# check-all sources: app/OcctViewer.cpp app/OcctViewer.hpp tools/cli
# The edited sketch shows through the bodies in front of it (mitcad#1762,
# GitHub #320), through the real UI:
#   - A 60 x 40 x 20 block; a sketch on the construction plane 10 above XY,
#     through the middle of the block. Seen along the sketch's normal, the
#     block's upper half lies in front of the plane.
#   - A circle drawn behind the block's top: the view draws a faint copy of
#     the edited sketch's curves and points through the bodies (the log
#     counts them; other sketches get none), and a screen dump shows its
#     rim tinted on the block's top face, fainter than the circle in sight.
#   - A diameter dimension placed behind the top: the overlay draws it
#     dimmed; a click on the hidden rim still selects the circle.
#   - Hide Above Sketch keeps its side: with it on, nothing is in front
#     from above (the dimension at full strength, the rim drawn in sight);
#     from below, the block's lower half hides the sketch and it shows
#     through dimmed again.
#   - Finishing the sketch takes the copies away.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sketch-through-test.sh [pictures-folder]
#   With a folder, the views from above and from below (and with Hide Above
#   Sketch on) are saved there as PNG files.

source "$(dirname "$0")/ui-test-lib.sh"

PICS=${1:-}
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-sketch-through.XXXXXX)
FILE=$WORK/block.mitcad
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT
[ -z "$PICS" ] || mkdir -p "$PICS"

# Sketch1 (F1) 60 x 40 at the origin, Extrude1 (F2) 20 up: Body1 (F2.b0).
# Plane1 (F3) 10 above XY, through the middle of the block.
cat > "$WORK/model.json" << 'EOF'
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 60, "height": 40},
  {"cmd": "add_feature", "def": {"type": "extrude",
    "profiles": [{"sketch": "F1", "region": "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}],
    "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "construction_plane",
    "definition": {"type": "offset", "plane": "xy", "distance": 10}}}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# since_mark "text": whether a line logged after the mark has the text.
since_mark() { tail -n +$((${UI_MARK:-0} + 1)) "$UI_LOG" | grep -qF -- "$1"; }

# last_line "prefix": the last line logged that starts with the prefix.
last_line() { grep -F -- "$1" "$UI_LOG" | tail -1; }

# expect_last "prefix" "line" description: the last line with the prefix is
# the line (waits a little for it).
expect_last() {
  for _ in $(seq 1 40); do
    [ "$(last_line "$1")" = "$2" ] && { echo "ok   $3"; return; }
    sleep 0.2
  done
  ui_fail "$3: the last line is '$(last_line "$1")', not '$2'"
}

# rgb_at name x y: the colour (r g b, 0-255) at a sketch point in the dump.
rgb_at() {
  local px py
  read -r px py <<< "$(ui_sketch_at "$2" "$3")"
  convert "$WORK/$1.xwd" -crop "1x1+$px+$py" +repage -depth 8 txt:- |
    sed -n 's/^[0-9]*,[0-9]*: *(\([0-9.]*\),\([0-9.]*\),\([0-9.]*\).*/\1 \2 \3/p' | head -1
}

# rim_tint name: how far the circle's rim (centre (30, 20), radius 10) is
# from the face under it in the dump: "<samples tinted> <largest change>",
# over ten places on the rim (clear of the diameter's line at 90 and 270
# degrees), each a 7 x 7 pixel square; the face's colour is read inside the
# circle, clear of everything drawn on the sketch.
rim_tint() {
  local face fr fg fb tinted=0 most=0 angle x y px py change
  face=$(rgb_at "$1" 24 16)
  read -r fr fg fb <<< "$face"
  for angle in 0 30 60 120 150 180 210 240 300 330; do
    read -r x y <<< "$(awk -v a="$angle" 'BEGIN { r = a * 3.14159265 / 180; printf "%.3f %.3f\n", 30 + 10 * cos(r), 20 + 10 * sin(r) }')"
    read -r px py <<< "$(ui_sketch_at "$x" "$y")"
    change=$(convert "$WORK/$1.xwd" -crop "7x7+$((px - 3))+$((py - 3))" +repage -depth 8 txt:- |
      sed -n 's/^[0-9]*,[0-9]*: *(\([0-9.]*\),\([0-9.]*\),\([0-9.]*\).*/\1 \2 \3/p' |
      awk -v r="$fr" -v g="$fg" -v b="$fb" '{
        d = ($1 > r ? $1 - r : r - $1) + ($2 > g ? $2 - g : g - $2) + ($3 > b ? $3 - b : b - $3)
        if (d > m) m = d } END { print m + 0 }')
    [ "$change" -gt 30 ] && tinted=$((tinted + 1))
    [ "$change" -gt "$most" ] && most=$change
  done
  echo "$tinted $most"
}

# dump name: with the pointer off the sketch (its hover would draw the
# circle over the view), a screen dump once the view rests; saved as a
# picture too when a folder was given.
dump() {
  xdotool mousemove $(ui_view_at 3 97)
  ui_wait_idle
  ui_sync
  sleep 1
  xwd -root -silent > "$WORK/$1.xwd" || ui_fail "$1: no screenshot"
  [ -z "$PICS" ] || convert "$WORK/$1.xwd" "$PICS/sketch-through-$1.png"
}

ui_start_display
ui_start_app --open "$FILE"
ui_step "fit (F6)"                         ui_key F6
ui_expect_log "Bodies shown: F2.b0" "the block shown"

echo "--- A sketch on Plane1, through the block"
ui_create_sketch F3
ui_expect_log "Sketch through the bodies: F4 0 curve(s), 0 point(s), dimmed, curves dashed" \
  "the edited sketch is drawn through the bodies"
ui_step "circle tool (C)"                  ui_key c
ui_step "circle: centre"                   ui_sketch_click 30 20
ui_step "circle: rim"                      ui_sketch_click 40 20
ui_expect_log "Added circle, diameter 20 mm at (30, 20)" "a circle under the block's top"
ui_step "end the tool (Esc)"               ui_key Escape
expect_last "Sketch through the bodies: " \
  "Sketch through the bodies: F4 1 curve(s), 1 point(s), dimmed, curves dashed" \
  "the circle and its centre get their faint copies, nothing of another sketch"

echo "--- A diameter dimension behind the block's top"
ui_mark
ui_step "sketch dimension (D)"             ui_key d
ui_step "pick the circle"                  ui_sketch_click 40 20
ui_step "place the value"                  ui_sketch_click 30 33
ui_expect_new "Sketch value editor at" "the value editor opened"
ui_step "type 20"                          xdotool type --delay 40 "20"
ui_step "Enter"                            ui_key Return
ui_step "end the tool (Esc)"               ui_key Escape
xdotool mousemove $(ui_view_at 3 97)
ui_wait_idle
expect_last "Sketch overlay through the bodies: " \
  "Sketch overlay through the bodies: 1 dimension(s), 0 constraint symbol(s) dimmed" \
  "the dimension behind the top is drawn dimmed"

echo "--- The circle through the block's top, from above"
dump above
read -r tinted through <<< "$(rim_tint above)"
[ "$tinted" -ge 5 ] || ui_fail "the hidden circle is not drawn through the top ($tinted of 10 places tinted)"
echo "ok   the hidden circle shows through the top ($tinted of 10 places tinted, change up to $through)"

echo "--- A click on the hidden rim selects the circle"
ui_mark
ui_step "click the rim"                    ui_sketch_click 20 20
ui_expect_new "Selected: 1 sketch curve [sketch curve " "the hidden circle is picked"
ui_step "clear the selection (Esc)"        ui_key Escape
ui_sync

echo "--- Hide Above Sketch: nothing in front from above"
ui_mark
ui_step "check Hide Above Sketch"          ui_click_logged "Palette paletteHideAbove"
ui_expect_new "Hide Above Sketch on" "on"
ui_expect_new "In front of the sketch plane: F2.b0 0.0" "the block cut at the plane"
xdotool mousemove $(ui_view_at 3 97)
ui_wait_idle
expect_last "Sketch overlay through the bodies: " \
  "Sketch overlay through the bodies: 0 dimension(s), 0 constraint symbol(s) dimmed" \
  "nothing in front: the dimension at full strength"
dump hidden-above
read -r tinted insight <<< "$(rim_tint hidden-above)"
[ "$tinted" -ge 5 ] || ui_fail "the circle in sight is not drawn ($tinted of 10 places)"
[ $((through * 10)) -lt $((insight * 7)) ] ||
  ui_fail "the circle through the top ($through) is not fainter than in sight ($insight)"
echo "ok   in sight the circle is drawn at full strength ($insight), through the top dimmed ($through)"

echo "--- From below, the lower half hides the sketch"
ui_mark
ui_step "bottom view (search)"             ui_command "Bottom View"
ui_expect_new "Camera direction 0 0 1 " "seen from below"
ui_wait_idle
expect_last "Sketch overlay through the bodies: " \
  "Sketch overlay through the bodies: 1 dimension(s), 0 constraint symbol(s) dimmed" \
  "Hide Above Sketch keeps its side: from below the dimension is dimmed"
dump below-hidden-above
read -r tinted below <<< "$(rim_tint below-hidden-above)"
[ "$tinted" -ge 5 ] || ui_fail "from below the circle does not show through ($tinted of 10 places tinted)"
echo "ok   from below the circle shows through the lower half ($tinted of 10 places tinted, change up to $below)"

ui_mark
ui_step "uncheck Hide Above Sketch"        ui_click_logged "Palette paletteHideAbove"
ui_expect_new "Hide Above Sketch off" "off"
ui_expect_new "In front of the sketch plane: F2.b0 10.0" "the block whole again"
dump below

echo "--- Finishing the sketch"
ui_mark
ui_step "top view (search)"                ui_command "Top View"
ui_expect_new "Camera direction 0 0 -1 " "the top view again"
ui_step "finish the sketch (Ctrl+Enter)"   ui_key ctrl+Return
ui_expect_new "Sketch finished" "sketch left"
expect_last "Sketch through the bodies: " "Sketch through the bodies: none" "no copies after the sketch"

ui_finish "UI sketch through test"
