#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/view app/OcctViewer.cpp app/OcctViewer.hpp
# An orbit past the edge of the screen on two screens side by side
# (mitcad#2188, GitHub mitcad/Mitcad#546), on a red block: the hidden
# display, 3200 x 1000 logical pixels, is split into two screens of
# 1600 x 1000 (xrandr --setmonitor), the window on the left one. A middle
# drag orbits (the navigation scheme middle-orbit); each orbit's turn is
# checked against the length of its moves, from a short orbit's degrees per
# pixel, so a jump the view noticed (a step of a screen's width, about half
# a turn) fails:
#   - over the shared edge in one move (onto the right screen), then on from
#     the left screen's left edge past its middle and back past it again:
#     the cursor stays on the left screen and jumps once. At device pixel
#     ratio 2 the right screen's global positions start at 3200 (Qt keeps a
#     screen's origin in device pixels): the jump is taken in the widget's
#     coordinates;
#   - over the left screen's outer (left) edge, and over its top;
#   - the cursor put back at the shared edge after the jump's own move at
#     the target arrived, as XWayland does: the jump is undone, the drag
#     makes no other, and back past the middle the view turns by the moves
#     only;
#   - four bursts of fast moves over the shared edge, a move every
#     millisecond: the jump is kept and the view turns by the moves it got
#     (mitcad#2205: OrbitWrap tells a move queued before the jump by its
#     timestamp alone, so a move from the old edge stamped after the one
#     that jumped would undo the jump and fail here; noted is how often the
#     server still had moves to carry out when it jumped).
# Runs at device pixel ratio 1 and again at 2 (UI_SCALE, QT_SCALE_FACTOR).
#
# Runs headless on Xvfb (see ui-test-lib.sh); needs xrandr, and is skipped
# (exit 0, a "skipped: xrandr missing" line) where it is not installed, as
# in a release build root without x11-xserver-utils.
# Usage: tools/ui-orbit-screens-test.sh
# check-all sources: tools/cli tools/ui-test-lib.sh

# Before the two runs below, so that the skip is said once.
if ! command -v xrandr > /dev/null; then
  echo "skipped: xrandr missing (apt-get install x11-xserver-utils); the two screens cannot be set up"
  exit 0
fi

if [ -z "${UI_SCALE:-}" ]; then
  UI_SCALE=1 bash "$0" "$@" || exit 1
  UI_SCALE=2 bash "$0" "$@"
  exit
fi

# Two screens' width, split below.
UI_SCREEN_WIDTH=3200
source "$(dirname "$0")/ui-test-lib.sh"

CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
# In the test's own cache, which ui_cleanup removes.
WORK=$UI_CACHE/orbit-screens
mkdir -p "$WORK"
S=$UI_SCALE
R='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'

cat > "$WORK/block.json" << EOF
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 60, "height": 40},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$R"}],
    "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
  {"cmd": "set_body_appearance", "uid": "F2.b0", "appearance": "paint_red"}
]
EOF
"$CLI" run "$WORK/block.json" --save "$WORK/block.mitcad" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

mkdir -p "$UI_CONFIG/Mitcad"
printf '[view]\nnavigationScheme=middle-orbit\n' > "$UI_CONFIG/Mitcad/Mitcad.conf"

ui_start_display
# The left screen has the display's output, so it is Qt's primary screen,
# where the window opens; the right one is a monitor of its own beside it.
xrandr --setmonitor mitcad-left $((1600 * S))/423x$((1000 * S))/265+0+0 screen > /dev/null &&
  xrandr --setmonitor mitcad-right $((1600 * S))/423x$((1000 * S))/265+$((1600 * S))+0 none > /dev/null ||
  ui_fail "the display could not be split into two screens (xrandr --setmonitor)"
echo "     $(xrandr --listmonitors | tr '\n' ' ')"

ui_start_app --open "$WORK/block.mitcad" --no-native-dialogs
ui_expect_log "Opened $WORK/block.mitcad" "the block opened"
ui_expect_log "Camera direction -0.577 0.577 -0.577" "the camera starts isometric"

# orbit_steps dx dy count: moves in logical pixels, paced like a hand.
orbit_steps() {
  local i
  for ((i = 0; i < $3; i++)); do
    xdotool mousemove_relative -- $(($1 * S)) $(($2 * S))
    sleep 0.03
  done
}
orbit_camera() {
  grep "Camera direction" "$UI_LOG" | tail -1 | sed 's/.*Camera direction \([^ ]*\) \([^ ]*\) \([^ ]*\) up.*/\1 \2 \3/'
}
# orbit_cursor: where the cursor is, in logical pixels.
orbit_cursor() {
  local X Y SCREEN WINDOW
  eval "$(xdotool getmouselocation --shell 2> /dev/null)"
  echo $((X / S)) $((Y / S))
}
# orbit_degrees "x y z" "x y z": the angle between two camera directions.
orbit_degrees() {
  echo "$1 $2" | awk '{ d = $1 * $4 + $2 * $5 + $3 * $6; if (d > 1) d = 1; if (d < -1) d = -1
                        printf "%.1f", atan2(sqrt(1 - d * d), d) * 57.29578 }'
}
# orbit_since pattern: how many lines since the mark have the pattern.
orbit_since() { tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -c -- "$1"; }
orbit_iso="-0.577 0.577 -0.577"
read -r press_x press_y <<< "$(ui_view_at 50 50)"
press_lx=$((press_x / S))
press_ly=$((press_y / S))
# orbit_from_iso: Isometric View, then the middle button down in the view.
orbit_from_iso() {
  ui_mark
  ui_step "isometric view (search)"        ui_command "Isometric View"
  sleep 1
  ui_focus_main
  ui_mark
  ui_apart "$press_x" "$press_y"
  xdotool mousemove "$press_x" "$press_y" mousedown 2
  sleep 0.1
}
# orbit_release what: the button up, and the view at rest.
orbit_release() {
  ui_mark
  xdotool mouseup 2
  ui_expect_new "Camera direction" "the view rests after $1" 15
}
# orbit_turned length degrees-per-pixel what: the view turned from the
# isometric one as far as a drag of that many logical pixels, within 3
# degrees.
orbit_turned() {
  local turned expected
  turned=$(orbit_degrees "$orbit_iso" "$(orbit_camera)")
  expected=$(awk -v l="$1" -v p="$2" 'BEGIN { if (l < 0) l = -l; a = l * p; while (a >= 360) a -= 360
                                              if (a > 180) a = 360 - a; printf "%.1f", a }')
  echo "     $3: $1 pixels, $turned degrees, expected $expected"
  awk -v a="$turned" -v b="$expected" 'BEGIN { d = a - b; if (d < 0) d = -d; exit !(d <= 3) }' ||
    ui_fail "$3: the orbit turned $turned degrees, expected $expected"
  echo "ok   $3: the view turned by the moves only"
}

echo "--- Degrees per pixel, from orbits away from the edges"
orbit_from_iso
orbit_steps 10 0 20
ui_sync
[ "$(orbit_since "Orbit cursor jumped")" = 0 ] || ui_fail "the cursor jumped away from the edges"
screen=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -o "Orbit on screen .*" | tail -1)
echo "     $screen"
[ "$screen" = "Orbit on screen 0,0 1600 x 1000 of 2" ] ||
  ui_fail "Qt does not see the left one of two screens of 1600 x 1000: '$screen'"
echo "ok   the orbit is on the left one of two screens"
orbit_release "a short orbit"
per_x=$(awk -v a="$(orbit_degrees "$orbit_iso" "$(orbit_camera)")" 'BEGIN { printf "%.5f", a / 200 }')
orbit_from_iso
orbit_steps 0 -10 20
orbit_release "a short orbit up"
per_y=$(awk -v a="$(orbit_degrees "$orbit_iso" "$(orbit_camera)")" 'BEGIN { printf "%.5f", a / 200 }')
echo "     $per_x degrees per pixel across, $per_y up"

echo "--- Over the shared edge in one move, on past the middle and back"
orbit_from_iso
xdotool mousemove $((1560 * S)) "$press_y"
sleep 0.1
orbit_steps 60 0 1
orbit_steps 10 0 90
orbit_steps -10 0 50
ui_sync
ui_expect_new "Orbit cursor jumped to 17," "the cursor jumps at the shared edge" 8
echo "     $(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep "Orbit cursor jumped" | tr '\n' ' ')"
[ "$(orbit_since "Orbit cursor jumped")" = 1 ] || ui_fail "the cursor jumped more than once"
[ "$(orbit_since "Orbit cursor jump not taken")" = 0 ] || ui_fail "the jump at the shared edge was undone"
read -r at _ <<< "$(orbit_cursor)"
[ "$at" -ge 397 ] && [ "$at" -le 437 ] ||
  ui_fail "the cursor did not go on from the left screen's left edge: x $at, expected 417"
echo "ok   the cursor went on from the left screen's left edge (x $at)"
orbit_release "the drag over the shared edge"
orbit_turned $((1560 - press_lx + 60 + 900 - 500)) "$per_x" "over the shared edge"

echo "--- Over the outer edge"
orbit_from_iso
xdotool mousemove $((40 * S)) "$press_y"
sleep 0.1
orbit_steps -10 0 30
ui_sync
ui_expect_new "Orbit cursor jumped to 1582," "the cursor jumps at the left screen's outer edge" 8
[ "$(orbit_since "Orbit cursor jumped")" = 1 ] || ui_fail "the cursor jumped more than once"
read -r at _ <<< "$(orbit_cursor)"
[ "$at" -ge 1292 ] && [ "$at" -le 1332 ] ||
  ui_fail "the cursor did not go on from the left screen's shared edge: x $at, expected 1312"
orbit_release "the drag over the outer edge"
orbit_turned $((40 - press_lx - 300)) "$per_x" "over the outer edge"

echo "--- Over the top"
orbit_from_iso
xdotool mousemove "$press_x" $((40 * S))
sleep 0.1
orbit_steps 0 -10 30
ui_sync
ui_expect_new ",982 from " "the cursor jumps at the top of the screen" 8
[ "$(orbit_since "Orbit cursor jumped")" = 1 ] || ui_fail "the cursor jumped more than once"
read -r _ at <<< "$(orbit_cursor)"
[ "$at" -ge 692 ] && [ "$at" -le 732 ] ||
  ui_fail "the cursor did not go on from the bottom of the screen: y $at, expected 712"
orbit_release "the drag over the top"
orbit_turned $((40 - press_ly - 300)) "$per_y" "over the top"

echo "--- The cursor put back after the jump's own move arrived"
orbit_from_iso
xdotool mousemove $((1560 * S)) "$press_y"
sleep 0.1
jumped=""
for ((i = 0; i < 6; i++)); do
  orbit_steps 10 0 1
  ui_sync
  if [ "$(orbit_since "Orbit cursor jumped")" != 0 ]; then
    jumped=1
    break
  fi
done
[ -n "$jumped" ] || ui_fail "the cursor did not jump at the shared edge"
read -r from_x <<< "$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep "Orbit cursor jumped" | tail -1 |
  sed 's/.* from \([0-9-]*\),.*/\1/')"
echo "     $(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep "Orbit cursor jumped" | tail -1)"
# The jump's own move (X11 posts one) reaches the view first.
sleep 0.2
ui_sync
# Then the system puts the cursor back where it was, and on by the next
# move: at the shared edge.
xdotool mousemove $(((from_x + 10) * S)) "$press_y"
sleep 0.1
ui_sync
ui_expect_new "Orbit cursor jump not taken" "the jump put back is undone" 8
orbit_steps -10 0 90
ui_sync
[ "$(orbit_since "Orbit cursor jumped")" = 1 ] || ui_fail "the drag jumped again after the jump was put back"
read -r at _ <<< "$(orbit_cursor)"
[ "$at" -lt 800 ] || ui_fail "the drag did not come back past the middle: x $at"
orbit_release "the drag put back"
orbit_turned $((at - press_lx)) "$per_x" "put back and back past the middle"

echo "--- Bursts of fast moves over the shared edge"
# A move of 3 pixels every millisecond from x 1500 across the shared edge
# and on, 1950 pixels from the screen's left edge in all, so that the
# server still has moves to carry out while the app handles the move that
# jumped (mitcad#2205). Those moves are stamped after it and come from the
# old edge: the timestamp alone takes such a move, when it reaches the view,
# for the cursor put back, which undoes the jump and fails here. How far
# the server moved the cursor between the move that jumped and the jump is
# lost to the drag (the jump puts the cursor at the target, not that far
# past it): it is where the cursor ends short of 1950 run on from the jump,
# and shows whether the server had such moves.
bursts=4
had_moves=0
for ((b = 1; b <= bursts; b++)); do
  orbit_from_iso
  xdotool mousemove $((1500 * S)) "$press_y"
  sleep 0.1
  burst=()
  for ((i = 0; i < 150; i++)); do
    burst+=(mousemove_relative $((3 * S)) 0 sleep 0.001)
  done
  xdotool "${burst[@]}"
  sleep 0.2
  ui_sync
  ui_expect_new "Orbit cursor jumped to 17," "burst $b: the cursor jumps at the shared edge" 8
  [ "$(orbit_since "Orbit cursor jumped")" = 1 ] || ui_fail "burst $b: the cursor jumped more than once"
  if [ "$(orbit_since "Orbit cursor jump not taken")" != 0 ]; then
    tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep "Orbit cursor jump"
    ui_fail "burst $b: a move from the old edge stamped after the move that jumped undid the jump (X11 delivered a move it had before the jump: OrbitWrap::position needs the cursor check, mitcad#2205)"
  fi
  read -r from_x <<< "$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep "Orbit cursor jumped" | tail -1 |
    sed 's/.* from \([0-9-]*\),.*/\1/')"
  # At device pixel ratio 2 the right screen's global positions start at
  # 3200, its logical ones at 1600.
  if [ "$from_x" -ge 3200 ]; then
    from_x=$((from_x - 1600))
  fi
  read -r at _ <<< "$(orbit_cursor)"
  lost=$((17 + 1950 - from_x - at))
  echo "     burst $b: jumped from x $from_x, the server moved the cursor $lost pixels more before the jump"
  if [ "$lost" -gt 0 ]; then
    had_moves=$((had_moves + 1))
  fi
  echo "ok   burst $b: the fast moves kept the jump"
  orbit_release "burst $b"
  orbit_turned $((at - 17 + from_x - press_lx)) "$per_x" "burst $b"
done
if [ "$had_moves" != 0 ]; then
  echo "note: in $had_moves of $bursts bursts the server had moves past the one that jumped before the jump; none undid it"
else
  echo "note: in none of $bursts bursts did the server have moves past the one that jumped before the jump"
fi

ui_finish "an orbit past the edge on two screens at device pixel ratio $UI_SCALE"
