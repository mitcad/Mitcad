#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands app/sketch app/framework/HybridAssembly.cpp
# check-all sources: app/framework/CommandPanel.cpp
# The CREATE group (U4) through the real UI, with the volumes the model
# reports (analytic, or tools/cli/tests/f1_*.json):
#   - Extrude: a thin extrusion (walls centred on the profile), edited to
#     two sides (a negative side ends on the other side, mitcad#765; ends
#     that meet refused); a start offset up
#     to the XY plane; a thin extrusion along
#     an open chain of sketch curves picked in the panel (mitcad#463).
#   - Revolve: two sides about the Y axis.
#   - Hole: a hole clicked on a face, its distances from the two nearest
#     edges typed (mitcad#656), edited to a counterbore through all, then to
#     a Clearance hole for M6 (mitcad#903: Normal fills 6.6, its counterbore
#     11 and 6.4; a typed depth of 8 stays when the size changes to M8 and
#     back, and when the hole opens again; a typed Countersink Diameter is
#     marked and its Reset to size makes it follow the size, mitcad#980),
#     then to
#     a tapered counterdrill; a counterdrilled, tapered hole at a sketch's
#     circle
#     (as the FreeCAD import makes them) opens for editing with its values
#     and keeps its counterdrill when its taper changes.
#   - Thread: suggested from the cylinder, cosmetic, edited to modelled,
#     then given extra radial and flank clearance, then end chamfers.
#   - Thread on a 5 mm hole (mitcad#691): picked in the open command it
#     suggests M6x1 6H, offers only a hole's classes, and keeps 6H when the
#     size changes; selected before Thread from the view's context menu it
#     suggests M6x1 6H too, and the modelled thread is made.
#   - Custom thread (mitcad#893): Thread Type Custom with a typed major
#     diameter (10) and pitch (1.5), Starts 2, modelled, its printed-fit
#     clearances set to 0: the two-start
#     thread of the M10x1.5 profile, whose volume is the single-start one's
#     (thread_modeled in f1_thread.json).
#   - Box, Cylinder, Sphere and Torus placed with a click on XY; New
#     Component.
#   - Emboss (mitcad#458): a rectangle wrapped onto a rod's side and
#     raised, edited to sunk (tools/cli/tests/emboss.json's volumes).
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
# An open L of lines (0,0)-(40,0)-(40,30) (Sketch1 F1) for thin walls
# along curves (mitcad#463).
cat > "$WORK/curves.json" << 'EOF'
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_line", "sketch": "F1", "start": [0, 0], "end": [40, 0]},
  {"cmd": "sketch.add_line", "sketch": "F1", "start": [40, 0], "end": [40, 30]}
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
# A rod of diameter 20, 30 long (F1, F2), and on XZ (through its axis) a
# 6 x 6 rectangle from z = 2 to 8 (F3) to emboss on its side.
cat > "$WORK/emboss.json" << EOF
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_circle", "sketch": "F1", "center": [0, 0], "diameter": 20},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "r{c1}"}],
    "extent": {"type": "distance", "distance": 30}, "operation": "new_body"}},
  {"cmd": "sketch.create", "plane": "xz"},
  {"cmd": "sketch.add_rectangle", "sketch": "F3", "corner": [-3, -8], "width": 6, "height": 6}
]
EOF
# A 30 x 20 block 6 high (F1, F2) with two 5 mm holes through it, the tap
# drill of M6x1 (mitcad#691): at (-7, 0) (Sketch2 F3, cut by Extrude2 F4)
# and at (7, 0) (Sketch3 F5, cut by Extrude3 F6).
cat > "$WORK/bores.json" << EOF
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [-15, -10], "width": 30, "height": 20},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$RECT"}],
    "extent": {"type": "distance", "distance": 6}, "operation": "new_body"}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_circle", "sketch": "F3", "center": [-7, 0], "diameter": 5},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F3", "region": "r{c1}"}],
    "extent": {"type": "through_all"}, "operation": "cut", "participants": ["F2.b0"]}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_circle", "sketch": "F5", "center": [7, 0], "diameter": 5},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F5", "region": "r{c1}"}],
    "extent": {"type": "through_all"}, "operation": "cut", "participants": ["F2.b0"]}}
]
EOF
for model in profiles rod drilled emboss curves bores; do
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

echo "--- Edited to two sides: 10 up and -4 (from 4 to 10 up), then 5 down"
ui_step "double-click Extrude2"            ui_double_click_logged "Timeline Extrude2"
ui_expect_log "Editing F6 with Extrude" "editing Extrude2"
ui_expect_log "Extrude Wall Thickness: 2 mm" "the thickness came from the feature"
ui_mark
ui_step "Two Sides"                        ui_choose "Panel Extrude input direction" 1
ui_expect_new "Extrude: Direction = Two Sides" "two sides chosen" 30
ui_expect_new "Preview Extrude: ok" "two sides previewed" 30
# A negative side ends on the other side's side (mitcad#765): with 10 up,
# -10 meets side one's end and is refused, -4 leaves walls from 4 to 10.
ui_mark
ui_step "side two -10"                     ui_type_in "Panel Extrude input distance2" "-10"
ui_expect_new "Panel Extrude error: The two sides overlap: Distance and Distance 2 must add up to more than zero (a negative distance ends on the other side)" \
  "ends that meet refused" 30
ui_mark
ui_step "side two -4"                      ui_type_in "Panel Extrude input distance2" "-4"
ui_expect_new "Preview Extrude: ok" "side two -4 previewed" 30
# Side two's wall is side one's until it is changed.
ui_expect_log "Extrude Wall Thickness 2: 2 mm" "side two's wall as side one's"
ui_mark
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Edited F6" "Extrude2 edited" 30
# 6 mm of walls off the sketch plane: 200 x 6.
ui_expect_volume "Body Body2 (F6.b0): volume 2000.000 -> \([0-9.]*\) mm3" 1200 "6 mm of walls off the sketch"
ui_mark
ui_step "double-click Extrude2"            ui_double_click_logged "Timeline Extrude2"
ui_expect_new "Editing F6 with Extrude" "editing Extrude2 again" 30
ui_mark
ui_step "side two 5"                       ui_type_in "Panel Extrude input distance2" "5"
ui_expect_new "Preview Extrude: ok" "side two 5 previewed" 30
ui_mark
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Edited F6" "Extrude2 edited again" 30
ui_expect_volume "Body Body2 (F6.b0): volume 1200.000 -> \([0-9.]*\) mm3" 3000 "15 mm of walls"

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

echo "=== Thin walls along an open chain of sketch curves (mitcad#463)"
ui_start_app --open "$WORK/curves.mitcad"
ui_step "fit (F6)"                         ui_key F6
ui_step "extrude (E)"                      ui_key e
ui_expect_log "Extrude Profiles: nothing" "no profile to take"
ui_step "Thin Extrude"                     ui_choose "Panel Extrude input type" 1
ui_expect_log "Extrude: Type = Thin Extrude" "thin chosen"
ui_step "pick the first line"              ui_click_logged "Sketch entity F1/c1"
ui_expect_log "Extrude Profiles: 1 sketch curve" "one line picked"
ui_expect_log "Preview Extrude: ok" "the first line previewed"
# The preview of the first line, the design's first body, leaves the view
# as it was (mitcad#467): the second line is picked right away where it
# was logged.
ui_step "pick the second line"             ui_click_logged "Sketch entity F1/c4"
ui_expect_log "Extrude Profiles: 2 sketch curves" "the open chain picked"
ui_step "thickness 2"                      ui_type_in "Panel Extrude input thickness" "2"
ui_step "distance 10"                      ui_type_in "Panel Extrude input distance" "10"
ui_expect_log "Preview Extrude: ok" "previewed"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_log "Added thin extrude (New Body), distance 10" "thin extrusion along the curves added"
# Side 1, to the right of the L as it bends left: round the outside of the
# corner, mitred: (2 x 40 + 2 x 30 + 2 x 2) x 10.
ui_expect_volume "New body Body1 (F2.b0): volume \([0-9.]*\) mm3" 1440 "walls along the L"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
ui_stop_app

echo "=== Hole"
ui_start_app --demo
ui_step "fit (F6)"                         ui_key F6
ui_step "hole (H)"                         ui_key h
ui_expect_log "Command Hole started" "hole started"
read -r hx hy <<< "$(ui_pick_at "face F2.b0/F2:end($RECT)")"
ui_step "click the top face"               xdotool mousemove "$hx" "$hy" click 1
ui_expect_log "Hole Place On: 1 face" "the face picked"
ui_expect_log "Place Hole: face face F2:end(" "measured from two of its edges"
ui_expect_log "Value box Hole at_offset1 at " "the distances are dimensions in the view"
ui_step "distance 1: 15"                   ui_type_in "Panel Hole input at_offset1" "15"
ui_step "distance 2: 20"                   ui_type_in "Panel Hole input at_offset2" "20"
ui_step "diameter 10"                      ui_type_in "Panel Hole input diameter" "10"
ui_expect_log "Manipulator Hole depth at" "the depth has an arrow"
ui_expect_log "Preview Hole: ok" "previewed"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_log "Added 1 hole(s) (simple), diameter 10" "hole added"
# A hole of hole_simple_distance: 864.0506 mm3.
ui_expect_volume "Body Body1 (F2.b0): volume 48000.000 -> \([0-9.]*\) mm3" \
  "$(awk 'BEGIN { printf "%.6f", 48000 - (48000 - 47135.94936549554) }')" "one hole"

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
  "$(awk 'BEGIN { printf "%.6f", 48000 - (48000 - 46926.50522389775) }')" "a counterbore"

echo "--- Edited to a Clearance hole for M6, its counterbore 8 deep (mitcad#903)"
ui_mark
ui_step "double-click Hole1"               ui_double_click_logged "Timeline Hole1"
ui_expect_new "Editing F3 with Hole" "editing Hole1 again"
ui_step "Clearance"                        ui_choose "Panel Hole input tap" 2
ui_expect_new "Hole: Hole Tap Type = Clearance" "Clearance chosen"
ui_expect_new "Hole sizes for M6 normal: diameter 6.6 mm, cb_diameter 11 mm, cb_depth 6.4 mm" \
  "M6 Normal fills the hole 6.6 and its counterbore 11 and 6.4"
for input in clearance_size clearance_fit; do
  grep "Panel Hole input $input at" "$UI_LOG" | tail -1 | grep -q "(hidden)" && ui_fail "$input hidden"
done
ui_step "8 deep"                           ui_type_in "Panel Hole input cb_depth" "8"
ui_expect_new "Hole cb_depth typed: kept when the size changes" "the typed depth is kept"
ui_mark
ui_step "Screw Size M8"                    ui_choose "Panel Hole input clearance_size" 1
ui_expect_new "Hole: Screw Size = M8" "M8 chosen"
ui_expect_new "Hole sizes for M8 normal: diameter 9 mm, cb_diameter 15 mm, cs_diameter" "M8 fills 9 and 15"
ui_expect_new "; kept cb_depth" "the typed depth stays at M8"
ui_mark
ui_step "Screw Size M6"                    ui_choose "Panel Hole input clearance_size" -1
ui_expect_new "Hole sizes for M6 normal: diameter 6.6 mm, cb_diameter 11 mm, cs_diameter" "back to 6.6 and 11"
ui_expect_new "; kept cb_depth" "the typed depth stays at M6"
# A typed Countersink Diameter no longer follows the size (mitcad#980): it
# is marked in its field, and the mark's Reset to size makes it follow
# again; back to the counterbore for the volume.
ui_mark
ui_step "Countersink"                      ui_choose "Panel Hole input type" 1
ui_expect_new "Hole: Hole Type = Countersink" "countersink chosen"
ui_step "countersink 17"                   ui_type_in "Panel Hole input cs_diameter" "17"
ui_expect_new "Hole cs_diameter typed: kept when the size changes" "the typed countersink is kept"
ui_expect_new "Panel Hole cs_diameter mark: Typed: does not follow the size" "and marked"
ui_expect_new "Panel Hole mark cs_diameter at " "its mark is in the field"
ui_mark
ui_step "Screw Size M8"                    ui_choose "Panel Hole input clearance_size" 1
ui_expect_new "; kept cb_depth, cs_diameter" "the typed countersink stays at M8"
tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qF "Panel Hole cs_diameter mark cleared" &&
  ui_fail "the typed countersink lost its mark at M8"
ui_mark
ui_step "Reset to size"                    ui_click_logged "Panel Hole mark cs_diameter"
ui_expect_new "Hole cs_diameter reset: follows the size" "Reset to size"
ui_expect_new "cs_diameter 17.92 mm" "the countersink follows M8 again"
ui_expect_new "Panel Hole cs_diameter mark cleared" "no longer marked"
ui_mark
ui_step "Screw Size M6"                    ui_choose "Panel Hole input clearance_size" -1
ui_expect_new "cs_diameter 13.44 mm" "and M6"
ui_step "Counterbore"                      ui_choose "Panel Hole input type" -1
ui_expect_new "Hole: Hole Type = Counterbore" "the counterbore again"
ui_expect_new "Preview Hole: ok" "previewed"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Edited F3" "Hole1 edited to a clearance hole"
# The block less a 6.6 mm hole through it and an 11 mm counterbore 8 deep.
ui_expect_volume "Body Body1 (F2.b0): volume [0-9.]* -> \([0-9.]*\) mm3" \
  "$(awk 'BEGIN { printf "%.6f", 48000 - 3.141592653589793 * (3.3 * 3.3 * 12 + 5.5 * 5.5 * 8) }')" \
  "an M6 clearance hole, its counterbore 8 deep"

echo "--- Edited to counterdrills, tapered 2 degrees"
ui_mark
ui_step "double-click Hole1"               ui_double_click_logged "Timeline Hole1"
ui_expect_new "Editing F3 with Hole" "editing Hole1 again"
ui_expect_new "tap=clearance, drill=angle, clearance_size=M6, clearance_fit=normal" "it opens as an M6 clearance hole"
ui_expect_new "Hole values kept from the size: cb_depth" "with its typed depth kept"
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
  "$(awk -v one="$(hole_volume 2 90)" 'BEGIN { printf "%.6f", 48000 - one }')" "a tapered counterdrill"
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

echo "=== Emboss: a rectangle wrapped onto a rod"
ui_start_app --open "$WORK/emboss.mitcad"
ui_step "fit (F6)"                         ui_key F6
ui_step "emboss (search)"                  ui_command "Emboss"
ui_expect_log "Command Emboss started" "emboss started"
ui_expect_log "Emboss Profiles: 1 profile [profile $RECT of F3]" "the newest profile taken"
ui_step "the face input"                   ui_click_logged "Panel Emboss input face"
ui_step "pick the rod's side"              ui_click_pick "face F2.b0/F2:side(c1)"
ui_expect_log "Emboss Face: 1 face" "the cylindrical face picked"
ui_step "depth 1"                          ui_type_in "Panel Emboss input depth" "1"
ui_expect_log "Preview Emboss: ok" "previewed"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_log "Added emboss (Raise) of 1 profile(s) on a cylindrical face, wrapped, depth 1" "emboss added"
# 36 mm2 wrapped onto radius 10 and raised to 11: 36 * 21 / 20 more.
ui_expect_volume "Body Body1 (F2.b0): volume 9424.778 -> \([0-9.]*\) mm3" 9462.577961 "raised on the rod"

echo "--- Edited to sink"
ui_mark
ui_step "double-click Emboss1"             ui_double_click_logged "Timeline Emboss1"
ui_expect_new "Editing F4 with Emboss" "editing Emboss1"
ui_expect_new "Emboss Face: 1 face" "the face loaded"
ui_step "Sink"                             ui_choose "Panel Emboss input mode" 1
ui_expect_new "Emboss: Raise or Sink = Sink" "sink chosen"
ui_expect_new "Preview Emboss: ok" "previewed"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Edited F4" "Emboss1 edited"
# Sunk to radius 9: 36 * 19 / 20 less than the rod.
ui_expect_volume "Body Body1 (F2.b0): volume [0-9.]* -> \([0-9.]*\) mm3" 9390.577961 "sunk into the rod"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
ui_stop_app

echo "=== Thread on a 5 mm hole (mitcad#691)"
BORE_A='F4:side(c1)'
BORE_B='F6:side(c1)'
HOLE_CLASSES="4H | 5G | 5H | 6G | 6H | 7G | 7H"
ui_start_app --open "$WORK/bores.mitcad"
ui_step "fit (F6)"                         ui_key F6

echo "--- Picked in the open command, then another size"
ui_mark
ui_step "thread (search)"                  ui_command "Thread"
ui_expect_new "Panel Thread choices: standard=iso_metric, size=10, designation=M10x1.5, class=6g" \
  "with nothing selected, M10x1.5 6g"
ui_expect_new "Pick face F2.b0/$BORE_A at" "the first hole can be picked"
ui_expect_new "Pick face F2.b0/$BORE_B at" "the second hole can be picked"
ui_mark
ui_step "pick the second hole"             ui_click_pick "face F2.b0/$BORE_B"
ui_expect_new "Thread Faces: 1 face [face $BORE_B of F2.b0" "the hole picked"
ui_expect_new "Panel Thread options class: $HOLE_CLASSES" "only a hole's classes offered"
ui_expect_new "Preview Thread: ok" "M6x1 6H previewed on the hole"
ui_mark
ui_step "Size: two down (8)"               ui_choose "Panel Thread input size" 2
ui_expect_new "Thread: Size = 8" "size 8 chosen"
ui_expect_new "Preview Thread: ok" "M8 previewed, its class still a hole's"
ui_focus_main
ui_mark
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Added cosmetic thread M8x1.25 6H on 1 face(s)" "the class stayed 6H for the hole"

echo "--- Selected before, Thread from the view's context menu"
ui_step "clear the selection (Esc)"        ui_key Escape
# Thread's pick places are where a click picks the face among faces only:
# outside a command the hole's rim in front of its wall may take that
# click. Fillet takes edges and faces, so its place for the wall is one
# no edge takes. Cancelled.
ui_mark
ui_step "fillet (search)"                  ui_command "Fillet"
ui_expect_new "Pick face F2.b0/$BORE_A at" "the first hole's wall clear of its edges"
ui_step "cancel (Esc)"                     ui_key Escape
ui_expect_new "Command Fillet cancelled" "cancelled"
at=$(ui_pick_at "face F2.b0/$BORE_A")
ui_mark
ui_step "right-click the first hole"       xdotool mousemove $at click 3
ui_expect_new "Selected: 1 face [face $BORE_A of F2.b0" "the hole selected"
ui_expect_new "Context menu: " "the context menu opened"
ui_mark
ui_step "Solid > Thread"                   ui_menu_choose "Solid" "Thread"
ui_expect_new "Command Thread started" "Thread started from the menu"
ui_expect_new "Thread Faces: 1 face [face $BORE_A of F2.b0" "the selected hole is the face"
ui_expect_new "Panel Thread choices: standard=iso_metric, size=6, designation=M6x1, class=6H" \
  "the hole selected before suggests M6x1 6H"
ui_expect_new "Panel Thread options class: $HOLE_CLASSES" "only a hole's classes offered"
ui_expect_new "Preview Thread: ok" "previewed"
ui_mark
ui_step "Modeled"                          ui_click_logged "Panel Thread input modeled"
ui_expect_new "Thread: Modeled = on" "modelled"
ui_expect_new "Preview Thread: ok" "the modelled thread previewed" 120
tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -F "Preview Thread: failed" && ui_fail "the thread was refused"
echo "ok   no class error"
ui_focus_main
ui_mark
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Added modeled thread M6x1 6H on 1 face(s)" "M6x1 6H modelled in the 5 mm hole" 60
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
ui_stop_app

echo "=== Custom thread (mitcad#893)"
# Its profile in every section through the axis is a single-start thread's
# of the same pitch, so two starts of 10 x 1.5 leave the rod with the
# modelled M10x1.5's volume (below); the geometry tests check the lead.
ui_start_app --open "$WORK/rod.mitcad"
ui_step "fit (F6)"                         ui_key F6
ui_mark
ui_step "thread (search)"                  ui_command "Thread"
ui_step "pick the rod's side"              ui_click_pick "face F2.b0/F2:side(c1)"
ui_expect_new "Thread Faces: 1 face" "cylinder picked"
ui_expect_new "Preview Thread: ok" "the suggested size previewed"
ui_mark
ui_step "Thread Type: Custom (the last)"   ui_choose "Panel Thread input standard" 8
ui_expect_new "Thread: Thread Type = Custom" "a custom thread"
ui_step "major diameter 10"                ui_type_in "Panel Thread input thread_major" "10"
ui_step "pitch 1.5"                        ui_type_in "Panel Thread input thread_pitch" "1.5"
ui_mark
ui_step "Modeled"                          ui_click_logged "Panel Thread input modeled"
ui_expect_new "Thread: Modeled = on" "modelled"
ui_step "Starts 2"                         ui_type_in "Panel Thread input starts" "2"
# A new modelled thread starts with the printed-fit clearances (radial 0.1,
# flank 0.1 capped at 0.0682 for this pitch: 1198.223 mm3 left); without
# them it is the plain profile of thread_modeled.
ui_step "radial clearance 0"               ui_type_in "Panel Thread input radial_clearance" "0"
ui_step "flank clearance 0"                ui_type_in "Panel Thread input flank_clearance" "0"
ui_expect_new "Preview Thread: ok" "the two-start thread previewed" 120
tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -F "Preview Thread: failed" && ui_fail "the custom thread was refused"
ui_focus_main
ui_mark
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Added modeled thread custom 10x1.5, 2 starts on 1 face(s)" "custom 10 x 1.5 on two starts, modelled" 60
ui_expect_volume "Body Body1 (F2.b0): volume 1570.796 -> \([0-9.]*\) mm3" 1302.830611500115 \
  "two starts of the M10x1.5 profile: the single-start thread's volume (thread_modeled)" 1e-3
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

echo "--- Extra clearance (thread_clearance in f1_thread.json, mitcad#195)"
ui_mark
ui_step "double-click Thread1"             ui_double_click_logged "Timeline Thread1"
ui_expect_new "Editing F3 with Thread" "editing Thread1 again"
ui_step "radial clearance 0.2"           ui_type_in "Panel Thread input radial_clearance" "0.2"
ui_step "flank clearance 0.05"             ui_type_in "Panel Thread input flank_clearance" "0.05"
ui_expect_new "Preview Thread: ok" "the loosened thread previewed" 120
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Edited F3" "Thread1 edited with clearance" 60
ui_expect_volume "Body Body1 (F2.b0): volume 130[0-9.]* -> \([0-9.]*\) mm3" 1156.647335933055 \
  "the profile moved in and its flanks relieved (thread_clearance)" 1e-3

echo "--- End chamfers (thread_chamfer in f1_thread.json, mitcad#304)"
ui_double_click_expect "Timeline Thread1" "Editing F3 with Thread" "editing Thread1 for its chamfers"
ui_step "Chamfer Ends"                     ui_click_logged "Panel Thread input chamfer"
ui_expect_new "Thread: Chamfer Ends = on" "chamfered"
ui_expect_new "Preview Thread: ok" "the chamfered thread previewed" 120
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Edited F3" "Thread1 edited with end chamfers" 60
ui_expect_volume "Body Body1 (F2.b0): volume 115[0-9.]* -> \([0-9.]*\) mm3" 1150.6351332522352 \
  "both free ends chamfered to the size's default (thread_chamfer)" 1e-3
ui_double_click_expect "Timeline Thread1" "Editing F3 with Thread" "editing Thread1: the chamfer loaded"
ui_step "Chamfer Size: Custom"             ui_choose "Panel Thread input chamfer_size" 1
ui_expect_new "Thread: Chamfer Size = Custom" "a size of its own"
ui_expect_new "Thread Chamfer Diameter: 8.226 mm" "starting at the size's default (M10x1.5: 8.376 - 0.15)"
ui_expect_new "Thread Chamfer Angle: 45°" "and 45 degrees"
ui_step "cancel (Escape)"                  ui_key Escape

# primitive name volume body plane: placed with a click on an origin plane
# (on xy, xz, yz: away from each other) or a face.
primitive() {
  ui_step "$1 (search)"                    ui_command "$1"
  ui_expect_log "Command $1 started" "$1 started"
  case $4 in
    xy|xz|yz) ui_step "click $4"           ui_click_logged "Datum $4"
              ui_expect_log "$1 Place On: 1 plane [plane $4]" "$4 picked" ;;
    *)        ui_step "click the face"     ui_click_pick "$4"
              ui_expect_log "$1 Place On: 1 face" "face picked" ;;
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
# The root has modelled bodies: asked first (mitcad#171, ui-hybrid-test.sh).
ui_expect_log "Hybrid assembly question: Root has" "asked about a hybrid assembly"
ui_focus_dialog '^Hybrid Assembly$'
ui_step "continue as is (Alt+O)"           ui_key alt+o
ui_focus_main
ui_expect_log "New component Bracket" "component made"
ui_expect_log "Browser Root/Bracket:1 at" "the browser shows it"

grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
ui_finish "UI create test"
