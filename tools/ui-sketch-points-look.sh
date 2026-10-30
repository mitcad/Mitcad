#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# The sketch points' look against the selection (mitcad#946): a one-off
# visual check, not a recurring test (its name is not ui-*-test.sh, so no
# check picks it up). Two designs mitcad-cli makes, each Sketch1 (F1) on XY
# with a fitted spline through (-30, -30), (-10, -20) and (10, -30), a line
# from (-70, -60) to its start, a line from its end to (50, -60) and two
# points at (-60, 50) and (60, 50) that widen the fitted view:
#   free   nothing constrained (the points not fully constrained);
#   fixed  every point fixed, so that everything is fully constrained.
# Each is edited on the white and on the dark background, and pictures of
# the screen are saved as <dir>/<design>-<background>-<case>.png:
#   spline-selected        the spline selected (its fit points on it)
#   spline-point-hovered   the spline selected, its middle fit point hovered
#   line-selected          the first line selected (its end points on it)
#   point-hovered          nothing selected, the middle fit point hovered
#   point-selected         the middle fit point selected
# each with <name>-view.png, the 3D view cut out of that same picture, and
# <name>-detail.png, the curves cut out of it; the designs as
# <dir>/<design>.mitcad and the app's logs as <dir>/<design>-<background>.log.
#
# Runs headless on Xvfb (see ui-test-lib.sh). Works on builds before
# mitcad#946 too (it reads no log line that came with it), for pictures
# before and after.
# Usage: tools/ui-sketch-points-look.sh <dir>

source "$(dirname "$0")/ui-test-lib.sh"

DIR=${1:?usage: tools/ui-sketch-points-look.sh <dir>}
mkdir -p "$DIR"
DIR=$(cd "$DIR" && pwd)
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
RUN=""
trap '[ -n "$RUN" ] && cp -f "$UI_LOG" "$DIR/$RUN.log" 2> /dev/null; ui_cleanup' EXIT

cat > "$DIR/free.json" << 'EOF'
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.spline", "sketch": "F1", "points": [[-30, -30], [-10, -20], [10, -30]]},
  {"cmd": "sketch.add_line", "sketch": "F1", "start": [-70, -60], "end": "p2"},
  {"cmd": "sketch.add_line", "sketch": "F1", "start": "p4", "end": [50, -60]},
  {"cmd": "sketch.add_point", "sketch": "F1", "at": [-60, 50]},
  {"cmd": "sketch.add_point", "sketch": "F1", "at": [60, 50]}
]
EOF
# The points first (p1 to p7), fixed; the curves through them.
cat > "$DIR/fixed.json" << 'EOF'
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_point", "sketch": "F1", "at": [-30, -30], "fixed": true},
  {"cmd": "sketch.add_point", "sketch": "F1", "at": [-10, -20], "fixed": true},
  {"cmd": "sketch.add_point", "sketch": "F1", "at": [10, -30], "fixed": true},
  {"cmd": "sketch.add_point", "sketch": "F1", "at": [-70, -60], "fixed": true},
  {"cmd": "sketch.add_point", "sketch": "F1", "at": [50, -60], "fixed": true},
  {"cmd": "sketch.add_point", "sketch": "F1", "at": [-60, 50], "fixed": true},
  {"cmd": "sketch.add_point", "sketch": "F1", "at": [60, 50], "fixed": true},
  {"cmd": "sketch.spline", "sketch": "F1", "points": ["p1", "p2", "p3"]},
  {"cmd": "sketch.add_line", "sketch": "F1", "start": "p4", "end": "p1"},
  {"cmd": "sketch.add_line", "sketch": "F1", "start": "p3", "end": "p5"}
]
EOF
for design in free fixed; do
  "$CLI" run "$DIR/$design.json" --save "$DIR/$design.mitcad" > "$DIR/$design-cli.log" 2>&1 ||
    { cat "$DIR/$design-cli.log"; ui_fail "mitcad-cli ($design)"; }
done

# away: the mouse off the window (no hover, no tooltip).
away() { xdotool mousemove 5000 5000; }

# shot name [keep]: the whole screen once the view has settled, the mouse
# away (left where it is with "keep", for a hover), and the 3D view and the
# curves cut out of that same picture.
shot() {
  local dump from to a b x0 y0 x1 y1
  [ "${2:-}" = keep ] || away
  ui_wait_idle
  ui_sync
  if [ "${2:-}" = keep ]; then sleep 0.5; else sleep 2; fi
  dump="$DIR/$1.xwd"
  xwd -root -silent > "$dump"
  python3 "$(dirname "$0")/xwd2png.py" "$dump" "$DIR/$1.png"
  read -r -a from <<< "$(ui_view_at 0 0)"
  read -r -a to <<< "$(ui_view_at 100 100)"
  python3 "$(dirname "$0")/xwd2png.py" "$dump" "$DIR/$1-view.png" \
    "${from[0]}" "${from[1]}" $((to[0] - from[0])) $((to[1] - from[1]))
  # The curves from (-75, -65) to (55, -15) mm, with a margin.
  read -r -a a <<< "$(ui_sketch_at -75 -65)"
  read -r -a b <<< "$(ui_sketch_at 55 -15)"
  x0=$((a[0] < b[0] ? a[0] : b[0])); x1=$((a[0] < b[0] ? b[0] : a[0]))
  y0=$((a[1] < b[1] ? a[1] : b[1])); y1=$((a[1] < b[1] ? b[1] : a[1]))
  python3 "$(dirname "$0")/xwd2png.py" "$dump" "$DIR/$1-detail.png" \
    $((x0 - 20)) $((y0 - 20)) $((x1 - x0 + 40)) $((y1 - y0 + 40))
  rm -f "$dump"
  echo "ok   $DIR/$1.png $DIR/$1-view.png $DIR/$1-detail.png"
}

# unselect: no sketch curve or point selected (a click where nothing is,
# off the x axis, which a click selects), the mouse away. The sketch the
# timeline's double-click selected stays: a click in the view does not
# change that selection and logs nothing.
unselect() {
  if grep -F "Selected: " "$UI_LOG" | tail -1 | grep -qE "Selected: [0-9]+ sketch (curve|point)"; then
    ui_mark
    ui_step "click where nothing is"       ui_sketch_click 45 15
    ui_expect_new "Selected: nothing" "the selection cleared"
  fi
  away
  ui_sync
}

# select x y what: a click at a sketch point (mm) selects "what".
select_at() {
  ui_mark
  ui_step "select $3"                      ui_sketch_click "$1" "$2"
  ui_expect_new "Selected: 1 sketch $3" "the $3 selected"
}

# hover x y: the mouse on a sketch point (mm), left there.
hover() {
  ui_step "hover at $1, $2"                xdotool mousemove $(ui_sketch_at "$1" "$2")
  ui_sync
}

ui_start_display
for design in free fixed; do
  for background in light dark; do
    RUN=$design-$background
    echo "--- $design, $background background"
    ui_start_app --open "$DIR/$design.mitcad"
    ui_step "fit (F6)"                     ui_key F6
    ui_expect_log "Timeline Sketch1 at" "the timeline is logged"
    ui_mark
    if [ "$background" = light ]; then
      ui_step "white background (search)"  ui_command "White Background"
    else
      ui_step "dark background (search)"   ui_command "Dark Background"
    fi
    ui_expect_new "Background " "the background $background"
    ui_step "edit Sketch1 (double-click)"  ui_double_click_logged "Timeline Sketch1"
    ui_expect_log "Editing sketch F1 (Sketch1)" "Sketch1 is edited"
    ui_step "fit (F6)"                     ui_key F6
    ui_sync
    grep -F "Sketch colours: " "$UI_LOG" | tail -1
    if [ "$design" = fixed ] && ! grep -F "Sketch colours: " "$UI_LOG" | tail -1 | grep -qF " 0 unconstrained"; then
      echo "note: the fixed design is not fully constrained everywhere"
    fi
    unselect

    # The spline at t = 0.2 of its first half (as tools/ui-sketch-spline-test.sh).
    select_at -22 -24.32 curve
    shot "$RUN-spline-selected"
    hover -10 -20
    shot "$RUN-spline-point-hovered" keep
    unselect

    # The first line, half way along.
    select_at -50 -45 curve
    shot "$RUN-line-selected"
    unselect

    hover -10 -20
    shot "$RUN-point-hovered" keep
    select_at -10 -20 point
    shot "$RUN-point-selected"
    unselect

    cp -f "$UI_LOG" "$DIR/$RUN.log"
    ui_stop_app
  done
done
RUN=""
ui_finish "Sketch points look pictures in $DIR"
