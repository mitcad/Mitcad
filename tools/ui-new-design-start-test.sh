#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/OcctViewer.cpp app/OcctViewer.hpp app/MainWindow.cpp app/framework/DocumentSession.cpp
# check-all sources: app/framework/DocumentSession.hpp core/model/src/document/display.rs app/view/DefaultView.hpp
# How a new design opens (mitcad#1683), through the real UI, so that a
# beginner finds the origin for the first sketch:
#   - The untitled design at the start shows its origin: the design's origin
#     shown in the view and the Origin folder's light bulb on, with no undo
#     step (the title has no "*").
#   - It looks at Top (along -Z, Y up) with the origin in the middle of the
#     view's free area ("View: new design looks along 0,0,-1, up 0,1,0; the
#     origin at <x>,<y>, the free area's centre at <x>,<y>", within three
#     pixels).
#   - Create Sketch turns the view off Top to the default view before the
#     plane pick (mitcad#1698: "View: Create Sketch turns a new design's
#     Top to the default view", the camera then along $DEFAULT_VIEW, a few
#     degrees off the isometric diagonal), so that every origin plane can
#     be picked: a click on YZ, edge-on from Top, starts a sketch on YZ; a
#     rectangle drawn and Finish Sketch return to the default view ("Sketch
#     left with the view before it" along it), with no second turn. Then a
#     sketch on XZ and one on XY, each by a click on its own plane in that
#     view.
#   - A view turned to Front (View > Front, on F8 here) before the first
#     sketch keeps Front through Create Sketch and after Finish Sketch, and
#     nothing turns it.
#   - A design read from a file keeps the origin it saved: one saved with
#     the origin hidden opens with it hidden and the light bulb off, one
#     saved with it shown opens with it shown and the light bulb on. Both
#     open in the default view, not the start's Top, and no new design's
#     framing is logged for them.
#   - New Design (Ctrl+N, in the current window) after an opened design
#     looks at Top again with the origin shown.
#
# The other UI tests keep the start they were written for
# (MITCAD_NEW_DESIGN_VIEW=0, ui-test-lib.sh); this one sets it to 1.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-new-design-start-test.sh

source "$(dirname "$0")/ui-test-lib.sh"
export MITCAD_NEW_DESIGN_VIEW=1

WORK=$(mktemp -d /tmp/mitcad-ui-new-design-start.XXXXXX)
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT

# The default view's direction (view/DefaultView.hpp: azimuth 41°,
# elevation 30°), as "Camera direction" and "Sketch left with" log it.
DEFAULT_VIEW="-0.568 0.654 -0.5"
DEFAULT_ALONG="(-0.568, 0.654, -0.500)"
SHOWN_ON="Design origin shown in the view; the Origin folder's light bulb on"
HIDDEN_OFF="Design origin hidden in the view; the Origin folder's light bulb off"

# expect_origin state description: within six seconds the last "Design
# origin" line is the state.
expect_origin() {
  local last
  for _ in $(seq 1 30); do
    ui_sync
    last=$(grep -o "Design origin .*" "$UI_LOG" | tail -1)
    [ "$last" = "$1" ] && break
    sleep 0.2
  done
  [ "$last" = "$1" ] || ui_fail "$2: the last origin line is '$last', expected '$1'"
  echo "ok   $2"
}

# expect_top description: the last "View: new design looks along" line
# since the mark looks along -Z with Y up, and puts the origin within three
# pixels of the free area's centre.
expect_top() {
  local line
  ui_expect_new "View: new design looks along " "$1: the new design's view is logged"
  line=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -o "View: new design looks along .*" | tail -1)
  [[ $line == "View: new design looks along 0,0,-1, up 0,1,0; "* ]] ||
    ui_fail "$1: not looking at Top: '$line'"
  awk -v line="$line" 'BEGIN {
    if (!match(line, /origin at -?[0-9]+,-?[0-9]+/)) exit 1
    split(substr(line, RSTART + 10, RLENGTH - 10), o, ",")
    if (!match(line, /centre at -?[0-9]+,-?[0-9]+/)) exit 1
    split(substr(line, RSTART + 10, RLENGTH - 10), c, ",")
    exit !((o[1] - c[1]) ^ 2 + (o[2] - c[2]) ^ 2 <= 9)
  }' || ui_fail "$1: the origin is not in the middle of the free area: '$line'"
  echo "ok   $1: ${line#View: new design }"
}

# draw_rectangle_and_finish description: a rectangle from the sketch point
# (-10, -10) to (-40, -30), then Finish Sketch (Ctrl+Enter); the mark is set
# before the finish. Left of and below the origin, away from the orientation
# cube in the view's top right corner: View > Front fits the origin planes
# closer than the default view's fit, and in a small view a point at
# (40, 30) lies on the cube (mitcad#1712).
draw_rectangle_and_finish() {
  ui_mark
  ui_step "rectangle tool (R)"             ui_key r
  ui_expect_new "Sketch tool 2-Point Rectangle" "$1: the rectangle tool"
  ui_step "first corner"                   ui_sketch_click -10 -10
  ui_step "opposite corner"                ui_sketch_click -40 -30
  ui_expect_new "Added rectangle" "$1: a rectangle"
  ui_step "end the tool (Esc)"             ui_key Escape
  ui_mark
  ui_step "finish sketch (Ctrl+Enter)"     ui_key ctrl+Return
  ui_expect_new "Sketch finished" "$1: finished"
}

# A box saved with the origin hidden (no display state) and one saved with
# it shown.
design() {
  cat << EOF
{
  "format": "mitcad", "version": 2,
  "units": {"length": "mm", "angle": "deg"},
  "parameters": [
    {"name": "d1", "expression": "40 mm", "unit": "mm", "owner": "F1"},
    {"name": "d2", "expression": "25 mm", "unit": "mm", "owner": "F1"},
    {"name": "d3", "expression": "10 mm", "unit": "mm", "owner": "F1"}
  ],
  "features": [
    {"uid": "F1", "name": "Box1", "type": "box", "plane": "xy", "corner": [0, 0],
     "length": "d1", "width": "d2", "height": "d3", "operation": "new_body"}
  ]$1
}
EOF
}
design "" > "$WORK/hidden.mitcad"
design ', "display": {"origin": true}' > "$WORK/shown.mitcad"

# New replaces the window's design (no second window to follow).
ui_open_in_current_window
# View > Front on a key of its own.
printf '[shortcuts]\nview.front=F8\n' >> "$UI_CONFIG/Mitcad/Mitcad.conf"
ui_start_display

echo "--- The untitled design at the start: Top, the origin shown"
ui_start_app
UI_MARK=0
expect_origin "$SHOWN_ON" "a new design shows its origin, its light bulb on"
expect_top "the untitled design at the start"
TITLE=$(ui_window_title)
[ "$TITLE" = "Untitled - MitCad" ] || ui_fail "the new design is modified: '$TITLE'"
echo "ok   showing the origin is no change to the design: '$TITLE'"

echo "--- Create Sketch from Top: the default view first, every origin plane picked"
ui_mark
ui_command "Create Sketch"
ui_expect_new "Create Sketch: select a plane" "Create Sketch asks for a plane"
ui_expect_new "View: Create Sketch turns a new design's Top to the default view" \
  "the view turns off Top before the plane pick"
ui_expect_new "Camera direction $DEFAULT_VIEW up " "the default view: from the front right and above"
# The planes' pick places are logged again as the camera rests (when they
# moved), right after its line: the last ones logged are the default view's.
ui_sync
BEFORE=$(grep -c "Sketch started on yz" "$UI_LOG")
ui_step "click YZ"                       ui_click_logged "Datum yz"
for _ in $(seq 1 25); do
  [ "$(grep -c "Sketch started on yz" "$UI_LOG")" -gt "$BEFORE" ] && break
  sleep 0.2
done
[ "$(grep -c "Sketch started on yz" "$UI_LOG")" -gt "$BEFORE" ] ||
  ui_fail "a click on YZ in the default view started no sketch on it"
echo "ok   YZ, edge-on from Top, picked by a click on it in the default view"
draw_rectangle_and_finish "the first sketch, on YZ"
ui_expect_new "Sketch left with the view before it: the view looks along $DEFAULT_ALONG" \
  "the first sketch returns to the default view"
! grep -q "View: after the first sketch" < <(tail -n +$((UI_MARK + 1)) "$UI_LOG") ||
  ui_fail "the view turned again after the first sketch"
echo "ok   no second turn after the first sketch"
for PLANE in xz xy; do
  ui_create_sketch "$PLANE"
  echo "ok   a sketch on $PLANE by a click on its plane in the default view"
  draw_rectangle_and_finish "the sketch on $PLANE"
  ui_expect_new "Sketch left with the view before it: the view looks along $DEFAULT_ALONG" \
    "the sketch on $PLANE returns to the default view"
done
ui_stop_app

echo "--- The view turned to Front before the first sketch keeps Front"
ui_start_app
UI_MARK=0
expect_top "the untitled design at the start, again"
ui_step "front view (F8)"                ui_key F8
ui_sync
ui_create_sketch xz
! grep -q "View: Create Sketch turns" < <(tail -n +$((UI_MARK + 1)) "$UI_LOG") ||
  ui_fail "a view turned to Front was turned to the default view for the plane pick"
echo "ok   no turn for the plane pick in a view turned to Front"
draw_rectangle_and_finish "a sketch on XZ from Front"
ui_expect_new "Sketch left with the view before it: the view looks along (0.000, 1.000, 0.000)" \
  "the sketch returns to Front, the view the user chose"
! grep -q "View: after the first sketch" < <(tail -n +$((UI_MARK + 1)) "$UI_LOG") ||
  ui_fail "a view turned to Front was turned to the isometric view after the sketch"
echo "ok   no turn to the isometric view after the sketch"
ui_stop_app

echo "--- A design saved with its origin hidden"
ui_start_app "$WORK/hidden.mitcad"
expect_origin "$HIDDEN_OFF" "the opened design's origin stays hidden, its light bulb off"
ui_expect_log "View: an opened design seen from the default view" "the opened design is seen from the default view"
ui_expect_log "Camera direction $DEFAULT_VIEW up " "the opened design looks along the default view"
! grep -q "View: new design" "$UI_LOG" || ui_fail "an opened design was framed as a new one: $(grep "View: new design" "$UI_LOG" | tail -1)"
echo "ok   no new design's framing for the opened design"
ui_stop_app

echo "--- A design saved with its origin shown, then New Design"
ui_start_app "$WORK/shown.mitcad"
expect_origin "$SHOWN_ON" "the opened design's origin stays shown, its light bulb on"
! grep -q "View: new design" "$UI_LOG" || ui_fail "an opened design was framed as a new one: $(grep "View: new design" "$UI_LOG" | tail -1)"
ui_mark
ui_step "new design (Ctrl+N)"            ui_key ctrl+n
ui_expect_new "New design in " "New Design replaced the opened design"
expect_top "New Design after an opened design"
expect_origin "$SHOWN_ON" "the new design shows its origin, its light bulb on"
TITLE=$(ui_window_title)
[ "$TITLE" = "Untitled - MitCad" ] || ui_fail "the new design is modified: '$TITLE'"
echo "ok   the new design is unmodified: '$TITLE'"

echo "PASS"
