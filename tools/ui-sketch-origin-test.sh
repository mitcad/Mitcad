#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/MainWindow.cpp app/MainWindowVisibility.cpp app/browser app/projects tools/cli
# check-all sources: app/OcctViewer.cpp app/OcctViewer.hpp
# The sketch's own origin and axes in a new design, whose Origin folder's
# light bulb is off (mitcad#739), through the real UI:
#   - A new design starts with the design's origin hidden in the view and
#     the Origin folder's light bulb off.
#   - Its first view is framed and logged ("View: new design framed,
#     scale <mm> mm per view height at <w>x<h>", mitcad#1000); the window
#     resized larger, taller than wide and back keeps that scale ("View:
#     resized to ...").
#   - Create Sketch on XY, XZ and YZ: the origin planes shown for the pick
#     only; in the sketch the design's origin is hidden again, while the
#     sketch draws its origin in the view and its x and y axes across it
#     ("Sketch axes drawn:"), where the sketch view puts the sketch's
#     (0, 0) and its x and y directions ("Sketch view").
#   - The Line tool clicked a few pixels beside the origin starts the line
#     at the origin exactly (the snap, as before mitcad#739). The XZ and
#     YZ sketches get a line away from the origin instead, so that they are
#     kept (an empty new sketch is taken back when it is finished).
#   - After each sketch the Origin folder's light bulb is still off and was
#     never switched ("Visibility Root/Origin" is never logged).
#   - Editing an existing sketch (the XZ one) draws its origin and axes
#     too, and its origin is snapped to.
#   - New Design in a project (mitcad#844; the project made with
#     mitcad-cli): the new design opens with its Component1 active, Create
#     Sketch shows the origin planes, XY clicked in the view starts the
#     sketch in Component1, and the light bulb stays off.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sketch-origin-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-sketch-origin.XXXXXX)
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT
# The Projects panel lists the test's own projects, not the user's.
export MITCAD_PROJECTS_DIR=$WORK/projects

HIDDEN="Design origin hidden in the view; the Origin folder's light bulb off"
SHOWN="Design origin shown in the view; the Origin folder's light bulb off"

# expect_origin_off description: within six seconds the last "Design
# origin" line says the design's origin is hidden with the light bulb off,
# and the Origin folder's light bulb was never switched.
expect_origin_off() {
  local last
  for _ in $(seq 1 30); do
    last=$(grep -o "Design origin .*" "$UI_LOG" | tail -1)
    [ "$last" = "$HIDDEN" ] && break
    sleep 0.2
  done
  [ "$last" = "$HIDDEN" ] || ui_fail "$1: the last origin line is '$last', expected '$HIDDEN'"
  ! grep -q "Visibility Root/Origin" "$UI_LOG" ||
    ui_fail "$1: the Origin folder's light bulb was switched: $(grep "Visibility Root/Origin" "$UI_LOG" | tail -1)"
  echo "ok   $1: the design's origin hidden, the Origin folder's light bulb off"
}

# expect_axes description: within six seconds the last "Sketch axes drawn:"
# line since the mark has the sketch's origin in the view where the last
# "Sketch view" puts the sketch's (0, 0) (within two pixels), and an x and
# a y axis drawn across the view through it, each along the view's
# direction of the sketch's x or y.
expect_axes() {
  local axes view
  for _ in $(seq 1 30); do
    ui_sync
    axes=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -o "Sketch axes drawn: .*" | tail -1)
    view=$(grep -o "Sketch view [-0-9.,]* [-0-9.,]* [-0-9.,]*" "$UI_LOG" | tail -1)
    if [ -n "$axes" ] && [ -n "$view" ] && awk -v axes="$axes" -v view="$view" '
      function numbers(text, into, pattern,   n) {
        n = 0
        while (match(text, pattern)) {
          into[++n] = substr(text, RSTART, RLENGTH) + 0
          text = substr(text, RSTART + RLENGTH)
        }
        return n
      }
      # along(ax, ay, bx, by, dx, dy): the line a-b runs in the direction
      # d and passes within two pixels of the origin.
      function along(ax, ay, bx, by, dx, dy,   lx, ly, span, d) {
        lx = bx - ax; ly = by - ay
        span = sqrt(lx ^ 2 + ly ^ 2); d = sqrt(dx ^ 2 + dy ^ 2)
        if (span < 50 || d < 1 || (lx * dx + ly * dy) / (span * d) < 0.999) return 0
        return (((o[1] - ax) * ly - (o[2] - ay) * lx) / span) ^ 2 <= 4
      }
      BEGIN {
        split("", o); split("", v)
        if (axes !~ / in the view;/ || axes ~ /none/) exit 1
        if (numbers(axes, o, "-?[0-9]+") != 10 || numbers(view, v, "-?[0-9.]+") != 6) exit 1
        if ((o[1] - v[1]) ^ 2 + (o[2] - v[2]) ^ 2 > 4) exit 1
        if (!along(o[3], o[4], o[5], o[6], v[3] - v[1], v[4] - v[2])) exit 1
        if (!along(o[7], o[8], o[9], o[10], v[5] - v[1], v[6] - v[2])) exit 1
      }'; then
      echo "ok   $1: ${axes#Sketch axes drawn: }"
      return
    fi
    sleep 0.2
  done
  ui_fail "$1: the sketch's origin and axes are not drawn where the view puts them: '$axes' ($view)"
}

# line_from_origin x y description: the Line tool from a click four pixels
# right of and three above the sketch's origin to the sketch point (x, y):
# the line starts at the origin exactly.
line_from_origin() {
  local ox oy
  ui_mark
  ui_step "line tool (L)"                  ui_key l
  ui_expect_new "Sketch tool Line" "line tool active"
  read -r ox oy <<< "$(ui_sketch_at 0 0)"
  ui_step "click beside the origin"        xdotool mousemove $((ox + 4)) $((oy - 3)) click 1
  ui_step "to ($1, $2)"                    ui_sketch_click "$1" "$2"
  ui_expect_new "from (0, 0) to" "$3: the line starts at the origin (snapped)"
  ui_step "end the tool (Esc)"             ui_key Escape
}

# free_line description: a line from the sketch point (20, 10) to (40, 30),
# away from the origin and the axes, so that the sketch is kept when it is
# finished (an empty new sketch is taken back, MainWindow::finishSketch) and
# its origin stays unmade for a later snap.
free_line() {
  ui_mark
  ui_step "line tool (L)"                  ui_key l
  ui_expect_new "Sketch tool Line" "line tool active"
  ui_step "from (20, 10)"                  ui_sketch_click 20 10
  ui_step "to (40, 30)"                    ui_sketch_click 40 30
  ui_expect_new "Added line " "$1: a line away from the origin"
  ui_step "end the tool (Esc)"             ui_key Escape
}

ui_start_display
ui_start_app

echo "--- A new design: the origin hidden, the Origin folder's light bulb off"
ui_expect_log "$HIDDEN" "a new design starts with its origin hidden"

echo "--- A new design's framing kept when the window is resized (mitcad#1000)"
ui_expect_log "View: new design framed, scale " "a new design's first view is logged"
FRAMED=$(grep -o "View: new design framed, scale [0-9.]* mm" "$UI_LOG" | tail -1 | grep -o "[0-9][0-9.]*")
[ -n "$FRAMED" ] || ui_fail "no scale in the new design's framing line"
# expect_framing description: within six seconds a "View: resized to" line
# since the mark, with the new design's scale per view height (within the
# logged 0.001 mm).
expect_framing() {
  local line scale
  for _ in $(seq 1 30); do
    ui_sync
    line=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -o "View: resized to .*" | tail -1)
    [ -n "$line" ] && break
    sleep 0.2
  done
  [ -n "$line" ] || ui_fail "$1: no 'View: resized to' line since the mark"
  scale=$(grep -o "scale [0-9.]* mm per view height" <<< "$line" | grep -o "[0-9][0-9.]*")
  awk -v a="$scale" -v b="$FRAMED" 'BEGIN { exit !(a != "" && (a - b) ^ 2 <= 1e-6) }' ||
    ui_fail "$1: '$line', expected the new design's scale $FRAMED mm per view height"
  echo "ok   $1: $line"
}
# Larger, taller than wide (OCCT alone would keep the width then), and
# back; Xvfb has no window manager to maximize the window, which the
# Windows test does.
ui_mark
ui_step "a larger window"                  xdotool windowsize "$UI_WINDOW" $((WIDTH + 300 * UI_SCALE)) $((HEIGHT + 150 * UI_SCALE))
expect_framing "a larger window"
ui_mark
ui_step "a window taller than wide"        xdotool windowsize "$UI_WINDOW" $((760 * UI_SCALE)) $((960 * UI_SCALE))
expect_framing "a window taller than wide"
ui_mark
ui_step "the window's size again"          xdotool windowsize "$UI_WINDOW" "$WIDTH" "$HEIGHT"
expect_framing "the window's size again"

for plane in xy xz yz; do
  echo "--- Create Sketch on ${plane^^}: the sketch's origin and axes drawn (mitcad#739)"
  ui_mark
  ui_step "create sketch on ${plane^^}"    ui_create_sketch "$plane"
  ui_expect_new "$SHOWN" "${plane^^}: the origin planes shown for the pick"
  ui_expect_new "Sketch view" "${plane^^}: the sketch view is logged"
  expect_axes "${plane^^}"
  expect_origin_off "${plane^^} in the sketch"
  if [ "$plane" = xy ]; then
    line_from_origin 30 20 "XY"
  else
    free_line "${plane^^}"
  fi
  ui_mark
  ui_step "finish sketch (Ctrl+Enter)"     ui_key ctrl+Return
  ui_expect_new "Sketch finished" "${plane^^}: the sketch finished"
  ui_wait_idle
  ui_sync
  expect_origin_off "${plane^^} after the sketch"
done

echo "--- Editing an existing sketch draws its origin and axes too (mitcad#739)"
ui_expect_log "Timeline Sketch2 at" "the timeline is laid out"
ui_double_click_expect "Timeline Sketch2" "Editing sketch F2 (Sketch2)" "the XZ sketch is edited again"
ui_wait_idle
expect_axes "the edited XZ sketch"
expect_origin_off "the edited XZ sketch"
line_from_origin -30 20 "the edited XZ sketch"
ui_mark
ui_step "finish sketch (Ctrl+Enter)"     ui_key ctrl+Return
ui_expect_new "Sketch finished" "the edited sketch finished"
ui_wait_idle
ui_sync
expect_origin_off "after the edit"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
ui_stop_app

echo "--- New Design in a project: Component1 active, the origin planes for the pick (mitcad#844)"
PROJECT=$MITCAD_PROJECTS_DIR/Shop
mkdir -p "$MITCAD_PROJECTS_DIR"
"$CLI" project create "$PROJECT" --author "Ada Tester <ada@example.invalid>" --design base.mitcad \
  > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli project create"; }
ui_start_app --open "$PROJECT/base.mitcad"
ui_expect_log "base.mitcad" "a design of the project opened"
ui_mark
ui_step "new design (Ctrl+N)"            ui_key ctrl+n
ui_expect_new "New Design: in " "a design of the current project, not an untitled one"
ui_expect_new "Projects panel: naming a new design in " "named in the Projects panel" 20
ui_step "take its name (Enter)"          ui_key Return
ui_expect_new "New Design: made " "made as a version" 20
ui_expect_new "Opened the new design with Component1 active" "the new design opens with Component1 active" 20
[ -f "$PROJECT/Design1.mitcad" ] || ui_fail "the new design is not in the project"
ui_focus_main
expect_origin_off "the new design"
ui_mark
ui_step "create sketch on XY"            ui_create_sketch xy
ui_expect_new "Create Sketch: origin planes shown" "the origin planes shown for the pick"
ui_expect_new "$SHOWN" "the design's origin shown for the pick, its light bulb off"
ui_expect_new "Sketch started on xy" "XY clicked in the view starts the sketch"
ui_expect_new "Sketch Sketch1 is in Component1" "the sketch is Component1's"
expect_origin_off "the new design's sketch"
ui_mark
ui_step "finish sketch (Ctrl+Enter)"     ui_key ctrl+Return
ui_expect_new "Sketch finished" "the new design's sketch finished"
ui_wait_idle
ui_sync
expect_origin_off "after the new design's sketch"

ui_finish "UI sketch origin test"
