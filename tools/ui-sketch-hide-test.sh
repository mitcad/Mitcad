#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch
# Hide Above Sketch, the sketch palette's option, through the real UI:
#   - Body1 from z = 0 to 10 and Body2 from z = 5 to 15; a sketch on the
#     construction plane at z = 5 has 5 mm of Body1 and all of Body2 in
#     front of it.
#   - Hide Above Sketch cuts the bodies at the sketch plane: nothing is in
#     front of it, Body2 is gone from the view; off again, both are whole.
#   - The cut face of Body1 is shown as a section cap (hatched); Body3,
#     from z = 0 to 5, only touches the plane: its top face is its own, no
#     cap.
#   - The circle drawn on the plane, over Body1's cap, is drawn over the
#     section (pixels of a screen dump in its line's or halo's colours, not
#     the cap's; mitcad#784): in the view the sketch opened in, zoomed in
#     and zoomed out, from the isometric view and in the perspective view
#     (fitted); from below, Body1's lower half is in front of it: the circle
#     is not drawn at full strength there (it shows through dimmed,
#     mitcad#1762).
#   - Offset (mitcad#223) on a circle under Body1's top: the bodies stay
#     cut while its panel is open, while it previews and after Cancel and
#     OK; with the option off, Offset does not turn it on.
#   - The option stays for the next sketch, and the bodies are cut only
#     while sketching.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sketch-hide-test.sh [screenshot.png]
# check-all sources: tools/cli
# check-all sources: app/OcctViewer.cpp app/OcctViewer.hpp app/view
# check-all sources: app/MainWindow.cpp app/MainWindow.hpp app/framework/CommandSession.cpp

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-sketch-hide.XXXXXX)
FILE=$WORK/blocks.mitcad
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT

# Body1 (F2.b0) 30 x 20 x 10 on XY; Plane1 (F3) 5 above XY; Body2 (F5.b0)
# 10 x 10 x 10 on Plane1, beside Body1; Body3 (F7.b0) 10 x 10 x 5 on XY,
# behind Body1.
cat > "$WORK/model.json" << 'EOF'
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 30, "height": 20},
  {"cmd": "add_feature", "def": {"type": "extrude",
    "profiles": [{"sketch": "F1", "region": "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "construction_plane",
    "definition": {"type": "offset", "plane": "xy", "distance": 5}}},
  {"cmd": "sketch.create", "plane": "F3"},
  {"cmd": "sketch.add_rectangle", "sketch": "F4", "corner": [40, 0], "width": 10, "height": 10},
  {"cmd": "add_feature", "def": {"type": "extrude",
    "profiles": [{"sketch": "F4", "region": "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F6", "corner": [0, 30], "width": 10, "height": 10},
  {"cmd": "add_feature", "def": {"type": "extrude",
    "profiles": [{"sketch": "F6", "region": "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}],
    "extent": {"type": "distance", "distance": 5}, "operation": "new_body"}}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# since_mark "text": whether a line logged after the mark has the text.
since_mark() { tail -n +$((${UI_MARK:-0} + 1)) "$UI_LOG" | grep -qF -- "$1"; }

# still_hidden "when": Hide Above Sketch still cuts the bodies at the
# sketch plane: since the mark it was not turned off, the caps did not go
# and no body came back in front of the plane, and the last reach logged
# is Body1 cut at it (mitcad#223).
still_hidden() {
  local last
  ui_sync
  since_mark "Hide Above Sketch off" && ui_fail "$1: Hide Above Sketch turned off"
  since_mark "Section caps: none" && ui_fail "$1: the bodies shown whole (no section caps)"
  since_mark "In front of the sketch plane: F2.b0 5.0" && ui_fail "$1: Body1 in front of the sketch plane again"
  last=$(grep "In front of the sketch plane: " "$UI_LOG" | tail -1)
  case "$last" in
  "In front of the sketch plane: F2.b0 0.0"*) ;;
  *) ui_fail "$1: the last reach is '$last'" ;;
  esac
  echo "ok   $1: the bodies stay cut at the sketch plane"
}

# sketch_pixels dump x y: how many pixels of the screen dump around the
# sketch point (x, y) mm are the sketch's curve or halo, in a square about
# 4 mm across (3 to 8 px each way from the point), which the cap fills 5 mm
# from its edge; "off" when the point is outside the 3D view. A sketch pixel
# is not orange (the cap, #f29e1a, and its hatch, #6d4302, also under the
# profile's tint, have far more red than blue) and is dark (a fully
# constrained curve), the unconstrained blue or the light halo; the bodies'
# grey faces and the cap's lit shades are none of these.
sketch_pixels() {
  local px py half x0 y0 x1 y1
  read -r px py <<< "$(ui_sketch_at "$2" "$3")"
  read -r x0 y0 <<< "$(ui_view_at 0 0)"
  read -r x1 y1 <<< "$(ui_view_at 100 100)"
  if [ "$px" -lt "$x0" ] || [ "$px" -ge "$x1" ] || [ "$py" -lt "$y0" ] || [ "$py" -ge "$y1" ]; then
    echo off
    return
  fi
  half=$(grep -o "Sketch view [-0-9.,]* [-0-9.,]* [-0-9.,]*" "$UI_LOG" | tail -1 | awk -v s="$UI_SCALE" '{
    split($3, o, ","); split($4, a, ",")
    h = int(2 * s * sqrt((a[1] - o[1]) ^ 2 + (a[2] - o[2]) ^ 2) / 100 + 0.5)
    print (h < 3 ? 3 : h > 8 ? 8 : h) }')
  convert "$WORK/$1.xwd" -crop "$((2 * half + 1))x$((2 * half + 1))+$((px - half))+$((py - half))" +repage \
    -depth 8 txt:- |
    sed -n 's/^[0-9]*,[0-9]*: *(\([0-9.]*\),\([0-9.]*\),\([0-9.]*\).*/\1 \2 \3/p' |
    awk '{ l = 0.299 * $1 + 0.587 * $2 + 0.114 * $3 }
      $1 - $3 < 40 && (l < 100 || l > 240 || $3 - $1 >= 80) { n++ } END { print n + 0 }'
}

# over_section "view" 1|0: with the pointer off the sketch (its hover would
# draw the circle over the view), a dump: at the circle's point (25, 10)
# over Body1's cap the circle's line or halo is drawn (1), or neither is
# (0: behind the body in front of the plane, at most dimmed: mitcad#1762
# draws the edited sketch through it faintly, which these colours do not
# count).
over_section() {
  local count
  xdotool mousemove $(ui_view_at 3 97)
  ui_wait_idle
  ui_sync
  sleep 1
  xwd -root -silent > "$WORK/section.xwd" || ui_fail "$1: no screenshot"
  [ -z "$SHOT" ] || convert "$WORK/section.xwd" "${SHOT%.png}-section-$(echo "$1" | tr -c 'a-z0-9\n' '-').png"
  count=$(sketch_pixels section 25 10)
  [ "$count" != off ] || ui_fail "$1: the circle's point (25, 10) is outside the view"
  if [ "$2" = 1 ]; then
    [ "$count" -ge 3 ] || ui_fail "$1: the circle over the section is hidden ($count pixels of its line or halo)"
    echo "ok   $1: the circle is drawn over the section ($count pixels of its line or halo)"
  else
    [ "$count" -eq 0 ] || ui_fail "$1: the circle is drawn at full strength through the body in front of it ($count pixels)"
    echo "ok   $1: behind the body in front of the plane the circle is not drawn at full strength"
  fi
}

# offset_circle: starts Offset with nothing selected and picks the circle
# (diameter 20 at (15, 10)) by its rim; it previews at 5 mm.
offset_circle() {
  ui_step "offset (O)"                     ui_key o
  ui_expect_new "Command Offset started" "Offset started"
  ui_step "pick the circle"                ui_sketch_click 25 10
  ui_expect_new "Offset Curves: 1 sketch curve" "the circle picked"
  ui_expect_new "Preview Offset: ok" "offset previewed"
}

ui_start_display
ui_start_app --open "$FILE"
ui_step "fit (F6)"                         ui_key F6
ui_expect_log "Bodies shown: F2.b0, F5.b0, F7.b0" "the bodies shown"

echo "--- A sketch on Plane1"
ui_create_sketch F3
ui_expect_log "Palette paletteHideAbove at" "the option is in the palette"
ui_expect_log "In front of the sketch plane: F2.b0 5.0, F5.b0 10.0" "5 mm of Body1 and Body2 in front"
ui_step "circle tool (C)"                  ui_key c
ui_step "circle: centre"                   ui_sketch_click 15 10
ui_step "circle: rim"                      ui_sketch_click 25 10
ui_expect_log "Added circle, diameter 20 mm at (15, 10)" "a circle under Body1's top"
ui_step "end the tool (Esc)"               ui_key Escape

echo "--- Hide Above Sketch"
ui_mark
ui_step "check Hide Above Sketch"          ui_click_logged "Palette paletteHideAbove"
ui_expect_new "Hide Above Sketch on" "on"
ui_expect_new "In front of the sketch plane: F2.b0 0.0" "Body1 cut at the plane, Body2 gone"
ui_expect_new "Section caps: 1 face(s)" "Body1's cut face as a cap, not Body3's top"
if [ -n "$SHOT" ]; then
  ui_sync
  ui_capture "$SHOT"
fi

echo "--- The sketch over the section (mitcad#784)"
# The circle's point (25, 10) lies on Body1's cap, 5 mm from its edge.
# There the circle's line or halo is drawn over the cap in the view the
# sketch opened in, zoomed in and out, from the isometric view (the plane at
# an angle) and in the perspective view (fitted: switched to in the
# sketch's view, the camera comes very close); from below, Body1's lower
# half hides it.
over_section "the sketch's view" 1
ui_mark
ui_step "zoom in at the circle"            xdotool mousemove $(ui_sketch_at 25 10) click --repeat 3 --delay 150 4
ui_expect_new "Sketch view " "the view zoomed"
over_section "zoomed in" 1
ui_mark
ui_step "zoom out at the circle"           xdotool mousemove $(ui_sketch_at 25 10) click --repeat 6 --delay 150 5
ui_expect_new "Sketch view " "the view zoomed"
over_section "zoomed out" 1
ui_mark
ui_step "isometric view (search)"          ui_command "Isometric View"
ui_expect_new "Camera direction -0.577 0.577 -0.577" "the plane seen at an angle, from above"
over_section "isometric" 1
ui_mark
ui_step "top view (search)"                ui_command "Top View"
ui_expect_new "Camera direction 0 0 -1 " "the top view"
ui_mark
ui_step "perspective (search)"             ui_command "Perspective"
ui_expect_new "(perspective)" "the perspective view"
ui_mark
ui_step "fit (F6)"                         ui_key F6
ui_expect_new "Sketch view " "the perspective view fitted"
ui_wait_idle
sleep 1.5
over_section "perspective" 1
ui_mark
ui_step "orthographic (search)"            ui_command "Orthographic"
ui_expect_new "(orthographic)" "orthographic again"
ui_mark
ui_step "bottom view (search)"             ui_command "Bottom View"
ui_expect_new "Camera direction 0 0 1 " "seen from below"
over_section "from below" 0
ui_mark
ui_step "top view (search)"                ui_command "Top View"
ui_expect_new "Camera direction 0 0 -1 " "the top view again"
ui_step "fit (F6)"                         ui_key F6
ui_wait_idle
ui_sync

echo "--- Offset keeps Hide Above Sketch (mitcad#223)"
ui_mark
offset_circle
still_hidden "Offset started and previewed"
ui_mark
ui_step "distance 2"                       ui_type_in "Panel Offset input distance" "2"
ui_expect_new "Preview Offset: ok" "2 mm previewed"
still_hidden "the preview refreshed"
ui_mark
ui_step "cancel (Esc)"                     ui_key Escape
ui_expect_new "Command Offset cancelled" "Offset cancelled"
still_hidden "after Cancel"
ui_mark
offset_circle
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Offset 1 curve(s) by 5 mm" "offset made"
still_hidden "after OK"

ui_mark
ui_step "uncheck Hide Above Sketch"        ui_click_logged "Palette paletteHideAbove"
ui_expect_new "Hide Above Sketch off" "off"
ui_expect_new "In front of the sketch plane: F2.b0 5.0, F5.b0 10.0" "the bodies whole again"
ui_expect_new "Section caps: none" "no caps"

echo "--- Offset with Hide Above Sketch off leaves it off"
ui_mark
offset_circle
ui_step "cancel (Esc)"                     ui_key Escape
ui_expect_new "Command Offset cancelled" "Offset cancelled"
ui_sync
since_mark "Hide Above Sketch on" && ui_fail "Offset turned Hide Above Sketch on"
since_mark "Section caps: 1 face(s)" && ui_fail "Offset cut the bodies"
grep "In front of the sketch plane: " "$UI_LOG" | tail -1 | grep -qF "F2.b0 5.0, F5.b0 10.0" ||
  ui_fail "the bodies are not whole after Offset"
echo "ok   Offset leaves the option off and the bodies whole"

echo "--- The next sketch"
ui_step "check Hide Above Sketch"          ui_click_logged "Palette paletteHideAbove"
ui_expect_log "Hide Above Sketch on" "on again"
ui_mark
ui_step "finish the sketch (Ctrl+Enter)"   ui_key ctrl+Return
ui_expect_new "Sketch finished" "sketch left"
ui_create_sketch F3
ui_expect_new "In front of the sketch plane: F2.b0 0.0" "the next sketch hides above too"

ui_finish "UI sketch hide test"
