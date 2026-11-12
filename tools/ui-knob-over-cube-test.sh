#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/ManipulatorOverlay.cpp app/framework/ManipulatorOverlay.hpp
# check-all sources: app/OcctViewer.cpp app/OcctViewer.hpp app/framework/CommandSession.cpp tools/cli
# A click on a manipulator's knob drawn over the orientation cube
# (mitcad#1105): a press that starts on a handle stays with it even
# without a drag, so it never reaches the cube (or the view) as a click.
# On a 40 mm cube that mitcad-cli makes (Box1, F1, body F1.b0), seen from
# the home view: Fillet on the top front edge shows its Radius arrow; the
# view is panned (middle drag) until the arrow's grab point lies on the
# place where a click turns the view to the cube's top face, and that
# place is clicked without a drag:
#   - the handle keeps the click ("Manipulator Fillet sets.0.radius
#     clicked without a drag: kept by the handle"), the radius is not
#     dragged, and the logged camera is unchanged (the view did not turn);
#   - with Fillet cancelled (no handle), the same place of the cube turns
#     the view to the top, as before: other clicks are unchanged.
# In the Floating and the Docked layout. The Windows test is
# tools/ui-windows-knob-over-cube-test.ps1.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-knob-over-cube-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-knob-over-cube.XXXXXX)
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
# Where a click hits the cube's faces, logged as the camera rests (and
# again when the cube moves with the panels).
export MITCAD_LOG_ORIENTATION_CUBE=1
EDGE='E{F1:side0|F1:top}'
KNOB='Manipulator Fillet sets.0.radius'

cat > "$WORK/cube.json" << 'EOF'
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, 0],
    "length": 40, "width": 40, "height": 40, "operation": "new_body"}}
]
EOF
"$CLI" run "$WORK/cube.json" --save "$WORK/cube.mitcad" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# place "text": x y (window pixels) of the newest "<text> at x,y" line.
place() {
  grep -F -- "$1 at " "$UI_LOG" | tail -1 | sed -n 's/.* at \([0-9-]*\),\([0-9-]*\).*$/\1 \2/p'
}

# camera: the newest logged camera (direction, up and projection).
camera() { grep -o 'Camera direction [^(]*([a-z]*)' "$UI_LOG" | tail -1; }

# settle: the app idle, its places logged, the camera at rest.
settle() {
  ui_wait_idle
  ui_sync
  sleep 0.8
  ui_sync
}

# pan_by dx dy: a middle drag in the view by (dx, dy) window pixels,
# from the knob's grab point.
pan_by() {
  local fx fy i
  read -r fx fy <<< "$(ui_logged_at "$KNOB")"
  ui_apart "$fx" "$fy"
  xdotool mousemove "$fx" "$fy" mousedown 2
  for i in 1 2 3 4 5 6 7 8; do
    sleep 0.08
    xdotool mousemove $((fx + $1 * UI_SCALE * i / 8)) $((fy + $2 * UI_SCALE * i / 8))
  done
  sleep 0.2
  xdotool mouseup 2
}

# knob_onto_cube: pans until the knob's grab point is within 3 pixels of
# the place where a click turns the view to the cube's top face.
knob_onto_cube() {
  local kx ky cx cy dx dy round
  for round in 1 2 3 4; do
    read -r cx cy <<< "$(place "Orientation cube top")"
    read -r kx ky <<< "$(place "$KNOB")"
    [ -n "${cx:-}" ] && [ -n "${kx:-}" ] || ui_fail "no places of the cube's top face and the knob logged"
    dx=$((cx - kx))
    dy=$((cy - ky))
    if [ $((dx * dx + dy * dy)) -le 9 ]; then
      echo "ok   the knob at $kx,$ky over the cube's top face at $cx,$cy"
      return
    fi
    ui_mark
    ui_step "pan the knob by $dx,$dy (round $round)" pan_by "$dx" "$dy"
    ui_expect_new "$KNOB at " "the knob moved with the view"
    settle
  done
  ui_fail "the knob at $kx,$ky did not come onto the cube's top face at $cx,$cy"
}

# knob_over_cube layout [app arguments]: the steps above in one layout.
knob_over_cube() {
  echo "=== A click on the knob over the cube, $1"
  shift
  ui_start_app --open "$WORK/cube.mitcad" "$@"
  ui_step "the home view"                  ui_click_logged "View home"
  ui_expect_log "Camera direction -0.568 0.654 -0.5 up" "seen from the front, the right and above (the default view)"
  ui_step "fit (F6)"                       ui_key F6
  settle

  echo "--- Fillet's arrow moved onto the cube"
  ui_step "fillet (F)"                     ui_key f
  ui_mark
  ui_step "pick the top front edge"        ui_click_pick "edge F1.b0/$EDGE"
  ui_expect_new "Fillet Edges: 1 edge" "the edge picked"
  ui_expect_new "$KNOB at " "Radius has an arrow"
  ui_expect_new "Preview Fillet: ok" "previewed"
  settle
  knob_onto_cube

  echo "--- The click stays with the knob"
  local before cube after
  before=$(camera)
  cube=$(place "Orientation cube top")
  [ -n "$before" ] || ui_fail "no camera logged"
  ui_mark
  ui_step "click the knob over the cube"   ui_click_logged "Orientation cube top"
  ui_expect_new "$KNOB clicked without a drag: kept by the handle" "the knob kept the click"
  settle
  sleep 1
  ui_sync
  [ "$(place "Orientation cube top")" = "$cube" ] || ui_fail "the cube moved before the click"
  tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -q "$KNOB dragged to" && ui_fail "the click dragged the radius"
  while read -r after; do
    [ "$after" = "$before" ] || ui_fail "the view turned: $before -> $after"
  done < <(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -o 'Camera direction [^(]*([a-z]*)')
  echo "ok   the camera unchanged: $before"
  grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"

  echo "--- Without the handle the cube still turns the view"
  ui_focus_main
  ui_mark
  ui_step "cancel (Esc)"                   ui_key Escape
  ui_expect_new "Command Fillet cancelled" "the fillet cancelled"
  settle
  ui_mark
  ui_step "the cube's top face"            ui_click_logged "Orientation cube top"
  ui_expect_new "Camera direction 0 0 -1 " "the view turned to the top" 8
  ui_stop_app
}

ui_start_display

knob_over_cube "Floating" --chrome=floating
knob_over_cube "Docked" --chrome=docked

ui_finish "UI knob over the orientation cube test"
