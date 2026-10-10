#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands app/sketch
# The CREATE group (U4) through the real UI, with the volumes the model
# reports (analytic, or tools/cli/tests/f1_*.json):
#   - Extrude: a thin extrusion (walls centred on the profile), edited to
#     two sides; a start offset up to the XY plane.
#   - Revolve: two sides about the Y axis.
#   - Hole: two holes clicked on a face, placed exactly in the position
#     table, edited to counterbores through all, then to tapered
#     counterdrills; a counterdrilled, tapered hole at a sketch's circle
#     (as the FreeCAD import makes them) opens for editing with its values
#     and keeps its counterdrill when its taper changes.
#   - Thread: suggested from the cylinder, cosmetic, edited to modelled.
#   - Box, Cylinder, Sphere and Torus placed with a click on XY; New
#     Component.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-create-test.sh [screenshot.png]
# check-all sources: tools/cli

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-create.XXXXXX)
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
RECT='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'

# The models: the block of the T0 models (Sketch1 F1, Extrude1 F2) and
# sketches for the features: a 30 x 20 rectangle at (100, 60) (F3), a 10 x
# 20 one at (150, 0) to revolve about Y (F4), a 20 mm circle at (-50, 20)
# (F5). And a 10 mm rod 20 long (F1, F2) to thread.
cat > "$WORK/profiles.json" << EOF
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 60, "height": 40},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$RECT"}],
    "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F3", "corner": [100, 60], "width": 30, "height": 20},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F4", "corner": [150, 0], "width": 10, "height": 20},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_circle", "sketch": "F5", "center": [-50, 20], "diameter": 20}
]
EOF
cat > "$WORK/rod.json" << 'EOF'
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_circle", "sketch": "F1", "center": [0, 0], "diameter": 10},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "r{c1}"}],
    "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}}
]
EOF
# The demo's 60 x 40 x 20 block (F1, F2) with a hole as the FreeCAD import
# writes one: at the centre of a 6.6 mm circle (Sketch2 F4 on Plane1 F3, the
# top face's plane), counterdrilled 11 mm 3 deep at 90 degrees, tapered 2
# degrees, through all (Hole1 F5).
cat > "$WORK/drilled.json" << EOF
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 60, "height": 40},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$RECT"}],
    "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "construction_plane", "definition": {"type": "offset", "plane": "xy", "distance": 20}}},
  {"cmd": "sketch.create", "plane": "F3"},
  {"cmd": "sketch.add_circle", "sketch": "F4", "center": [30, 20], "diameter": 6.6},
  {"cmd": "add_feature", "def": {"type": "hole", "placement": {"type": "sketch_points", "sketch": "F4", "points": ["c1"]},
    "diameter": 6.6, "kind": {"type": "counterdrill", "diameter": 11, "depth": 3, "angle": 1.5707963267948966},
    "taper": 0.03490658503988659, "flat": true, "extent": {"type": "through_all"}, "flip": false,
    "participants": ["F2.b0"]}}
]
EOF
# hole_volume taper angle: what a 6.6 mm hole through the block's 20 mm
# takes away, its wall leaning in by the taper (degrees), counterdrilled 11
# mm 3 deep with a cone of the full angle (degrees) down to the wall: a
# cylinder and two frusta.
hole_volume() {
  awk -v taper="$1" -v angle="$2" 'BEGIN {
    pi = 3.14159265358979; r = 3.3; R = 5.5; d = 3
    lean = sin(taper * pi / 180) / cos(taper * pi / 180)
    slope = sin(angle * pi / 360) / cos(angle * pi / 360)
    zc = (R - r + d * slope) / (slope - lean); rc = r - zc * lean; rh = r - 20 * lean
    printf "%.6f", pi * (R * R * d + (zc - d) / 3 * (R * R + R * rc + rc * rc) + (20 - zc) / 3 * (rc * rc + rc * rh + rh * rh))
  }'
}
for model in profiles rod drilled; do
  "$CLI" run "$WORK/$model.json" --save "$WORK/$model.mitcad" > "$WORK/cli.log" 2>&1 ||
    { cat "$WORK/cli.log"; ui_fail "mitcad-cli $model"; }
done

ui_start_display

echo "=== Extrude and Revolve"
ui_start_app --open "$WORK/profiles.mitcad"
ui_step "fit (F6)"                         ui_key F6

echo "--- A thin extrusion of the rectangle at (100, 60)"
ui_step "extrude (E)"                      ui_key e
ui_expect_log "Extrude Profiles: 1 profile [profile r{c1} of F5]" "the newest profile taken"
ui_expect_log "Pick profile F3/" "profiles can be picked"
ui_step "take the circle away"             ui_click_pick "profile F5/"
ui_expect_log "Extrude Profiles: nothing" "circle taken away"
ui_step "pick the rectangle"               ui_click_pick "profile F3/"
ui_expect_log "Extrude Profiles: 1 profile [profile $RECT of F3]" "rectangle picked"
ui_step "Thin Extrude"                     ui_choose "Panel Extrude input type" 1
ui_expect_log "Extrude: Type = Thin Extrude" "thin chosen"
ui_step "wall in the centre"               ui_choose "Panel Extrude input wall" 1
ui_expect_log "Extrude: Wall Location = Center" "centred wall"
ui_step "thickness 2"                      ui_type_in "Panel Extrude input thickness" "2"
ui_step "distance 10"                      ui_type_in "Panel Extrude input distance" "10"
ui_expect_log "Extrude Distance: 10 = 10 mm" "distance typed"
ui_expect_log "Preview Extrude: ok" "previewed"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_log "Added thin extrude (New Body), distance 10" "thin extrusion added"
# Walls 2 thick round 30 x 20: (32 x 22 - 28 x 18) x 10.
ui_expect_volume "New body Body2 (F6.b0): volume \([0-9.]*\) mm3" 2000 "thin walls"

echo "--- Edited to two sides: 10 up and 5 down"
ui_step "double-click Extrude2"            ui_double_click_logged "Timeline Extrude2"
ui_expect_log "Editing F6 with Extrude" "editing Extrude2"
ui_expect_log "Extrude Wall Thickness: 2 mm" "the thickness came from the feature"
ui_step "Two Sides"                        ui_choose "Panel Extrude input direction" 1
ui_expect_log "Extrude: Direction = Two Sides" "two sides chosen"
ui_step "side two 5"                       ui_type_in "Panel Extrude input distance2" "5"
# Side two's wall is side one's until it is changed.
ui_expect_log "Extrude Wall Thickness 2: 2 mm" "side two's wall as side one's"
ui_expect_log "Preview Extrude: ok" "previewed"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_log "Edited F6" "Extrude2 edited"
ui_expect_volume "Body Body2 (F6.b0): volume 2000.000 -> \([0-9.]*\) mm3" 3000 "15 mm of walls"

echo "--- Revolve: two sides about Y, 90 and 45 degrees"
ui_step "revolve (search)"                 ui_command "Revolve"
ui_step "the profiles input"               ui_click_logged "Panel Revolve input profiles"
ui_step "take the circle away"             ui_click_pick "profile F5/"
ui_step "pick the 10 x 20 rectangle"       ui_click_pick "profile F4/"
ui_expect_log "Revolve Profiles: 1 profile [profile $RECT of F4]" "rectangle picked"
ui_step "the axis input"                   ui_click_logged "Panel Revolve input axis"
ui_step "pick the Y axis"                  ui_click_logged "Datum y"
ui_expect_log "Revolve Axis: 1 axis [axis y]" "Y axis picked"
ui_step "Angle"                            ui_choose "Panel Revolve input type" 1
ui_step "Two Sides"                        ui_choose "Panel Revolve input direction" 1
ui_expect_log "Revolve: Direction = Two Sides" "two sides chosen"
ui_expect_log "Manipulator Revolve angle at" "the angle has a ring"
ui_step "side two 45"                      ui_type_in "Panel Revolve input angle2" "45"
ui_expect_log "Preview Revolve: ok" "previewed"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_log "Added revolve (New Body), angle 90 deg" "revolve added"
# 135 of 360 degrees of the ring between radii 150 and 160, 20 high.
ui_expect_volume "New body Body3 (F7.b0): volume \([0-9.]*\) mm3" \
  "$(awk 'BEGIN { printf "%.6f", 0.375 * 3.14159265358979 * (160^2 - 150^2) * 20 }')" "revolved ring"

echo "--- Extrude from 25 below XY up to it"
ui_step "extrude (E)"                      ui_key e
ui_expect_log "Extrude Profiles: 1 profile [profile r{c1} of F5]" "the circle taken"
ui_step "Offset start"                     ui_choose "Panel Extrude input start" 1
ui_step "offset -25"                       ui_type_in "Panel Extrude input start_offset" "-25"
ui_expect_log "Extrude Offset: -25 = -25 mm" "start offset typed"
ui_step "To Object"                        ui_choose "Panel Extrude input extent" 1
ui_expect_log "Extrude: Extent = To Object" "to object chosen"
ui_expect_log "Datum xy at" "the object input takes planes"
ui_step "pick XY"                          ui_click_logged "Datum xy"
ui_expect_log "Extrude Object: 1 plane [plane xy]" "XY the object"
ui_expect_log "Preview Extrude: ok" "previewed"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_log "Added extrude (New Body), distance to object" "extrusion added"
ui_expect_volume "New body Body4 (F8.b0): volume \([0-9.]*\) mm3" 7853.981634 "25 mm of the circle"
[ -n "$SHOT" ] && ui_capture "$SHOT"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
ui_stop_app

echo "=== Hole"
ui_start_app --demo
ui_step "fit (F6)"                         ui_key F6
ui_step "hole (H)"                         ui_key h
ui_expect_log "Command Hole started" "hole started"
read -r hx hy <<< "$(ui_pick_at "face F2.b0/F2:end($RECT)")"
ui_step "click the top face"               xdotool mousemove "$hx" "$hy" click 1
ui_expect_log "Hole Face: 1 face" "the face picked"
ui_step "click it again beside"            xdotool mousemove $((hx + 20)) "$hy" click 1
ui_expect_log "Panel Hole input positions.1.x at" "a second row of the position table"
for row in "0 15" "1 45"; do
  read -r index x <<< "$row"
  ui_step "hole $index at x $x"            ui_type_in "Panel Hole input positions.$index.x" "$x"
  ui_step "hole $index at y 20"            ui_type_in "Panel Hole input positions.$index.y" "20"
done
ui_step "diameter 10"                      ui_type_in "Panel Hole input diameter" "10"
ui_expect_log "Manipulator Hole depth at" "the depth has an arrow"
ui_expect_log "Preview Hole: ok" "previewed"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_log "Added 2 hole(s) (simple), diameter 10" "holes added"
# Two holes of hole_simple_distance: 864.0506 mm3 each.
ui_expect_volume "Body Body1 (F2.b0): volume 48000.000 -> \([0-9.]*\) mm3" \
  "$(awk 'BEGIN { printf "%.6f", 48000 - 2 * (48000 - 47135.94936549554) }')" "two holes"

echo "--- Edited to counterbores through all (hole_counterbore)"
ui_step "double-click Hole1"               ui_double_click_logged "Timeline Hole1"
ui_expect_log "Editing F3 with Hole" "editing Hole1"
ui_step "All"                              ui_choose "Panel Hole input extent" 2
ui_step "Counterbore"                      ui_choose "Panel Hole input type" 1
ui_step "diameter 6.6"                     ui_type_in "Panel Hole input diameter" "6.6"
ui_step "counterbore 11"                   ui_type_in "Panel Hole input cb_diameter" "11"
ui_step "6.4 deep"                         ui_type_in "Panel Hole input cb_depth" "6.4"
ui_expect_log "Preview Hole: ok" "previewed"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_log "Edited F3" "Hole1 edited"
ui_expect_volume "Body Body1 (F2.b0): volume [0-9.]* -> \([0-9.]*\) mm3" \
  "$(awk 'BEGIN { printf "%.6f", 48000 - 2 * (48000 - 46926.50522389775) }')" "two counterbores"

echo "--- Edited to counterdrills, tapered 2 degrees"
ui_mark
ui_step "double-click Hole1"               ui_double_click_logged "Timeline Hole1"
ui_expect_new "Editing F3 with Hole" "editing Hole1 again"
ui_step "Counterdrill"                     ui_choose "Panel Hole input type" 2
ui_expect_new "Hole: Hole Type = Counterdrill" "counterdrill chosen"
ui_expect_new "Panel Hole input cd_angle at" "its diameter, depth and angle shown"
ui_step "counterdrill 11"                  ui_type_in "Panel Hole input cd_diameter" "11"
ui_step "3 deep"                           ui_type_in "Panel Hole input cd_depth" "3"
ui_step "taper 2 degrees"                  ui_type_in "Panel Hole input taper" "2"
ui_expect_new "Hole Taper Angle: 2 = 2 deg" "the taper typed"
ui_expect_new "Preview Hole: ok" "previewed"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Edited F3" "Hole1 edited to counterdrills"
ui_expect_new "Body Body1 (F2.b0): volume" "the block measured"
ui_expect_volume "Body Body1 (F2.b0): volume [0-9.]* -> \([0-9.]*\) mm3" \
  "$(awk -v one="$(hole_volume 2 90)" 'BEGIN { printf "%.6f", 48000 - 2 * one }')" "two tapered counterdrills"
ui_stop_app

echo "=== An imported counterdrilled, tapered hole"
ui_start_app --open "$WORK/drilled.mitcad"
ui_step "fit (F6)"                         ui_key F6
ui_step "double-click Hole1"               ui_double_click_logged "Timeline Hole1"
ui_expect_log "Editing F5 with Hole" "the hole opens for editing"
ui_expect_log "Panel Hole choices: placement=sketch, extent=through_all" "at sketch points, through all"
ui_expect_log "type=counterdrill, tap=simple, drill=flat" "a counterdrill with a flat bottom"
ui_expect_log "Hole Counterdrill Diameter: 11 mm" "its counterdrill's diameter"
ui_expect_log "Hole Counterdrill Angle: 90 deg" "its counterdrill's angle"
ui_expect_log "Hole Taper Angle: 2 deg" "its taper"
ui_step "taper 1 degree"                   ui_type_in "Panel Hole input taper" "1"
ui_step "counterdrill angle 60"            ui_type_in "Panel Hole input cd_angle" "60"
ui_expect_log "Hole Counterdrill Angle: 60 = 60 deg" "the angle typed"
ui_expect_log "Preview Hole: ok" "previewed"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_log "Edited F5" "Hole1 edited"
ui_expect_volume "Body Body1 (F2.b0): volume [0-9.]* -> \([0-9.]*\) mm3" \
  "$(awk -v one="$(hole_volume 1 60)" 'BEGIN { printf "%.6f", 48000 - one }')" "still counterdrilled, tapered 1 degree"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
ui_stop_app

echo "=== Thread, primitives, New Component"
ui_start_app --open "$WORK/rod.mitcad"
ui_step "fit (F6)"                         ui_key F6
ui_step "thread (search)"                  ui_command "Thread"
ui_step "pick the rod's side"              ui_click_pick "face F2.b0/F2:side(c1)"
ui_expect_log "Thread Faces: 1 face" "cylinder picked"
ui_expect_log "Preview Thread: ok" "previewed"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_log "Added cosmetic thread M10x1.5 6g on 1 face(s)" "the suggested size, cosmetic"
ui_expect_log "Cosmetic threads shown: 1" "the cosmetic thread drawn as rings"
ui_step "double-click Thread1"             ui_double_click_logged "Timeline Thread1"
ui_expect_log "Editing F3 with Thread" "editing Thread1"
ui_mark
ui_step "Modeled"                          ui_click_logged "Panel Thread input modeled"
ui_expect_new "Thread: Modeled = on" "modelled"
ui_expect_new "Preview Thread: ok" "the groove previewed" 120
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Edited F3" "Thread1 edited" 60
ui_expect_new "Body Body1 (F2.b0): volume 1570.796 ->" "the rod measured" 60
ui_expect_volume "Body Body1 (F2.b0): volume 1570.796 -> \([0-9.]*\) mm3" 1302.830611500115 \
  "the groove cut in (thread_modeled)" 1e-3
[ "$(grep -o 'Cosmetic threads shown: [0-9]*' "$UI_LOG" | tail -1)" = "Cosmetic threads shown: 0" ] ||
  ui_fail "a modelled thread is drawn as rings"
echo "ok   a modelled thread has no rings"

# primitive name volume body plane: placed with a click on an origin plane
# (on xy, xz, yz: away from each other) or a face.
primitive() {
  ui_step "$1 (search)"                    ui_command "$1"
  ui_expect_log "Command $1 started" "$1 started"
  case $4 in
    xy|xz|yz) ui_step "click $4"           ui_click_logged "Datum $4"
              ui_expect_log "$1 Plane: 1 plane [plane $4]" "$4 picked" ;;
    *)        ui_step "click the face"     ui_click_pick "$4"
              ui_expect_log "$1 Plane: 1 face" "face picked" ;;
  esac
  ui_expect_log "Preview $1: ok" "$1 previewed"
  ui_step "OK (Enter)"                     ui_key Return
  ui_expect_volume "New body $3 ([A-Z0-9]*.b0): volume \([0-9.]*\) mm3" "$2" "$1"
}
primitive "Box" 6000 Body2 xy
primitive "Cylinder" "$(awk 'BEGIN { printf "%.6f", 3.14159265358979 * 100 * 30 }')" Body3 xz
primitive "Sphere" "$(awk 'BEGIN { printf "%.6f", 3.14159265358979 * 30^3 / 6 }')" Body4 yz
ui_step "fit (F6)"                         ui_key F6
primitive "Torus" "$(awk 'BEGIN { printf "%.6f", 2 * 3.14159265358979^2 * 20 * 25 }')" Body5 \
  "face F4.b0/F4:"

ui_step "new component (search)"           ui_command "New Component"
ui_step "name it Bracket"                  ui_type_in "Panel New Component input name" "Bracket"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_log "New component Bracket" "component made"
ui_expect_log "Browser Root/Bracket:1 at" "the browser shows it"

grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
ui_finish "UI create test"
