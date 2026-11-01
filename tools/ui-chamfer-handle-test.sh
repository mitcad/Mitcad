#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands app/framework/ManipulatorOverlay.cpp app/framework/CommandSession.cpp app/framework/RememberedValues.cpp
# check-all sources: tools/cli
# Chamfer's distance arrows in the view (mitcad#534), through the real UI
# on the demo block (60 x 40 x 20, Body1, F2.b0):
#   - In the Floating and the Docked layout, Equal Distance: the arrow
#     stands at the top front edge's middle, also after a zoom; dragged
#     out, the panel's Distance and the preview follow, and OK bevels the
#     edge at the dragged distance. Edited from the timeline: the handle
#     where it stood, at the saved distance; a drag cancelled leaves the
#     chamfer as it was; a drag accepted changes it, and Undo restores it.
#   - Two sets: the second set's arrow changes the second set only.
#   - Two Distances: an arrow along each face, which Flip swaps; Distance
#     2 dragged. Distance and Angle: no arrow for the angle.
#   - A component turned and moved: the arrow on its edge as the view
#     shows it, and its drag bevels the edge.
#   - Remembered sizes (mitcad#1028, MITCAD_REMEMBER_PANEL_VALUES=1 for
#     these steps only): Two Distances with 3 mm and 2 mm typed, OK;
#     Chamfer again starts with Distance 3 mm and Distance 2 2 mm.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-chamfer-handle-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-chamfer-handle.XXXXXX)
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
R='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'
TOP="F2:end($R)"
BOTTOM="F2:start($R)"
FRONT='F2:side(c1[c4,c2])'
RIGHT='F2:side(c2[c1,c3])'
TOP_FRONT="E{$TOP|$FRONT}"
TOP_RIGHT="E{$TOP|$RIGHT}"
RIGHT_BOTTOM="E{$RIGHT|$BOTTOM}"

# Part:1 (O1): the block of the demo in a component, turned a quarter
# clockwise about Z and moved 100 mm along X, so that its right face
# faces the front.
cat > "$WORK/placed.json" << EOF
[
  {"cmd": "create_component", "name": "Part",
   "transform": {"translation": [100, 0, 0], "rotation": {"axis": [0, 0, 1], "angle": -1.5707963267948966}}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 60, "height": 40},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$R"}],
    "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}}
]
EOF
"$CLI" run "$WORK/placed.json" --save "$WORK/placed.mitcad" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# newest "text": x y of the newest "<text> at x,y" line (window pixels).
newest() {
  grep -F -- "$1 at " "$UI_LOG" | tail -1 | sed -n 's/.* at \([0-9-]*\),\([0-9-]*\)$/\1 \2/p'
}

# aligned edge key description: the handle of the input `key` is grabbed
# within 45 pixels of the edge's middle, where the edge's pick place is
# (the knob is a millimetre or so off the edge, the grab point 17 pixels
# beyond it): the arrow stands on the edge as the view shows it.
aligned() {
  local ex ey hx hy
  read -r ex ey <<< "$(newest "Pick edge F2.b0/$1")"
  read -r hx hy <<< "$(newest "Manipulator Chamfer $2")"
  [ -n "${ex:-}" ] && [ -n "${hx:-}" ] || ui_fail "$3: no places logged"
  awk -v a="$ex" -v b="$ey" -v c="$hx" -v d="$hy" 'BEGIN { exit !((a - c) ^ 2 + (b - d) ^ 2 < 45 ^ 2) }' ||
    ui_fail "$3: the handle at $hx,$hy, the edge's middle at $ex,$ey"
  echo "ok   $3 ($hx,$hy by $ex,$ey)"
}

# drag_out edge key percent: drags the handle along its arrow by `percent`
# of the way from the edge's middle to the handle's grab point (negative:
# back towards the edge).
drag_out() {
  local ex ey hx hy
  read -r ex ey <<< "$(newest "Pick edge F2.b0/$1")"
  read -r hx hy <<< "$(newest "Manipulator Chamfer $2")"
  [ -n "${ex:-}" ] && [ -n "${hx:-}" ] || ui_fail "no places logged to drag $2 by"
  ui_drag_from "Manipulator Chamfer $2" $(((hx - ex) * $3 / 100)) $(((hy - ey) * $3 / 100))
}

# drag_mm edge key value millimetres: drags the handle along its arrow so
# that its value, now `value`, changes by about that many millimetres
# (negative: back towards the edge). The grab point is 17 pixels beyond
# the knob, which stands value * (pixels per mm) from the edge's middle, so
# the way from the edge's middle to the grab point gives the pixels per
# mm. The drags stay well inside the block: a bevel deeper than its 20 mm
# height fails the preview ("the distance may be too large").
drag_mm() {
  local ex ey hx hy step
  read -r ex ey <<< "$(newest "Pick edge F2.b0/$1")"
  read -r hx hy <<< "$(newest "Manipulator Chamfer $2")"
  [ -n "${ex:-}" ] && [ -n "${hx:-}" ] || ui_fail "no places logged to drag $2 by"
  step=$(awk -v a="$ex" -v b="$ey" -v c="$hx" -v d="$hy" -v v="$3" -v m="$4" 'BEGIN {
    x = c - a; y = d - b; l = sqrt(x * x + y * y); r = (l - 17) / v
    if (l < 1 || r <= 0) { exit 1 }
    p = m * r / l; printf "%d %d", (x * p < 0 ? x * p - 0.5 : x * p + 0.5), (y * p < 0 ? y * p - 0.5 : y * p + 0.5) }') ||
    ui_fail "$2: no pixels per mm from the edge at $ex,$ey and the handle at $hx,$hy"
  ui_drag_from "Manipulator Chamfer $2" $step
}

# dragged key: where the newest drag of the input's handle ended (mm).
dragged() { grep -F "Manipulator Chamfer $1 dragged to " "$UI_LOG" | tail -1 | sed 's/.* dragged to //'; }

# between value low high description: low < value < high.
between() {
  awk -v v="$1" -v l="$2" -v h="$3" 'BEGIN { exit !(v > l && v < h) }' ||
    ui_fail "$4: $1 is not between $2 and $3"
  echo "ok   $4: $1"
}

# at_place key x y description: the input's handle is (within 3 pixels) at
# x, y, waiting a moment for it to get there.
at_place() {
  local hx hy
  for _ in $(seq 1 25); do
    read -r hx hy <<< "$(newest "Manipulator Chamfer $1")"
    if [ -n "${hx:-}" ] && awk -v a="$hx" -v b="$hy" -v c="$2" -v d="$3" \
      'BEGIN { exit !((a - c) ^ 2 + (b - d) ^ 2 <= 9) }'; then
      echo "ok   $4 ($hx,$hy)"
      return
    fi
    sleep 0.2
  done
  ui_fail "$4: the handle at ${hx:-?},${hy:-?}, not at $2,$3"
}

# volume_now expected description: the block's volume in the newest line
# about it logged since the mark comes to the expected one (a roll-back
# undone before an edit may log a volume on the way).
volume_now() {
  local volume=""
  for _ in $(seq 1 150); do
    volume=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" |
      sed -n 's/.*(F2\.b0): volume [0-9.]* -> \([0-9.]*\) mm3.*/\1/p' | tail -1)
    if [ -n "$volume" ] &&
      awk -v v="$volume" -v e="$1" 'BEGIN { exit !((v - e) ^ 2 < (1e-6 * e) ^ 2 + 1e-4) }'; then
      echo "ok   $2: $volume mm3"
      return
    fi
    sleep 0.2
  done
  ui_fail "$2: volume ${volume:-not logged} mm3, expected $1"
}

# bevelled k d [d2]: the block's volume with a chamfer of d (and d2 on the
# other face) along k / 2 mm of square edge: 60 mm for k = 30.
bevelled() { awk -v k="$1" -v a="$2" -v b="${3:-$2}" 'BEGIN { printf "%.6f", 48000 - k * a * b }'; }

# equal_distance_round layout [app arguments]: the Equal Distance arrow,
# made and edited from the timeline, in one layout.
equal_distance_round() {
  echo "=== Equal Distance, $1"
  shift
  ui_start_app --demo "$@"
  ui_step "fit (F6)"                       ui_key F6
  ui_step "chamfer (search)"               ui_command "Chamfer"
  ui_mark
  ui_step "pick the top front edge"        ui_click_pick "edge F2.b0/$TOP_FRONT"
  ui_expect_new "Chamfer Edges: 1 edge" "the edge picked"
  ui_expect_new "Manipulator Chamfer sets.0.distance at" "Distance has an arrow"
  ui_sync
  aligned "$TOP_FRONT" sets.0.distance "the arrow stands on the edge"
  grep -q "Manipulator Chamfer sets.0.distance2 at" "$UI_LOG" && ui_fail "Distance 2 has an arrow while hidden"

  echo "--- The camera changes"
  ui_mark
  ui_step "zoom out a notch"               xdotool mousemove $(ui_view_at 50 50) click 5
  ui_sync
  ui_expect_new "Manipulator Chamfer sets.0.distance at" "the arrow followed the camera"
  ui_step "the Edges input again"          ui_click_logged "Panel Chamfer input sets.0.edges"
  ui_sync
  aligned "$TOP_FRONT" sets.0.distance "the arrow still stands on the edge"

  echo "--- Dragged out"
  ui_mark
  ui_step "drag the arrow out"             drag_out "$TOP_FRONT" sets.0.distance 50
  ui_expect_new "Manipulator Chamfer sets.0.distance dragged to" "the arrow dragged"
  local d1 d3 v1 v3 x1 y1
  d1=$(dragged sets.0.distance)
  between "$d1" 1 15 "the distance grew"
  ui_expect_new "Chamfer Distance: $d1 mm = " "the panel's Distance followed"
  ui_expect_new "Preview Chamfer: ok" "the preview followed"
  ui_wait_idle
  ui_sync
  read -r x1 y1 <<< "$(newest "Manipulator Chamfer sets.0.distance")"
  ui_mark
  ui_step "OK (Enter)"                     ui_key Return
  ui_expect_new "Added chamfer (Equal Distance) on 1 edge(s)" "chamfer added"
  v1=$(bevelled 30 "$d1")
  volume_now "$v1" "bevelled at the dragged distance"

  echo "--- Edited from the timeline, cancelled"
  ui_double_click_expect "Timeline Chamfer1" " with Chamfer" "editing Chamfer1"
  ui_expect_new "Chamfer Distance: $d1 mm" "the saved distance in the panel"
  ui_expect_new "Manipulator Chamfer sets.0.distance at" "the arrow shown"
  at_place sets.0.distance "$x1" "$y1" "the handle at the saved distance, where it was dragged to"
  ui_mark
  ui_step "drag the arrow 2 mm further out" drag_mm "$TOP_FRONT" sets.0.distance "$d1" 2
  ui_expect_new "Manipulator Chamfer sets.0.distance dragged to" "the arrow dragged"
  ui_expect_new "Preview Chamfer: ok" "previewed"
  ui_step "cancel (Esc)"                   ui_key Escape
  ui_expect_new "Command Chamfer cancelled" "the edit cancelled"
  tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -q "Edited F" && ui_fail "the cancelled edit changed the chamfer"
  ui_double_click_expect "Timeline Chamfer1" " with Chamfer" "editing Chamfer1 again"
  ui_expect_new "Chamfer Distance: $d1 mm" "Chamfer1 kept its distance"
  at_place sets.0.distance "$x1" "$y1" "the handle at the kept distance"

  echo "--- Edited from the timeline, accepted and undone"
  ui_wait_idle
  ui_sync
  ui_mark
  ui_step "drag the arrow halfway back in" drag_mm "$TOP_FRONT" sets.0.distance "$d1" \
    "$(awk -v d="$d1" 'BEGIN { print -d / 2 }')"
  ui_expect_new "Manipulator Chamfer sets.0.distance dragged to" "the arrow dragged"
  d3=$(dragged sets.0.distance)
  between "$d3" 0 "$d1" "the distance shrank"
  ui_expect_new "Chamfer Distance: $d3 mm = " "the panel's Distance followed"
  ui_expect_new "Preview Chamfer: ok" "previewed"
  ui_mark
  ui_step "OK (Enter)"                     ui_key Return
  ui_expect_new "Edited F" "Chamfer1 edited"
  v3=$(bevelled 30 "$d3")
  volume_now "$v3" "bevelled at the new distance"
  ui_wait_idle
  ui_mark
  ui_step "undo (Ctrl+Z)"                  ui_key ctrl+z
  ui_expect_new "Undo: Edit Chamfer1" "the edit taken back"
  volume_now "$v1" "the distance dragged first again"
  grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
  ui_stop_app
}

ui_start_display

equal_distance_round "Floating" --chrome=floating
equal_distance_round "Docked" --chrome=docked

echo "=== Two sets, each its own arrow"
ui_start_app --demo
ui_step "fit (F6)"                         ui_key F6
ui_step "chamfer (search)"                 ui_command "Chamfer"
ui_step "pick the top front edge"          ui_click_pick "edge F2.b0/$TOP_FRONT"
ui_expect_log "Manipulator Chamfer sets.0.distance at" "the first set's arrow"
ui_step "add a set"                        ui_click_logged "Panel Chamfer add sets"
ui_expect_log "Chamfer: added sets row 2" "a second set"
ui_mark
ui_step "pick the right bottom edge"       ui_click_pick "edge F2.b0/$RIGHT_BOTTOM"
ui_expect_new "Manipulator Chamfer sets.1.distance at" "the second set's arrow"
ui_sync
aligned "$RIGHT_BOTTOM" sets.1.distance "the second set's arrow stands on its edge"
ui_mark
ui_step "drag the second set's arrow"      drag_out "$RIGHT_BOTTOM" sets.1.distance 50
ui_expect_new "Manipulator Chamfer sets.1.distance dragged to" "the second arrow dragged"
d2=$(dragged sets.1.distance)
between "$d2" 1 15 "the second set's distance grew"
ui_expect_new "Preview Chamfer: ok" "previewed"
ui_mark
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Added chamfer (Equal Distance) on 2 edge(s) in 2 sets" "chamfer added"
# The top front edge (60 mm) at 1 mm, the right bottom one (40 mm) at d2.
volume_now "$(awk -v d="$d2" 'BEGIN { printf "%.6f", 48000 - 30 - 20 * d * d }')" "each set at its own distance"
ui_mark
ui_step "undo (Ctrl+Z)"                    ui_key ctrl+z
ui_expect_new "Undo: Add Chamfer1" "the chamfer taken back"

echo "=== Two Distances, Distance and Angle"
ui_step "chamfer (search)"                 ui_command "Chamfer"
ui_step "pick the top front edge"          ui_click_pick "edge F2.b0/$TOP_FRONT"
ui_mark
ui_step "Two Distances"                    ui_choose "Panel Chamfer input sets.0.type" 1
ui_expect_new "Manipulator Chamfer sets.0.distance2 at" "Distance 2 has an arrow"
ui_sync
aligned "$TOP_FRONT" sets.0.distance "Distance's arrow stands on the edge"
aligned "$TOP_FRONT" sets.0.distance2 "Distance 2's arrow stands on the edge"
read -r ax ay <<< "$(newest "Manipulator Chamfer sets.0.distance")"
read -r bx by <<< "$(newest "Manipulator Chamfer sets.0.distance2")"
awk -v a="$ax" -v b="$ay" -v c="$bx" -v d="$by" 'BEGIN { exit !((a - c) ^ 2 + (b - d) ^ 2 > 100) }' ||
  ui_fail "both arrows at $ax,$ay: not one along each face"
echo "ok   an arrow along each face"
ui_mark
ui_step "Flip"                             ui_click_logged "Panel Chamfer input sets.0.flip"
ui_expect_new "Manipulator Chamfer sets.0.distance at" "the arrows moved"
at_place sets.0.distance "$bx" "$by" "Distance measured on the other face: where Distance 2's arrow was"
at_place sets.0.distance2 "$ax" "$ay" "Distance 2's arrow where Distance's was"
ui_expect_new "Preview Chamfer: ok" "previewed"
ui_wait_idle
ui_sync
ui_mark
ui_step "drag Distance 2's arrow"          drag_out "$TOP_FRONT" sets.0.distance2 50
ui_expect_new "Manipulator Chamfer sets.0.distance2 dragged to" "Distance 2's arrow dragged"
e2=$(dragged sets.0.distance2)
between "$e2" 1 15 "Distance 2 grew"
ui_expect_new "Chamfer Distance 2: $e2 mm = " "the panel's Distance 2 followed"
ui_expect_new "Preview Chamfer: ok" "previewed"
ui_mark
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Added chamfer (Two Distances) on 1 edge(s)" "chamfer added"
volume_now "$(bevelled 30 1 "$e2")" "1 mm on one face, Distance 2 on the other"
ui_double_click_expect "Timeline Chamfer1" " with Chamfer" "editing Chamfer1"
ui_expect_new "Chamfer Distance 2: $e2 mm" "the saved Distance 2"
ui_expect_new "Manipulator Chamfer sets.0.distance2 at" "Distance 2's arrow shown"
ui_mark
ui_step "Distance and Angle"               ui_choose "Panel Chamfer input sets.0.type" 1
ui_expect_new "Panel Chamfer input sets.0.angle at" "the angle shown"
ui_expect_new "Preview Chamfer: ok" "previewed"
grep -q "Manipulator Chamfer sets.0.angle" "$UI_LOG" && ui_fail "the angle has a handle"
echo "ok   the angle has no handle"
ui_step "cancel (Esc)"                     ui_key Escape
ui_expect_new "Command Chamfer cancelled" "the edit cancelled"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
ui_stop_app

echo "=== A placed component"
ui_start_app --open "$WORK/placed.mitcad"
ui_step "fit (F6)"                         ui_key F6
ui_mark
ui_step "activate Part (radio)"            ui_click_logged "Browser radio Root/Part:1"
ui_expect_new "Activated Part" "Part is active"
ui_step "chamfer (search)"                 ui_command "Chamfer"
ui_mark
ui_step "pick the top right edge"          ui_click_pick "edge F2.b0/$TOP_RIGHT"
ui_expect_new "Chamfer Edges: 1 edge" "the edge picked"
ui_expect_new "Manipulator Chamfer sets.0.distance at" "Distance has an arrow"
ui_sync
aligned "$TOP_RIGHT" sets.0.distance "the arrow stands on the edge where the component shows it"
ui_mark
ui_step "drag the arrow out"               drag_out "$TOP_RIGHT" sets.0.distance 50
ui_expect_new "Manipulator Chamfer sets.0.distance dragged to" "the arrow dragged"
d4=$(dragged sets.0.distance)
between "$d4" 1 15 "the distance grew"
ui_expect_new "Preview Chamfer: ok" "previewed"
ui_mark
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Added chamfer (Equal Distance) on 1 edge(s)" "chamfer added"
# The top right edge is 40 mm long.
volume_now "$(bevelled 20 "$d4")" "bevelled at the dragged distance"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
ui_stop_app

echo "=== Chamfer's remembered sizes (mitcad#1028)"
# The test library starts panels from their defaults; here they remember.
# No chamfer was made with them on before, so the first one starts at 1 mm.
export MITCAD_REMEMBER_PANEL_VALUES=1
ui_start_app --demo
ui_step "fit (F6)"                         ui_key F6
ui_mark
ui_step "chamfer (search)"                 ui_command "Chamfer"
ui_expect_new "Chamfer Distance: 1 mm = 1 mm" "nothing remembered yet: the default 1 mm"
grep -q "Panel Chamfer distance: remembered" "$UI_LOG" && ui_fail "a distance remembered before any OK"
ui_mark
ui_step "pick the top front edge"          ui_click_pick "edge F2.b0/$TOP_FRONT"
ui_expect_new "Chamfer Edges: 1 edge" "the edge picked"
ui_step "Two Distances"                    ui_choose "Panel Chamfer input sets.0.type" 1
ui_expect_new "Panel Chamfer input sets.0.distance2 at" "Distance 2 shown"
ui_step "distance 3"                       ui_type_in "Panel Chamfer input sets.0.distance" "3"
ui_expect_new "Chamfer Distance: 3 = 3 mm" "3 mm typed"
ui_step "distance 2 2"                     ui_type_in "Panel Chamfer input sets.0.distance2" "2"
ui_expect_new "Chamfer Distance 2: 2 = 2 mm" "2 mm typed"
ui_expect_new "Preview Chamfer: ok" "previewed"
ui_wait_idle
ui_mark
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Added chamfer (Two Distances) on 1 edge(s)" "chamfer added"
volume_now "$(bevelled 30 3 2)" "bevelled 3 mm on one face, 2 mm on the other"
ui_wait_idle

echo "--- Chamfer again starts with 3 mm and Distance 2 with 2 mm"
ui_mark
ui_step "chamfer (search)"                 ui_command "Chamfer"
ui_expect_new "Panel Chamfer distance: remembered 3 mm" "the distance of the last OK remembered"
ui_expect_new "Panel Chamfer distance2: remembered 2 mm" "Distance 2 of the last OK remembered"
ui_expect_new "Chamfer Distance: 3 mm = 3 mm" "the panel's Distance is 3 mm"
grep -q "Panel Chamfer angle: remembered" "$UI_LOG" && ui_fail "an angle remembered that no OK gave"
echo "ok   no angle remembered"
ui_focus_main
ui_mark
ui_step "cancel (Esc)"                     ui_key Escape
ui_expect_new "Command Chamfer cancelled" "the chamfer cancelled"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
export MITCAD_REMEMBER_PANEL_VALUES=0
ui_finish "UI chamfer handle test"
