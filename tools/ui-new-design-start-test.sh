#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/OcctViewer.cpp app/OcctViewer.hpp app/MainWindow.cpp app/framework/DocumentSession.cpp
# check-all sources: app/framework/DocumentSession.hpp core/model/src/document/display.rs app/view/DefaultView.hpp
# How a new design opens (mitcad#1683, mitcad#1809), through the real UI:
#   - The untitled design at the start shows its origin, so that a beginner
#     finds it for the first sketch: the design's origin shown in the view
#     and the Origin folder's light bulb on, with no undo step (the title
#     has no "*").
#   - It looks from the front right and above, Z up, along the default view
#     (mitcad#1698: a few degrees off the isometric diagonal), not at Top
#     (mitcad#1809: mitcad#1683 opened new designs looking at Top, which was
#     not wanted), with the origin in the middle of the view's free area:
#     "View: new design looks along -0.568,0.654,-0.5, up -0.328,0.377,0.866;
#     the origin at <x>,<y>, the free area's centre at <x>,<y>" (within
#     three pixels), logged once the new design's framing is made on the
#     next frame, and the camera at rest along it ("Camera direction").
#   - Create Sketch does not turn that view: every origin plane is picked by
#     a click on its own area (YZ, XZ, XY), and each sketch, finished,
#     returns to the default view ("Sketch left with the view before it").
#   - A view turned to Front (View > Front, on F8 here) before the first
#     sketch keeps Front through Create Sketch and after Finish Sketch.
#   - A design read from a file keeps the origin it saved: one saved with
#     the origin hidden opens with it hidden and the light bulb off, one
#     saved with it shown opens with it shown and the light bulb on. Both
#     are seen from the default view, and no new design's framing is logged
#     for them.
#   - New Design (Ctrl+N, in the current window) after an opened design
#     turned to Top (View > Top) opens from the default view again, not at
#     Top, with the origin shown.
#
# The other UI tests keep the start they were written for
# (MITCAD_NEW_DESIGN_VIEW=0, ui-test-lib.sh: the exact isometric view, the
# origin hidden); this one sets it to 1.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-new-design-start-test.sh

source "$(dirname "$0")/ui-test-lib.sh"
export MITCAD_NEW_DESIGN_VIEW=1

WORK=$(mktemp -d /tmp/mitcad-ui-new-design-start.XXXXXX)
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT

# The default view's direction (view/DefaultView.hpp: azimuth 41°,
# elevation 30°) and its up (Z up, square to it), as "View: new design
# looks along", "Camera direction" and "Sketch left with" log them.
DEFAULT_LOOK="-0.568,0.654,-0.5, up -0.328,0.377,0.866; "
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

# expect_default_view description: the last "View: new design looks along"
# line since the mark looks along the default view with Z up and puts the
# origin within three pixels of the free area's centre, and the camera
# comes to rest along the default view.
expect_default_view() {
  local line
  ui_expect_new "View: new design looks along " "$1: the new design's view is logged"
  line=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -o "View: new design looks along .*" | tail -1)
  [[ $line == "View: new design looks along $DEFAULT_LOOK"* ]] ||
    ui_fail "$1: not the default view from the front right and above: '$line'"
  awk -v line="$line" 'BEGIN {
    if (!match(line, /origin at -?[0-9]+,-?[0-9]+/)) exit 1
    split(substr(line, RSTART + 10, RLENGTH - 10), o, ",")
    if (!match(line, /centre at -?[0-9]+,-?[0-9]+/)) exit 1
    split(substr(line, RSTART + 10, RLENGTH - 10), c, ",")
    exit !((o[1] - c[1]) ^ 2 + (o[2] - c[2]) ^ 2 <= 9)
  }' || ui_fail "$1: the origin is not in the middle of the free area: '$line'"
  echo "ok   $1: ${line#View: new design }"
  ui_expect_new "Camera direction $DEFAULT_VIEW up " "$1: the camera rests along the default view"
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
# View > Front and View > Top on keys of their own.
printf '[shortcuts]\nview.front=F8\nview.top=F12\n' >> "$UI_CONFIG/Mitcad/Mitcad.conf"
ui_start_display

echo "--- The untitled design at the start: the default view, the origin shown"
ui_start_app
UI_MARK=0
expect_origin "$SHOWN_ON" "a new design shows its origin, its light bulb on"
expect_default_view "the untitled design at the start"
TITLE=$(ui_window_title)
[ "$TITLE" = "Untitled - MitCad" ] || ui_fail "the new design is modified: '$TITLE'"
echo "ok   showing the origin is no change to the design: '$TITLE'"

echo "--- Create Sketch in the default view: every origin plane picked by a click on it"
for PLANE in yz xz xy; do
  ui_mark
  ui_create_sketch "$PLANE"
  echo "ok   a sketch on $PLANE by a click on its plane in the default view"
  ! grep -q "View: Create Sketch turns" < <(tail -n +$((UI_MARK + 1)) "$UI_LOG") ||
    ui_fail "the view was turned for the plane pick"
  draw_rectangle_and_finish "the sketch on $PLANE"
  ui_expect_new "Sketch left with the view before it: the view looks along $DEFAULT_ALONG" \
    "the sketch on $PLANE returns to the default view"
done
ui_stop_app

echo "--- The view turned to Front before the first sketch keeps Front"
ui_start_app
UI_MARK=0
expect_default_view "the untitled design at the start, again"
ui_mark
ui_step "front view (F8)"                ui_key F8
ui_expect_new "Camera direction 0 1 0" "looking from the front"
ui_create_sketch xz
draw_rectangle_and_finish "a sketch on XZ from Front"
ui_expect_new "Sketch left with the view before it: the view looks along (0.000, 1.000, 0.000)" \
  "the sketch returns to Front, the view the user chose"
ui_stop_app

echo "--- A design saved with its origin hidden"
ui_start_app "$WORK/hidden.mitcad"
expect_origin "$HIDDEN_OFF" "the opened design's origin stays hidden, its light bulb off"
ui_expect_log "Camera direction $DEFAULT_VIEW up " "the opened design is seen from the default view"
! grep -q "View: new design" "$UI_LOG" || ui_fail "an opened design was framed as a new one: $(grep "View: new design" "$UI_LOG" | tail -1)"
echo "ok   no new design's framing for the opened design"
ui_stop_app

echo "--- A design saved with its origin shown, turned to Top, then New Design"
ui_start_app "$WORK/shown.mitcad"
expect_origin "$SHOWN_ON" "the opened design's origin stays shown, its light bulb on"
! grep -q "View: new design" "$UI_LOG" || ui_fail "an opened design was framed as a new one: $(grep "View: new design" "$UI_LOG" | tail -1)"
ui_mark
ui_step "top view (F12)"                  ui_key F12
ui_expect_new "Camera direction 0 0 -1" "the opened design seen from the top"
ui_mark
ui_step "new design (Ctrl+N)"            ui_key ctrl+n
ui_expect_new "New design in " "New Design replaced the opened design"
expect_default_view "New Design after a design turned to Top"
expect_origin "$SHOWN_ON" "the new design shows its origin, its light bulb on"
TITLE=$(ui_window_title)
[ "$TITLE" = "Untitled - MitCad" ] || ui_fail "the new design is modified: '$TITLE'"
echo "ok   the new design is unmodified: '$TITLE'"

echo "PASS"
