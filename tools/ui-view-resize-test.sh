#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/OcctViewer.cpp app/OcctViewer.hpp tools/ui-image-stats.py
# check-all sources: app/framework/GraphicsStart.cpp app/framework/DisplayPlatform.cpp
# check-all sources: app/DocumentFrame.cpp app/ViewerVisibility.cpp
# The 3D view follows a resized window (mitcad#2422, GitHub
# mitcad/Mitcad#672): on X11 with GLX, where the view draws in its own EGL
# context and is read back into the window (mitcad#2292), a window made
# larger kept the old picture size with black margins beside it, one made
# smaller cut it off, and the picks stayed at the old size, away from what
# the view showed, until MitCad was started again. Two runs, each with an
# empty design and its origin shown:
#   egl  Qt's own EGL context (X11 with Qt's EGL integration, the default),
#        which OCCT draws with;
#   glx  X11 with GLX (QT_XCB_GL_INTEGRATION=xcb_glx, the start-up's
#        fallback), the view in its own EGL context.
# Each run makes the window larger, smaller, as large as the screen (what a
# maximize does: Xvfb has no window manager to maximize it) and its first
# size again, and checks after each:
#   - "View size:" logs the view's size in device pixels as the size OCCT
#     draws and picks at (and the frame read back, with GLX), and no
#     "3D view: drawn at ..." warning says they differ;
#   - the origin's place (Datum origin, projected by OCCT at its size)
#     moved with the view: a resize keeps the camera's centre in the view's
#     middle and the height the view shows (mitcad#1000), so the origin's
#     offset from the view's middle scales with the view's height. A view
#     whose drawn size stayed would keep the origin where it was;
#   - made larger: the view's right and bottom edges are drawn, not black;
#   - the XY plane, clicked where the view says it is, is picked.
# Then two designs in the window's tabs (mitcad#2471), each with a view of
# its own (on the GLX path in an EGL context of its own): the first design
# edited so that New Design opens a second tab beside it, the window made
# larger, the second design opened, the first chosen again (Ctrl+Tab), the
# window made smaller with both open, and the tabs switched twice. A view
# hidden in its tab follows the window when it is shown again: after each
# step the shown view's sizes, the origin's place against that design's own
# last one, the edges drawn and a pick of the XY plane. The GLX run does
# the two designs once more in the Floating layout, the one the report's
# session log shows, where the view fills the window under the floating
# panels (in both layouts the tabs are in the top bar's title row, so a
# tab switch alone does not resize the view). The UI tests start new
# designs with their origin hidden (MITCAD_NEW_DESIGN_VIEW=0): the test
# shows each design's origin by its light bulb.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-view-resize-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
stats_tool=$here/ui-image-stats.py
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-view-resize.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
unset MITCAD_GRAPHICS_FALLBACK MITCAD_GRAPHICS_RESTARTS MITCAD_GRAPHICS_TRIED QT_XCB_GL_INTEGRATION QT_QPA_PLATFORM
# The origin's places where a click picks them (MainWindow::logDatumPlaces).
export MITCAD_LOG_PICKS=1

# stats x y width height: ui-image-stats.py's counts of a screen region as
# "pixels dark".
stats() {
  local json
  xwd -root -silent > "$work/screen.xwd" || ui_fail 'no screen dump'
  json=$(python3 "$stats_tool" "$work/screen.xwd" "$1" "$2" "$3" "$4") || ui_fail 'no image statistics'
  python3 -c 'import json, sys; s = json.load(sys.stdin); print(s["pixels"], s["dark"])' <<< "$json"
}

# view_area: the last "View area x y w h" (window coordinates, logical pixels).
view_area() {
  grep -o "View area [0-9]* [0-9]* [0-9]* [0-9]*" "$UI_LOG" | tail -1 | cut -d' ' -f3-
}

# origin_at: the last "Datum origin at x,y" as "x y".
origin_at() {
  grep -o "^Datum origin at [0-9-]*,[0-9-]*" "$UI_LOG" | tail -1 | sed 's/^Datum origin at //; s/,/ /'
}

# expect_sizes run step: within eight seconds a "View size:" line since the
# mark whose widget size is the view area's and whose drawn size (and frame
# read back) is its size in device pixels; no warning that they differ.
expect_sizes() {
  local line="" widget device drawn back aw ah
  for _ in $(seq 1 40); do
    ui_sync
    line=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -o "^View size: .*" | tail -1)
    [ -n "$line" ] && break
    sleep 0.2
  done
  [ -n "$line" ] || ui_fail "$1 $2: no 'View size:' line after the resize"
  read -r _ _ aw ah <<< "$(view_area)"
  widget=$(sed -n 's/^View size: \([0-9]*x[0-9]*\),.*/\1/p' <<< "$line")
  device=$(sed -n 's/.*, device pixels \([0-9]*x[0-9]*\),.*/\1/p' <<< "$line")
  drawn=$(sed -n 's/.*, drawn \([0-9]*x[0-9]*\).*/\1/p' <<< "$line")
  back=$(sed -n 's/.*, read back \([0-9]*x[0-9]*\).*/\1/p' <<< "$line")
  [ "$widget" = "${aw}x${ah}" ] || ui_fail "$1 $2: '$line', but the view area is ${aw}x${ah}"
  [ -n "$device" ] && [ "$drawn" = "$device" ] ||
    ui_fail "$1 $2: '$line': OCCT draws and picks at another size than the view's"
  if [[ $1 == glx* ]]; then
    [ "$back" = "$device" ] || ui_fail "$1 $2: '$line': the frame read back is not the view's size"
  fi
  tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep "^3D view: drawn at " &&
    ui_fail "$1 $2: the view warned that its drawn size is not the window's"
  echo "ok   $1 $2: $line"
}

# expect_origin run step before-area before-origin: the origin's place
# moved with the view (within 3 px and 2 % of its offset from the middle),
# up to six seconds after the view rested.
expect_origin() {
  local vx0 vy0 vw0 vh0 ox0 oy0 vx1 vy1 vw1 vh1 ox1 oy1 verdict=""
  read -r vx0 vy0 vw0 vh0 <<< "$3"
  read -r ox0 oy0 <<< "$4"
  for _ in $(seq 1 30); do
    ui_sync
    read -r vx1 vy1 vw1 vh1 <<< "$(view_area)"
    read -r ox1 oy1 <<< "$(origin_at)"
    verdict=$(awk -v vx0="$vx0" -v vy0="$vy0" -v vw0="$vw0" -v vh0="$vh0" -v ox0="$ox0" -v oy0="$oy0" \
      -v vx1="$vx1" -v vy1="$vy1" -v vw1="$vw1" -v vh1="$vh1" -v ox1="$ox1" -v oy1="$oy1" 'BEGIN {
        k = vh1 / vh0
        dx = (ox0 - (vx0 + vw0 / 2)) * k
        dy = (oy0 - (vy0 + vh0 / 2)) * k
        ex = vx1 + vw1 / 2 + dx
        ey = vy1 + vh1 / 2 + dy
        tx = 3 + 0.02 * (dx < 0 ? -dx : dx)
        ty = 3 + 0.02 * (dy < 0 ? -dy : dy)
        ok = (ox1 - ex) ^ 2 <= tx ^ 2 && (oy1 - ey) ^ 2 <= ty ^ 2
        printf "%s %.0f,%.0f", ok ? "ok" : "off", ex, ey
      }')
    [[ $verdict == ok* ]] && break
    sleep 0.2
  done
  [[ $verdict == ok* ]] ||
    ui_fail "$1 $2: the origin is at $ox1,$oy1, expected about ${verdict#off } (view ${vw0}x${vh0} -> ${vw1}x${vh1}): the view did not follow the window"
  echo "ok   $1 $2: the origin moved with the view to $ox1,$oy1 (expected ${verdict#ok })"
}

# expect_edges_drawn run step: the view's right and bottom edges are not
# black where its middle is not (a picture of the old size left margins).
expect_edges_drawn() {
  local x0 y0 x1 y1 pixels dark mpixels mdark band bx0 by0 bx1 by1 name
  read -r x0 y0 <<< "$(ui_view_at 40 40)"
  read -r x1 y1 <<< "$(ui_view_at 60 60)"
  read -r mpixels mdark <<< "$(stats "$x0" "$y0" $((x1 - x0)) $((y1 - y0)))"
  [ -n "$mpixels" ] && [ "$mpixels" -gt 0 ] || ui_fail "$1 $2: no pixels in the view's middle"
  for band in "92 40 97 60 right" "5 93 25 98 bottom"; do
    read -r bx0 by0 bx1 by1 name <<< "$band"
    read -r x0 y0 <<< "$(ui_view_at "$bx0" "$by0")"
    read -r x1 y1 <<< "$(ui_view_at "$bx1" "$by1")"
    read -r pixels dark <<< "$(stats "$x0" "$y0" $((x1 - x0)) $((y1 - y0)))"
    [ -n "$pixels" ] && [ "$pixels" -gt 0 ] || ui_fail "$1 $2: no pixels at the view's $name edge"
    if [ $((dark * 100)) -ge $((pixels * 95)) ] && [ $((mdark * 100)) -lt $((mpixels * 95)) ]; then
      ui_fail "$1 $2: the view's $name edge is black ($dark of $pixels pixels dark): the picture kept its old size"
    fi
    echo "ok   $1 $2: the view's $name edge is drawn ($dark of $pixels pixels dark)"
  done
}

# resize_to run step width height [grown]: the window resized (screen
# pixels), then its checks.
resize_to() {
  local area origin
  area=$(view_area)
  origin=$(origin_at)
  [ -n "$origin" ] || ui_fail "$1 $2: the origin's place is not logged"
  ui_mark
  ui_step "$1: $2 (${3}x$4)" xdotool windowsize "$UI_WINDOW" "$3" "$4"
  xdotool windowmove "$UI_WINDOW" 0 0 2> /dev/null
  ui_sync
  eval "$(xdotool getwindowgeometry --shell "$UI_WINDOW")"
  expect_sizes "$1" "$2"
  expect_origin "$1" "$2" "$area" "$origin"
  [ "${5:-}" = grown ] && expect_edges_drawn "$1" "$2"
  return 0
}

# pick_xy run step: the XY plane, clicked where the view says a click picks
# it, is selected.
pick_xy() {
  ui_mark
  ui_step "$1: click XY ($2)" ui_click_logged "Datum xy"
  ui_expect_new "Selected: 1 plane" "$1 $2: the XY plane picked where the view shows it"
  ui_step "$1: clear the selection (Esc)" ui_key Escape
}

# expect_since run what regex: within six seconds a line since the mark
# that matches the regular expression.
expect_since() {
  for _ in $(seq 1 30); do
    grep -qE -- "$3" < <(tail -n +$((UI_MARK + 1)) "$UI_LOG") && { echo "ok   $1: $2"; return 0; }
    sleep 0.2
  done
  ui_fail "$1: $2: no line '$3' since the mark"
}

# expect_shown run design: the window's title is the first design's
# (changed: "Untitled* - MitCad") or the second's (unchanged) within four
# seconds: the tab shown is that design's.
expect_shown() {
  local title=""
  for _ in $(seq 1 20); do
    title=$(ui_window_title)
    if [ "$2" = 1 ]; then
      [[ $title == *"* - MitCad" ]] && break
    else
      [[ $title == *" - MitCad" && $title != *"* - MitCad" ]] && break
    fi
    sleep 0.2
  done
  if [ "$2" = 1 ]; then
    [[ $title == *"* - MitCad" ]] || ui_fail "$1: the first design's tab is not shown (title '$title')"
  else
    [[ $title == *" - MitCad" && $title != *"* - MitCad" ]] ||
      ui_fail "$1: the second design's tab is not shown (title '$title')"
  fi
  echo "ok   $1: design $2's tab shown ('$title')"
}

# The view's area and the origin's place of each design as it was last
# shown (two_tabs), and the design shown now.
declare -A tab_area tab_origin
shown_design=1

# keep_shown: the shown design's view area and origin's place kept, before
# its tab is hidden.
keep_shown() {
  tab_area[$shown_design]=$(view_area)
  tab_origin[$shown_design]=$(origin_at)
  [ -n "${tab_origin[$shown_design]}" ] || ui_fail "design $shown_design: the origin's place is not logged"
}

# shown_tab: where the shown design's tab is, from the last "Document tab
# shown at x,y" (DocumentFrame::logShownTab), as "x,y".
shown_tab() {
  grep -o "^Document tab shown at [0-9-]*,[0-9-]*" "$UI_LOG" | tail -1 | sed 's/^Document tab shown at //'
}

# switch_tab run step: Ctrl+Tab shows the other design's tab. Its view,
# hidden while the window changed, follows the window: its sizes, the
# origin's place against the one it had when it was last shown, the edges
# drawn and a pick of the XY plane.
switch_tab() {
  local other=$((3 - shown_design))
  keep_shown
  local before now=""
  before=$(shown_tab)
  ui_mark
  ui_step "$1: $2 (Ctrl+Tab)" ui_key ctrl+Tab
  ui_sync
  # The other design's tab shown: the shown tab's place moved to it (both
  # designs are changed by now, so their titles are alike).
  for _ in $(seq 1 20); do
    now=$(shown_tab)
    [ -n "$now" ] && [ "$now" != "$before" ] && break
    sleep 0.2
  done
  [ -n "$now" ] && [ "$now" != "$before" ] ||
    ui_fail "$1 $2: Ctrl+Tab did not show the other design's tab (the shown tab stays at '$before')"
  echo "ok   $1 $2: design $other's tab shown, at $now (was at $before)"
  shown_design=$other
  eval "$(xdotool getwindowgeometry --shell "$UI_WINDOW")"
  expect_since "$1 $2" "the view is shown again" "^Document viewer .*: visible, frames "
  expect_sizes "$1" "$2"
  expect_origin "$1" "$2" "${tab_area[$other]}" "${tab_origin[$other]}"
  expect_edges_drawn "$1" "$2"
  pick_xy "$1" "$2"
}

# two_tabs run larger-width larger-height: two designs in the window's tabs
# (mitcad#2471, GitHub mitcad/Mitcad#672), each view checked after the
# window changed and as its tab is shown again.
two_tabs() {
  shown_design=1
  # A change to the first design (its origin's light bulb off and on, two
  # undo steps): New Design opens beside a design, but replaces the
  # untouched untitled one (WindowManager::reusable).
  ui_mark
  ui_step "$1: hide the origin (light bulb)" ui_click_logged "Browser eye Root/Origin"
  ui_expect_new "Visibility Root/Origin: hidden" "$1: the first design's origin hidden"
  ui_mark
  ui_step "$1: show the origin again (light bulb)" ui_click_logged "Browser eye Root/Origin"
  ui_expect_new "Visibility Root/Origin: shown" "$1: the first design's origin shown again"
  ui_expect_new "Datum origin at" "$1: the origin in the view again"
  ui_sync
  expect_shown "$1 the first design changed" 1
  resize_to "$1" "one design, larger" "$2" "$3" grown
  pick_xy "$1" "one design, larger"
  keep_shown
  ui_mark
  ui_step "$1: new design (Ctrl+N)" ui_key ctrl+n
  ui_expect_new "New document" "$1: a new design"
  expect_since "$1" "the new design opened beside the first, not in its place" \
    "^Document window created: .* \(2 open\)$"
  expect_since "$1" "both designs' tabs shown" "^Document tabs shown: '[^']*' '[^']*'$"
  expect_shown "$1 second design" 2
  shown_design=2
  eval "$(xdotool getwindowgeometry --shell "$UI_WINDOW")"
  # Its origin shown by its light bulb, as the first design's: the UI tests
  # start new designs without it (MITCAD_NEW_DESIGN_VIEW=0, ui-test-lib.sh).
  # A change, so its title gets the change mark too.
  ui_show_origin
  ui_expect_new "Datum origin at" "$1 second design: its origin in the view"
  expect_sizes "$1" "second design opened"
  expect_edges_drawn "$1" "second design opened"
  pick_xy "$1" "second design opened"
  switch_tab "$1" "back to the first design"
  resize_to "$1" "two designs, smaller" $((900 * UI_SCALE)) $((680 * UI_SCALE))
  pick_xy "$1" "two designs, smaller"
  switch_tab "$1" "the second design, hidden while the window became smaller"
  switch_tab "$1" "the first design again, the window as it was"
}

# run name: one start, the window resized and checked.
run() {
  local width height screen_width screen_height larger_width larger_height
  ui_start_app --no-mcp
  if [ "$1" = glx ]; then
    ui_expect_log '3D view: drawn in its own EGL context and read back into the window' \
      "$1: the view draws in its own EGL context"
  else
    ui_expect_log 'View composition: ' "$1: the view's composition is logged"
    grep '^View composition: ' "$UI_LOG" | tail -1 | grep -q "own EGL context" &&
      ui_fail "$1: the view draws in its own EGL context, not with Qt's"
  fi
  # "View size:" once the view drew at its first size (": first frame" is
  # an open's line; this run opens no file).
  ui_expect_log 'View size: ' "$1: the view drew its first frame"
  ui_show_origin
  ui_sync
  width=$WIDTH
  height=$HEIGHT
  read -r screen_width screen_height <<< "$(xdotool getdisplaygeometry)"
  larger_width=$((width + 300 * UI_SCALE > screen_width - 40 * UI_SCALE ? screen_width - 40 * UI_SCALE : width + 300 * UI_SCALE))
  larger_height=$((height + 150 * UI_SCALE > screen_height - 40 * UI_SCALE ? screen_height - 40 * UI_SCALE : height + 150 * UI_SCALE))
  resize_to "$1" "larger" "$larger_width" "$larger_height" grown
  pick_xy "$1" "larger"
  resize_to "$1" "smaller" $((900 * UI_SCALE)) $((680 * UI_SCALE))
  pick_xy "$1" "smaller"
  resize_to "$1" "as large as the screen" "$screen_width" "$screen_height" grown
  pick_xy "$1" "as large as the screen"
  resize_to "$1" "its first size again" "$width" "$height"
  pick_xy "$1" "its first size again"
  two_tabs "$1" "$larger_width" "$larger_height"
  grep -q '^3D view: no graphics: ' "$UI_LOG" && ui_fail "$1: the view logged a graphics failure"
  ui_stop_app
}

# floating_tabs name: the two designs again in the Floating layout, the
# default one the report's session log shows (GitHub mitcad/Mitcad#672):
# the view fills the window under the floating panels, the designs' tabs
# in the top bar's title row.
floating_tabs() {
  local screen_width screen_height
  ui_start_app --no-mcp --chrome=floating
  [ "$1" = glx ] && ui_expect_log '3D view: drawn in its own EGL context and read back into the window' \
    "$1 floating: the view draws in its own EGL context"
  ui_expect_log 'View size: ' "$1 floating: the view drew its first frame"
  ui_show_origin
  ui_sync
  read -r screen_width screen_height <<< "$(xdotool getdisplaygeometry)"
  two_tabs "$1 floating" \
    $((WIDTH + 300 * UI_SCALE > screen_width - 40 * UI_SCALE ? screen_width - 40 * UI_SCALE : WIDTH + 300 * UI_SCALE)) \
    $((HEIGHT + 150 * UI_SCALE > screen_height - 40 * UI_SCALE ? screen_height - 40 * UI_SCALE : HEIGHT + 150 * UI_SCALE))
  grep -q '^3D view: no graphics: ' "$UI_LOG" && ui_fail "$1 floating: the view logged a graphics failure"
  ui_stop_app
}

ui_start_display

echo '--- egl: X11 with Qt'"'"'s EGL context, which OCCT draws with'
run egl

echo '--- glx: X11 with GLX, the 3D view in its own EGL context, read back'
export QT_XCB_GL_INTEGRATION=xcb_glx
run glx
echo '--- glx, Floating: two designs in the title row'"'"'s tabs, as in the report'
floating_tabs glx
unset QT_XCB_GL_INTEGRATION

ui_finish '3D view and picks follow a resized window, also in two design tabs, with Qt'"'"'s EGL context and the view'"'"'s own'
