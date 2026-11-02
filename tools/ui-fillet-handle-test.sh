#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/ChamferHandles.cpp app/commands/ChamferHandles.hpp app/commands/FilletCommands.cpp
# check-all sources: app/framework/ManipulatorOverlay.cpp app/framework/CommandSession.cpp app/framework/RememberedValues.cpp
# check-all sources: tools/cli
# Fillet's and Chamfer's size arrows on an inside (concave) corner
# (mitcad#571), through the real UI, on an L bracket: a 60 x 40 x 5 mm
# floor (Box1, F1) and a 60 x 5 x 40 mm wall at its back (Box2, F2,
# joined; body F1.b0, 22500 mm3), seen from its Home view (from the
# front, a little from the right and above), where the inside corner
# between the floor's top and the wall's front runs across the view. On
# that edge each arrow points into the open corner or out along its face,
# away from the body; pulled that way its value grows, pushed back it
# shrinks, and the bracket gains the corner's material at the dragged
# size (60 mm long: a fillet r^2 (1 - pi/4) per mm, a chamfer d1 d2 / 2):
#   - In the Floating and the Docked layout: Fillet's constant Radius and
#     Chamfer's Equal Distance, made and edited from the timeline (pulled
#     out, pushed back, Undo).
#   - Two Distances with the floor as the Reference Face: Distance along
#     the floor, Distance 2 up the wall, both pulled out; Flip swaps the
#     arrows, and the one now up the wall pushed back; edited from the
#     timeline.
#   - Distance and Angle with the floor as the Reference Face: Distance
#     along the floor, with Flip up the wall; no handle for the angle.
#   - An outside (convex) edge keeps its arrow into the material: Fillet
#     on the wall's top front edge, pulled, rounds it off.
#   - Remembered Radius (mitcad#1028, MITCAD_REMEMBER_PANEL_VALUES=1 for
#     these steps only): a fillet with 3 mm typed, OK; Fillet again starts
#     with 3 mm; 5 mm typed and cancelled, and Fillet1 edited to 4 mm,
#     change nothing: a new start of the app still starts with 3 mm.
# Where an arrow points is checked against the camera: the side of the
# edge's line on the screen that the expected direction is on.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-fillet-handle-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-fillet-handle.XXXXXX)
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
INSIDE='E{F1:top|F2:side0}'
WALL_TOP='E{F2:side0|F2:top}'
FLOOR_FRONT='E{F1:side0|F1:top}'
# Knob millimetres per millimetre of the value at a right-angled edge:
# a fillet's middle (sqrt 2 - 1), an equal bevel's middle (sin 45 deg),
# a distance along a face (1).
ROUND=0.414214
BEVEL=0.707107
ALONG=1

# The Home view looks along (-0.5, 2, -1): the inside corner's open side
# (0, -1, 1) and both faces' ways (0, -1, 0) and (0, 0, 1) stand off the
# edge's line on the screen. (Seen isometrically, the open side would lie
# along the edge's line.)
cat > "$WORK/bracket.json" << EOF
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, 0],
    "length": 60, "width": 40, "height": 5, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, 35],
    "length": 60, "width": 5, "height": 40, "operation": "join", "participants": ["F1.b0"]}},
  {"cmd": "add_named_view", "name": "Home", "eye": [80, -180, 115], "target": [30, 20, 15], "up": [0, 0, 1],
    "height": 70}
]
EOF
"$CLI" run "$WORK/bracket.json" --save "$WORK/bracket.mitcad" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# newest "text": x y of the newest "<text> at x,y" line (window pixels).
newest() {
  grep -F -- "$1 at " "$UI_LOG" | tail -1 | sed -n 's/.* at \([0-9-]*\),\([0-9-]*\)$/\1 \2/p'
}

# screen dx dy dz: the way the view shows the direction (dx, dy, dz), from
# the newest camera line: "x y length", x to the right and y down on the
# screen (unit), and how long a millimetre along it shows next to one
# square to the view.
screen() {
  local camera
  camera=$(grep -o 'Camera direction [-0-9.]* [-0-9.]* [-0-9.]* up [-0-9.]* [-0-9.]* [-0-9.]*' "$UI_LOG" | tail -1)
  [ -n "$camera" ] || ui_fail "no camera logged"
  echo "$camera" | awk -v x="$1" -v y="$2" -v z="$3" '{
    a = $3; b = $4; c = $5; p = $7; q = $8; s = $9
    n = sqrt(a * a + b * b + c * c); a /= n; b /= n; c /= n
    k = p * a + q * b + s * c; p -= k * a; q -= k * b; s -= k * c
    n = sqrt(p * p + q * q + s * s); p /= n; q /= n; s /= n
    rx = b * s - c * q; ry = c * p - a * s; rz = a * q - b * p
    n = sqrt(x * x + y * y + z * z); x /= n; y /= n; z /= n
    u = x * rx + y * ry + z * rz; v = -(x * p + y * q + z * s); l = sqrt(u * u + v * v)
    if (l < 1e-6) { exit 1 }
    printf "%.6f %.6f %.6f\n", u / l, v / l, l }' || ui_fail "($1, $2, $3) points at the eye"
}

# pixels_per_mm: logical pixels per millimetre square to the view, from
# the pick places of the inside edge and the floor's front edge: both run
# along X, the floor's front edge 35 mm towards -Y.
pixels_per_mm() {
  local ax ay bx by ex ey el fx fy fl
  read -r ax ay <<< "$(newest "Pick edge F1.b0/$INSIDE")"
  read -r bx by <<< "$(newest "Pick edge F1.b0/$FLOOR_FRONT")"
  [ -n "${ax:-}" ] && [ -n "${bx:-}" ] || ui_fail "no pick places of the edges logged"
  read -r ex ey el <<< "$(screen 1 0 0)"
  read -r fx fy fl <<< "$(screen 0 -1 0)"
  awk -v ax="$ax" -v ay="$ay" -v bx="$bx" -v by="$by" -v ex="$ex" -v ey="$ey" -v fx="$fx" -v fy="$fy" \
    -v fl="$fl" 'BEGIN {
    apart = ex * (by - ay) - ey * (bx - ax); along = 35 * fl * (ex * fy - ey * fx)
    if (along * along < 1e-6 || apart / along < 1) { exit 1 }
    printf "%.6f\n", apart / along }' ||
    ui_fail "no scale from the edges at $ax,$ay and $bx,$by"
}

# pointing command key edge dx dy dz description: the input's arrow on the
# edge points the way the view shows (dx, dy, dz) from it: its grab point
# (17 pixels beyond its knob along it) is off the edge's line on that
# side, by 5 pixels at least.
pointing() {
  local ex ey hx hy sx sy sl dx dy dl
  read -r ex ey <<< "$(newest "Pick edge F1.b0/$3")"
  read -r hx hy <<< "$(newest "Manipulator $1 $2")"
  [ -n "${ex:-}" ] && [ -n "${hx:-}" ] || ui_fail "$7: no places logged"
  read -r sx sy sl <<< "$(screen 1 0 0)"
  read -r dx dy dl <<< "$(screen "$4" "$5" "$6")"
  awk -v ex="$ex" -v ey="$ey" -v hx="$hx" -v hy="$hy" -v sx="$sx" -v sy="$sy" -v dx="$dx" -v dy="$dy" 'BEGIN {
    off = sx * (hy - ey) - sy * (hx - ex); want = sx * dy - sy * dx
    exit !(off * want > 0 && off * off >= 25) }' ||
    ui_fail "$7: the handle at $hx,$hy is not off the edge at $ex,$ey towards ($4, $5, $6)"
  echo "ok   $7 ($hx,$hy by $ex,$ey)"
}

# drag_by command key mm factor dx dy dz: drags the input's handle along
# the way the view shows (dx, dy, dz), the arrow's way, so that its value
# changes by about `mm` millimetres (negative: pushed back), its knob
# `factor` mm out per mm of the value.
# The cursor first goes 6 pixels square to the arrow's way and comes back
# onto it on the way to the end: the value follows the cursor along the
# arrow's line only, and a pull of fewer pixels than the overlay's 3-pixel
# drag threshold (the convex edge's arrow seen nearly end-on, in a view
# that the bottom cards make smaller) would otherwise be a click, which
# the handle keeps and which changes nothing (mitcad#1042, mitcad#1086,
# mitcad#1105).
drag_by() {
  local ux uy ul ppm step side
  read -r ux uy ul <<< "$(screen "$5" "$6" "$7")"
  ppm=$(pixels_per_mm)
  step=$(awk -v ux="$ux" -v uy="$uy" -v l="$ul" -v p="$ppm" -v m="$3" -v f="$4" '
    function near(v) { return v < 0 ? int(v - 0.5) : int(v + 0.5) }
    BEGIN { d = m * f * l * p; printf "%d %d", near(ux * d), near(uy * d) }')
  side=$(awk -v ux="$ux" -v uy="$uy" '
    function near(v) { return v < 0 ? int(v - 0.5) : int(v + 0.5) }
    BEGIN { printf "%d %d", near(-uy * 6), near(ux * 6) }')
  # shellcheck disable=SC2086 # two numbers each
  drag_sideways "Manipulator $1 $2" $step $side
}

# drag_sideways "<text>" dx dy sx sy: as ui_drag_from, by (dx, dy) from the
# place logged as "<text> at x,y", the first move to (sx, sy) beside it
# and the rest straight on to the end.
drag_sideways() {
  local fx fy i
  read -r fx fy <<< "$(ui_logged_at "$1")"
  ui_apart "$fx" "$fy"
  xdotool mousemove "$fx" "$fy" mousedown 1
  sleep 0.08
  xdotool mousemove $((fx + $4)) $((fy + $5))
  for i in 1 2 3 4 5 6 7 8; do
    sleep 0.08
    xdotool mousemove $((fx + $4 * (8 - i) / 8 + $2 * i / 8)) $((fy + $5 * (8 - i) / 8 + $3 * i / 8))
  done
  ui_sync
  xdotool mouseup 1
}

# dragged command key: where the newest drag of the input's handle ended.
dragged() { grep -F "Manipulator $1 $2 dragged to " "$UI_LOG" | tail -1 | sed 's/.* dragged to //'; }

# between value low high description: low < value < high.
between() {
  awk -v v="$1" -v l="$2" -v h="$3" 'BEGIN { exit !(v > l && v < h) }' ||
    ui_fail "$4: $1 is not between $2 and $3"
  echo "ok   $4: $1"
}

# half_back value: minus half the value (a push back by half of it).
half_back() { awk -v v="$1" 'BEGIN { print -v / 2 }'; }

# volume_now expected description [relative tolerance]: the bracket's
# volume in the newest line about it logged since the mark comes to the
# expected one (a roll-back undone before an edit may log a volume on the
# way).
volume_now() {
  local volume=""
  for _ in $(seq 1 150); do
    volume=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" |
      sed -n 's/.*(F1\.b0): volume [0-9.]* -> \([0-9.]*\) mm3.*/\1/p' | tail -1)
    if [ -n "$volume" ] &&
      awk -v v="$volume" -v e="$1" -v r="${3:-1e-6}" 'BEGIN { exit !((v - e) ^ 2 < (r * e) ^ 2 + 1e-4) }'; then
      echo "ok   $2: $volume mm3"
      return
    fi
    sleep 0.2
  done
  ui_fail "$2: volume ${volume:-not logged} mm3, expected $1"
}

# filled r: the bracket with its inside corner rounded at radius r.
filled() { awk -v r="$1" 'BEGIN { printf "%.6f", 22500 + 60 * r * r * (1 - atan2(1, 1)) }'; }
# rounded_off r: the bracket with its wall's top front edge rounded at r.
rounded_off() { awk -v r="$1" 'BEGIN { printf "%.6f", 22500 - 60 * r * r * (1 - atan2(1, 1)) }'; }
# bevelled d1 [d2]: the bracket with its inside corner bevelled d1 on one
# face and d2 (default d1) on the other.
bevelled() { awk -v a="$1" -v b="${2:-$1}" 'BEGIN { printf "%.6f", 22500 + 30 * a * b }'; }

# home_view: the bracket from its Home view, fitted.
home_view() {
  ui_step "the Home view (search)"         ui_command "Home"
  ui_expect_log "Camera direction -0.218 0.873 -0.436" "seen from the front, a little from the right and above"
  ui_step "fit (F6)"                       ui_key F6
  ui_sync
}

# pick_inside command: the inside edge in the command's Edges input.
pick_inside() {
  ui_mark
  ui_step "pick the inside edge"           ui_click_pick "edge F1.b0/$INSIDE"
  ui_expect_new "$1 Edges: 1 edge" "the inside edge picked"
}

# reference_floor: the floor's top as Chamfer's Reference Face, then the
# Edges input again, with the inside edge.
reference_floor() {
  ui_mark
  ui_step "the Reference Face input"       ui_click_logged "Panel Chamfer input sets.0.reference"
  ui_expect_new "Pick face F1.b0/F1:top at" "the floor's top can be picked"
  ui_step "pick the floor's top"           ui_click_pick "face F1.b0/F1:top"
  ui_expect_new "Chamfer Reference Face: 1 face" "the floor is the reference face"
  ui_mark
  ui_step "the Edges input again"          ui_click_logged "Panel Chamfer input sets.0.edges"
  ui_expect_new "Pick edge F1.b0/$INSIDE at" "the inside edge can be picked"
  pick_inside Chamfer
}

# concave_round layout [app arguments]: Fillet's Radius and Chamfer's
# Equal Distance on the inside corner, made and edited from the timeline,
# in one layout.
concave_round() {
  echo "=== Fillet and Equal Distance on the inside corner, $1"
  shift
  ui_start_app --open "$WORK/bracket.mitcad" "$@"
  home_view
  local r1 r2 d1 d2

  echo "--- Fillet"
  ui_step "fillet (F)"                     ui_key f
  pick_inside Fillet
  ui_expect_new "Manipulator Fillet sets.0.radius at" "Radius has an arrow"
  ui_wait_idle
  ui_sync
  pointing Fillet sets.0.radius "$INSIDE" 0 -1 1 "the arrow points into the open corner"
  ui_mark
  ui_step "pull it 4 mm into the corner"   drag_by Fillet sets.0.radius 4 "$ROUND" 0 -1 1
  ui_expect_new "Manipulator Fillet sets.0.radius dragged to" "the arrow dragged"
  r1=$(dragged Fillet sets.0.radius)
  between "$r1" 3 9 "pulled: the radius grew from 2 mm"
  ui_expect_new "Fillet Radius: $r1 mm = " "the panel's Radius followed"
  ui_expect_new "Preview Fillet: ok" "the preview followed"
  ui_wait_idle
  ui_sync
  ui_mark
  ui_step "OK (Enter)"                     ui_key Return
  ui_expect_new "Added fillet on 1 edge(s)" "fillet added"
  volume_now "$(filled "$r1")" "the corner filled at the dragged radius" 1e-5

  echo "--- Fillet edited from the timeline"
  ui_double_click_expect "Timeline Fillet1" " with Fillet" "editing Fillet1"
  ui_expect_new "Fillet Radius: $r1 mm" "the saved radius in the panel"
  ui_expect_new "Manipulator Fillet sets.0.radius at" "the arrow shown"
  ui_wait_idle
  ui_sync
  pointing Fillet sets.0.radius "$INSIDE" 0 -1 1 "the arrow points into the open corner"
  ui_mark
  ui_step "push it back half the radius"   drag_by Fillet sets.0.radius "$(half_back "$r1")" "$ROUND" 0 -1 1
  ui_expect_new "Manipulator Fillet sets.0.radius dragged to" "the arrow dragged"
  r2=$(dragged Fillet sets.0.radius)
  between "$r2" 0.5 "$r1" "pushed back: the radius shrank"
  ui_expect_new "Fillet Radius: $r2 mm = " "the panel's Radius followed"
  ui_expect_new "Preview Fillet: ok" "previewed"
  ui_wait_idle
  ui_mark
  ui_step "OK (Enter)"                     ui_key Return
  ui_expect_new "Edited F" "Fillet1 edited"
  volume_now "$(filled "$r2")" "the corner filled at the smaller radius" 1e-5
  ui_wait_idle
  ui_mark
  ui_step "undo the edit (Ctrl+Z)"         ui_key ctrl+z
  ui_expect_new "Undo: Edit Fillet1" "the edit taken back"
  volume_now "$(filled "$r1")" "the radius pulled first again" 1e-5
  ui_wait_idle
  ui_mark
  ui_step "undo the fillet (Ctrl+Z)"       ui_key ctrl+z
  ui_expect_new "Undo: Add Fillet1" "back to the bracket"

  echo "--- Chamfer, Equal Distance"
  ui_step "chamfer (search)"               ui_command "Chamfer"
  pick_inside Chamfer
  ui_expect_new "Manipulator Chamfer sets.0.distance at" "Distance has an arrow"
  ui_wait_idle
  ui_sync
  pointing Chamfer sets.0.distance "$INSIDE" 0 -1 1 "the arrow points into the open corner"
  ui_mark
  ui_step "pull it 3 mm into the corner"   drag_by Chamfer sets.0.distance 3 "$BEVEL" 0 -1 1
  ui_expect_new "Manipulator Chamfer sets.0.distance dragged to" "the arrow dragged"
  d1=$(dragged Chamfer sets.0.distance)
  between "$d1" 2 7 "pulled: the distance grew from 1 mm"
  ui_expect_new "Chamfer Distance: $d1 mm = " "the panel's Distance followed"
  ui_expect_new "Preview Chamfer: ok" "the preview followed"
  ui_wait_idle
  ui_sync
  ui_mark
  ui_step "OK (Enter)"                     ui_key Return
  ui_expect_new "Added chamfer (Equal Distance) on 1 edge(s)" "chamfer added"
  volume_now "$(bevelled "$d1")" "the corner bevelled at the dragged distance"

  echo "--- Equal Distance edited from the timeline"
  ui_double_click_expect "Timeline Chamfer1" " with Chamfer" "editing Chamfer1"
  ui_expect_new "Chamfer Distance: $d1 mm" "the saved distance in the panel"
  ui_expect_new "Manipulator Chamfer sets.0.distance at" "the arrow shown"
  ui_wait_idle
  ui_sync
  pointing Chamfer sets.0.distance "$INSIDE" 0 -1 1 "the arrow points into the open corner"
  ui_mark
  ui_step "push it back half the distance" drag_by Chamfer sets.0.distance "$(half_back "$d1")" "$BEVEL" 0 -1 1
  ui_expect_new "Manipulator Chamfer sets.0.distance dragged to" "the arrow dragged"
  d2=$(dragged Chamfer sets.0.distance)
  between "$d2" 0.2 "$d1" "pushed back: the distance shrank"
  ui_expect_new "Chamfer Distance: $d2 mm = " "the panel's Distance followed"
  ui_expect_new "Preview Chamfer: ok" "previewed"
  ui_wait_idle
  ui_mark
  ui_step "OK (Enter)"                     ui_key Return
  ui_expect_new "Edited F" "Chamfer1 edited"
  volume_now "$(bevelled "$d2")" "the corner bevelled at the smaller distance"
  ui_wait_idle
  ui_mark
  ui_step "undo (Ctrl+Z)"                  ui_key ctrl+z
  ui_expect_new "Undo: Edit Chamfer1" "the edit taken back"
  volume_now "$(bevelled "$d1")" "the distance pulled first again"
  grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
  ui_stop_app
}

ui_start_display

concave_round "Floating" --chrome=floating
concave_round "Docked" --chrome=docked

echo "=== Two Distances, Distance and Angle, a convex edge"
ui_start_app --open "$WORK/bracket.mitcad" --chrome=floating
home_view

echo "--- Two Distances, the floor as the reference face"
ui_step "chamfer (search)"                 ui_command "Chamfer"
ui_mark
ui_step "Two Distances"                    ui_choose "Panel Chamfer input sets.0.type" 1
ui_expect_new "Panel Chamfer input sets.0.reference at" "the Reference Face shown"
reference_floor
ui_expect_new "Manipulator Chamfer sets.0.distance2 at" "Distance 2 has an arrow"
ui_wait_idle
ui_sync
pointing Chamfer sets.0.distance "$INSIDE" 0 -1 0 "Distance's arrow points along the floor, away from the wall"
pointing Chamfer sets.0.distance2 "$INSIDE" 0 0 1 "Distance 2's arrow points up the wall, away from the floor"
ui_mark
ui_step "pull Distance 2 3 mm up the wall" drag_by Chamfer sets.0.distance2 3 "$ALONG" 0 0 1
ui_expect_new "Manipulator Chamfer sets.0.distance2 dragged to" "Distance 2's arrow dragged"
e2=$(dragged Chamfer sets.0.distance2)
between "$e2" 2 6 "pulled: Distance 2 grew from 1 mm"
ui_expect_new "Chamfer Distance 2: $e2 mm = " "the panel's Distance 2 followed"
ui_expect_new "Preview Chamfer: ok" "previewed"
ui_wait_idle
ui_sync
ui_mark
ui_step "pull Distance 4 mm along the floor" drag_by Chamfer sets.0.distance 4 "$ALONG" 0 -1 0
ui_expect_new "Manipulator Chamfer sets.0.distance dragged to" "Distance's arrow dragged"
e1=$(dragged Chamfer sets.0.distance)
between "$e1" 3 7 "pulled: Distance grew from 1 mm"
ui_expect_new "Chamfer Distance: $e1 mm = " "the panel's Distance followed"
ui_expect_new "Preview Chamfer: ok" "previewed"
ui_wait_idle
ui_mark
ui_step "Flip"                             ui_click_logged "Panel Chamfer input sets.0.flip"
ui_expect_new "Manipulator Chamfer sets.0.distance at" "the arrows moved"
ui_expect_new "Preview Chamfer: ok" "previewed"
ui_wait_idle
ui_sync
pointing Chamfer sets.0.distance "$INSIDE" 0 0 1 "flipped: Distance's arrow points up the wall"
pointing Chamfer sets.0.distance2 "$INSIDE" 0 -1 0 "flipped: Distance 2's arrow points along the floor"
ui_mark
ui_step "push Distance back down the wall" drag_by Chamfer sets.0.distance "$(half_back "$e1")" "$ALONG" 0 0 1
ui_expect_new "Manipulator Chamfer sets.0.distance dragged to" "Distance's arrow dragged"
e3=$(dragged Chamfer sets.0.distance)
between "$e3" 0.5 "$e1" "pushed back: Distance shrank"
ui_expect_new "Preview Chamfer: ok" "previewed"
ui_wait_idle
ui_mark
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Added chamfer (Two Distances) on 1 edge(s)" "chamfer added"
volume_now "$(bevelled "$e3" "$e2")" "Distance up the wall, Distance 2 along the floor"

echo "--- Two Distances edited from the timeline"
ui_double_click_expect "Timeline Chamfer1" " with Chamfer" "editing Chamfer1"
ui_expect_new "Chamfer Distance 2: $e2 mm" "the saved Distance 2"
ui_expect_new "Manipulator Chamfer sets.0.distance2 at" "Distance 2's arrow shown"
ui_wait_idle
ui_sync
pointing Chamfer sets.0.distance2 "$INSIDE" 0 -1 0 "Distance 2's arrow along the floor, as saved with Flip"
pointing Chamfer sets.0.distance "$INSIDE" 0 0 1 "Distance's arrow up the wall"
ui_mark
ui_step "push Distance 2 back"             drag_by Chamfer sets.0.distance2 "$(half_back "$e2")" "$ALONG" 0 -1 0
ui_expect_new "Manipulator Chamfer sets.0.distance2 dragged to" "Distance 2's arrow dragged"
e4=$(dragged Chamfer sets.0.distance2)
between "$e4" 0.5 "$e2" "pushed back: Distance 2 shrank"
ui_expect_new "Chamfer Distance 2: $e4 mm = " "the panel's Distance 2 followed"
ui_expect_new "Preview Chamfer: ok" "previewed"
ui_wait_idle
ui_mark
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Edited F" "Chamfer1 edited"
volume_now "$(bevelled "$e3" "$e4")" "bevelled at the smaller Distance 2"
ui_wait_idle
ui_mark
ui_step "undo the edit (Ctrl+Z)"           ui_key ctrl+z
ui_expect_new "Undo: Edit Chamfer1" "the edit taken back"
ui_wait_idle
ui_mark
ui_step "undo the chamfer (Ctrl+Z)"        ui_key ctrl+z
ui_expect_new "Undo: Add Chamfer1" "back to the bracket"

echo "--- Distance and Angle, the floor as the reference face"
ui_step "chamfer (search)"                 ui_command "Chamfer"
ui_mark
ui_step "Distance and Angle"               ui_choose "Panel Chamfer input sets.0.type" 2
ui_expect_new "Panel Chamfer input sets.0.angle at" "the angle shown"
reference_floor
ui_expect_new "Manipulator Chamfer sets.0.distance at" "Distance has an arrow"
ui_expect_new "Preview Chamfer: ok" "previewed"
ui_wait_idle
ui_sync
grep -q "Manipulator Chamfer sets.0.angle" "$UI_LOG" && ui_fail "the angle has a handle"
echo "ok   the angle has no handle"
pointing Chamfer sets.0.distance "$INSIDE" 0 -1 0 "Distance's arrow points along the floor, away from the wall"
ui_mark
ui_step "pull Distance 3 mm along the floor" drag_by Chamfer sets.0.distance 3 "$ALONG" 0 -1 0
ui_expect_new "Manipulator Chamfer sets.0.distance dragged to" "Distance's arrow dragged"
a1=$(dragged Chamfer sets.0.distance)
between "$a1" 2 6 "pulled: Distance grew from 1 mm"
ui_expect_new "Chamfer Distance: $a1 mm = " "the panel's Distance followed"
ui_expect_new "Preview Chamfer: ok" "previewed"
ui_wait_idle
ui_mark
ui_step "Flip"                             ui_click_logged "Panel Chamfer input sets.0.flip"
ui_expect_new "Manipulator Chamfer sets.0.distance at" "the arrow moved"
ui_expect_new "Preview Chamfer: ok" "previewed"
ui_wait_idle
ui_sync
pointing Chamfer sets.0.distance "$INSIDE" 0 0 1 "flipped: Distance's arrow points up the wall"
ui_mark
ui_step "push Distance back down the wall" drag_by Chamfer sets.0.distance "$(half_back "$a1")" "$ALONG" 0 0 1
ui_expect_new "Manipulator Chamfer sets.0.distance dragged to" "Distance's arrow dragged"
a2=$(dragged Chamfer sets.0.distance)
between "$a2" 0.5 "$a1" "pushed back: Distance shrank"
ui_expect_new "Preview Chamfer: ok" "previewed"
ui_wait_idle
ui_mark
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Added chamfer (Distance and Angle) on 1 edge(s)" "chamfer added"
# At 45 degrees the bevel goes as far along the floor as up the wall.
volume_now "$(bevelled "$a2")" "bevelled at the pushed-back distance"
ui_wait_idle
ui_mark
ui_step "undo the chamfer (Ctrl+Z)"        ui_key ctrl+z
ui_expect_new "Undo: Add Chamfer1" "back to the bracket"

echo "--- A convex edge: Fillet on the wall's top front edge"
ui_step "fillet (F)"                       ui_key f
ui_mark
ui_step "pick the wall's top front edge"   ui_click_pick "edge F1.b0/$WALL_TOP"
ui_expect_new "Fillet Edges: 1 edge" "the edge picked"
ui_expect_new "Manipulator Fillet sets.0.radius at" "Radius has an arrow"
ui_wait_idle
ui_sync
pointing Fillet sets.0.radius "$WALL_TOP" 0 1 -1 "the arrow points into the wall, to the rounding's middle"
ui_mark
# The wall is 5 mm thick: from 2 mm, 2.3 mm more (to 4 in whole mm; a
# pull of 2 mm or less moves the knob too few pixels for a drag).
ui_step "pull it 2.3 mm into the wall"     drag_by Fillet sets.0.radius 2.3 "$ROUND" 0 1 -1
ui_expect_new "Manipulator Fillet sets.0.radius dragged to" "the arrow dragged"
c1=$(dragged Fillet sets.0.radius)
between "$c1" 2 4.9 "pulled: the radius grew from 2 mm"
ui_expect_new "Preview Fillet: ok" "previewed"
ui_wait_idle
ui_mark
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Added fillet on 1 edge(s)" "fillet added"
volume_now "$(rounded_off "$c1")" "the wall's edge rounded off at the dragged radius" 1e-5
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
ui_stop_app

echo "=== Fillet's remembered Radius (mitcad#1028)"
# The test library starts panels from their defaults; here they remember.
# No fillet was made with them on before, so the first one starts at 2 mm.
export MITCAD_REMEMBER_PANEL_VALUES=1
ui_start_app --open "$WORK/bracket.mitcad"
home_view
ui_mark
ui_step "fillet (F)"                       ui_key f
ui_expect_new "Fillet Radius: 2 mm = 2 mm" "nothing remembered yet: the default 2 mm"
grep -q "Panel Fillet radius: remembered" "$UI_LOG" && ui_fail "a radius remembered before any OK"
pick_inside Fillet
ui_step "radius 3"                         ui_type_in "Panel Fillet input sets.0.radius" "3"
ui_expect_new "Fillet Radius: 3 = 3 mm" "3 mm typed"
ui_expect_new "Preview Fillet: ok" "previewed"
ui_wait_idle
ui_mark
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Added fillet on 1 edge(s)" "fillet added"
volume_now "$(filled 3)" "the corner filled at 3 mm" 1e-5
ui_wait_idle

echo "--- Fillet again starts with 3 mm"
ui_mark
ui_step "fillet (F)"                       ui_key f
ui_expect_new "Panel Fillet radius: remembered 3 mm" "the radius of the last OK remembered"
ui_expect_new "Fillet Radius: 3 mm = 3 mm" "the panel's Radius is 3 mm"
ui_step "radius 5"                         ui_type_in "Panel Fillet input sets.0.radius" "5"
ui_focus_main
ui_mark
ui_step "cancel (Esc)"                     ui_key Escape
ui_expect_new "Command Fillet cancelled" "the fillet cancelled"
ui_wait_idle

echo "--- An edit keeps nothing either"
ui_double_click_expect "Timeline Fillet1" " with Fillet" "editing Fillet1"
ui_expect_new "Fillet Radius: 3 mm" "the fillet's own radius in the panel"
ui_step "radius 4"                         ui_type_in "Panel Fillet input sets.0.radius" "4"
ui_expect_new "Preview Fillet: ok" "previewed"
ui_wait_idle
ui_mark
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Edited F" "Fillet1 edited"
volume_now "$(filled 4)" "the corner filled at 4 mm" 1e-5
ui_wait_idle
ui_stop_app

echo "--- A new start of the app: Fillet still starts with 3 mm"
grep -q 'solid.fillet.radius=3 mm' "$UI_CONFIG/Mitcad/Mitcad.conf" ||
  ui_fail "the settings do not keep the radius (commands/solid.fillet/radius)"
echo "ok   the settings keep commands/solid.fillet/radius=3 mm"
ui_start_app --open "$WORK/bracket.mitcad"
ui_mark
ui_step "fillet (F)"                       ui_key f
ui_expect_new "Panel Fillet radius: remembered 3 mm" "neither Cancel nor the edit changed it"
ui_expect_new "Fillet Radius: 3 mm = 3 mm" "the panel's Radius is 3 mm"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
export MITCAD_REMEMBER_PANEL_VALUES=0
ui_finish "UI fillet and chamfer handle test (inside corner)"
