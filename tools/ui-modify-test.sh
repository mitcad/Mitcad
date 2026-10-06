#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# The MODIFY group (U4) through the real UI, with the volumes the model
# reports (analytic, or tools/cli/tests/f2_*.json, f4_*.json):
#   - Fillet with two sets of edges, each its own radius; Chamfer with two
#     sets, one of two distances, edited.
#   - Fillet options (P6): the radius handle dragged, an asymmetric set
#     edited into G2, a variable radius through a mid radius, Press Pull on
#     the fillet's face opening the fillet; Chamfer with miter corners.
#   - Shell (shell_inside_open_top), Draft (draft_face).
#   - Replace Face up to a plane, Press Pull of an edge (a fillet) and of a
#     face (an offset), Delete of the fillet's face, Split Face; Replace
#     Face onto another body's cylinder (tools/cli/tests/f2_face_ops.json).
#   - Move/Copy: translate, a copy moved freely (its arrow dragged), a
#     rotation; Combine, Align (a vertex onto a vertex), Split Body;
#     Physical Material, Physical Properties and Appearance.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-modify-test.sh [screenshot.png]

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-modify.XXXXXX)
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
R='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'
TOP="F2:end($R)"
BOTTOM="F2:start($R)"
FRONT='F2:side(c1[c4,c2])'
RIGHT='F2:side(c2[c1,c3])'
TOP_FRONT="E{$TOP|$FRONT}"
RIGHT_BOTTOM="E{$RIGHT|$BOTTOM}"
CORNER=$(awk 'BEGIN { printf "%.10f", 1 - 3.14159265358979 / 4 }') # a fillet's section / r^2

# Two 20 mm cubes, at x = 0 (Body1) and x = 30 (Body2).
cat > "$WORK/cubes.json" << EOF
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 20, "height": 20},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$R"}],
    "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F3", "corner": [30, 0], "width": 20, "height": 20},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F3", "region": "$R"}],
    "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}}
]
EOF
"$CLI" run "$WORK/cubes.json" --save "$WORK/cubes.mitcad" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# A 60 x 40 x 20 block (Body1) and beside it a cylinder along X through
# (y, z) = (20, 0), R = 30, from x = 70 to 90 (Cylinder1, F3, Body2). Its
# plane's x axis is -Y, so that the curved face's pick place (the start of
# its mesh, at -Y) faces the view.
cat > "$WORK/arch.json" << EOF
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 60, "height": 40},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$R"}],
    "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "cylinder",
    "plane": {"origin": [70, 0, 0], "normal": [1, 0, 0], "x_axis": [0, -1, 0]},
    "center": [-20, 0], "diameter": 60, "height": 20, "operation": "new_body"}}
]
EOF
"$CLI" run "$WORK/arch.json" --save "$WORK/arch.mitcad" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

ui_start_display

echo "=== Fillet and Chamfer sets on the demo block"
ui_start_app --demo
ui_step "fit (F6)"                         ui_key F6
ui_step "fillet (F)"                       ui_key f
ui_step "pick the top front edge"          ui_click_pick "edge F2.b0/$TOP_FRONT"
ui_expect_log "Fillet Edges: 1 edge" "first set's edge"
ui_step "radius 2"                         ui_type_in "Panel Fillet input sets.0.radius" "2"
ui_step "add a set"                        ui_click_logged "Panel Fillet add sets"
ui_expect_log "Fillet: added sets row 2" "second set"
ui_expect_log "Panel Fillet input sets.1.edges at" "its inputs shown"
ui_step "pick the right bottom edge"       ui_click_pick "edge F2.b0/$RIGHT_BOTTOM"
ui_step "radius 5"                         ui_type_in "Panel Fillet input sets.1.radius" "5"
ui_expect_log "Preview Fillet: ok" "previewed"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_log "Added fillet on 2 edge(s) in 2 sets" "fillet added"
ui_expect_volume "Body Body1 (F2.b0): volume 48000.000 -> \([0-9.]*\) mm3" \
  "$(awk -v k="$CORNER" 'BEGIN { printf "%.6f", 48000 - k * (4 * 60 + 25 * 40) }')" "two radii"
ui_step "undo (Ctrl+Z)"                    ui_key ctrl+z
ui_expect_log "Undo: Add Fillet1" "fillet taken back"

ui_step "chamfer (search)"                 ui_command "Chamfer"
ui_step "pick the top front edge"          ui_click_pick "edge F2.b0/$TOP_FRONT"
ui_step "distance 2"                       ui_type_in "Panel Chamfer input sets.0.distance" "2"
ui_step "add a set"                        ui_click_logged "Panel Chamfer add sets"
ui_step "pick the right bottom edge"       ui_click_pick "edge F2.b0/$RIGHT_BOTTOM"
ui_step "Two Distances"                    ui_choose "Panel Chamfer input sets.1.type" 1
ui_step "distance 2"                       ui_type_in "Panel Chamfer input sets.1.distance" "2"
ui_step "distance 2 is 3"                  ui_type_in "Panel Chamfer input sets.1.distance2" "3"
ui_expect_log "Preview Chamfer: ok" "previewed"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_log "Added chamfer (Equal Distance) on 2 edge(s) in 2 sets" "chamfer added"
# 2 * 2 / 2 * 60 and 2 * 3 / 2 * 40.
ui_expect_volume "Body Body1 (F2.b0): volume 48000.000 -> \([0-9.]*\) mm3" 47760 "two chamfers"
ui_step "double-click Chamfer1"            ui_double_click_logged "Timeline Chamfer1"
ui_expect_log " with Chamfer" "editing Chamfer1"
ui_expect_log "Chamfer Distance 2: 3 mm" "set two's second distance came from the feature"
ui_step "set one 4"                        ui_type_in "Panel Chamfer input sets.0.distance" "4"
ui_expect_log "Preview Chamfer: ok" "previewed"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_log "Edited F" "Chamfer1 edited"
ui_expect_volume "Body Body1 (F2.b0): volume 47760.000 -> \([0-9.]*\) mm3" 47400 "the first chamfer 4 mm"
ui_stop_app

echo "=== Fillet options and chamfer corners (P6)"
ui_start_app --demo
ui_step "fit (F6)"                         ui_key F6
ui_step "fillet (F)"                       ui_key f
ui_step "pick the top front edge"          ui_click_pick "edge F2.b0/$TOP_FRONT"
ui_expect_log "Manipulator Fillet sets.0.radius at" "the radius has a handle"
ui_mark
ui_step "drag the radius handle"           ui_drag_from "Manipulator Fillet sets.0.radius" 30 30
ui_expect_new "Manipulator Fillet sets.0.radius dragged to" "the handle dragged"
ui_expect_new "Fillet Radius: " "the radius followed"
ui_step "Asymmetric"                       ui_choose "Panel Fillet input sets.0.size" 3
ui_expect_log "Panel Fillet input sets.0.distance2 at" "its second distance shown"
ui_step "distance 3"                       ui_type_in "Panel Fillet input sets.0.radius" "3"
ui_mark
ui_step "distance 2 is 6"                  ui_type_in "Panel Fillet input sets.0.distance2" "6"
ui_expect_new "Preview Fillet: ok" "previewed"
ui_step "OK (Enter)"                       ui_key Return
# 3 mm on the top (the edge's first face), 6 mm down the front: a quarter
# ellipse, (1 - pi/4) 3 6 60 (tools/cli/tests/f2_fillet_chamfer.json).
ui_expect_volume "Body Body1 (F2.b0): volume 48000.000 -> \([0-9.]*\) mm3" 47768.23001647 "asymmetric"
ui_step "double-click Fillet1"             ui_double_click_logged "Timeline Fillet1"
ui_expect_log " with Fillet" "editing Fillet1"
ui_expect_log "Fillet Distance 2: 6 mm" "the second distance came from the feature"
ui_mark
ui_step "Curvature (G2)"                   ui_choose "Panel Fillet input sets.0.continuity" 1
ui_expect_new "Panel Fillet input sets.0.weight at" "the tangency weight shown"
ui_expect_new "Preview Fillet: ok" "previewed"
ui_step "OK (Enter)"                       ui_key Return
# Mitcad's quintic: 131/960 of the legs' product, 48000 - 60 * 18 * 131/960.
ui_expect_volume "Body Body1 (F2.b0): volume 47768.230 -> \([0-9.]*\) mm3" 47852.625 "asymmetric G2"
ui_step "undo the edit (Ctrl+Z)"           ui_key ctrl+z
ui_step "undo the fillet (Ctrl+Z)"         ui_key ctrl+z
ui_expect_log "Undo: Add Fillet1" "back to the block"

ui_step "fillet (F)"                       ui_key f
ui_step "pick the top front edge"          ui_click_pick "edge F2.b0/$TOP_FRONT"
ui_step "Variable"                         ui_choose "Panel Fillet input sets.0.size" 2
ui_step "start 5"                          ui_type_in "Panel Fillet input sets.0.radius" "5"
ui_step "end 10"                           ui_type_in "Panel Fillet input sets.0.end" "10"
ui_step "add a mid radius"                 ui_click_logged "Panel Fillet add mid"
ui_expect_log "Panel Fillet input mid.0.position at" "the mid radius row"
ui_step "a quarter along"                  ui_type_in "Panel Fillet input mid.0.position" "0.25"
ui_mark
ui_step "6 mm"                             ui_type_in "Panel Fillet input mid.0.radius" "6"
ui_expect_new "Preview Fillet: ok" "previewed"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_volume "Body Body1 (F2.b0): volume 48000.000 -> \([0-9.]*\) mm3" 47239.89 \
  "5 to 10 mm through 6 mm (f2_fillet_chamfer.json)" 1e-5
ui_mark
ui_step "press pull (Q)"                   ui_key q
ui_step "pick the fillet's face"           ui_click_pick ":fillet($TOP_FRONT) at"
ui_expect_new "Press Pull opened Fillet" "Press Pull opened the fillet"
ui_expect_new " with Fillet" "editing it"
ui_mark
ui_step "start 4"                          ui_type_in "Panel Fillet input sets.0.radius" "4"
ui_expect_new "Preview Fillet: ok" "previewed"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Edited F" "the fillet edited"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
ui_stop_app

ui_start_app --demo
ui_step "fit (F6)"                         ui_key F6
ui_step "chamfer (search)"                 ui_command "Chamfer"
ui_step "pick the top front edge"          ui_click_pick "edge F2.b0/$TOP_FRONT"
ui_step "pick the top right edge"          ui_click_pick "edge F2.b0/E{$TOP|$RIGHT}"
ui_step "pick the front right edge"        ui_click_pick "edge F2.b0/E{$FRONT|$RIGHT}"
ui_expect_log "Chamfer Edges: 3 edges" "the corner's three edges"
ui_step "distance 2"                       ui_type_in "Panel Chamfer input sets.0.distance" "2"
ui_mark
ui_step "Miter"                            ui_choose "Panel Chamfer input corner" 1
ui_expect_new "Preview Chamfer: ok" "previewed"
ui_step "OK (Enter)"                       ui_key Return
# The bevels meet in a point: 48000 - (240 - 3 * 8/3 + 2).
ui_expect_volume "Body Body1 (F2.b0): volume 48000.000 -> \([0-9.]*\) mm3" 47766 "miter corner"
ui_stop_app

echo "=== Shell"
ui_start_app --demo
ui_step "fit (F6)"                         ui_key F6
ui_step "shell (search)"                   ui_command "Shell"
ui_step "pick the top face"                ui_click_pick "face F2.b0/$TOP"
ui_expect_log "Shell Faces/Body: 1 face" "top picked"
ui_expect_log "Manipulator Shell inside at" "the thickness has an arrow"
ui_expect_log "Preview Shell: ok" "previewed"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_volume "Body Body1 (F2.b0): volume 48000.000 -> \([0-9.]*\) mm3" 11712 "open box 2 mm thick"
ui_stop_app

echo "=== Draft"
ui_start_app --demo
ui_step "fit (F6)"                         ui_key F6
ui_step "draft (search)"                   ui_command "Draft"
ui_step "pick XY"                          ui_click_logged "Datum xy"
ui_expect_log "Draft Plane: 1 plane [plane xy]" "XY the fixed plane"
ui_step "pick the front face"              ui_click_pick "face F2.b0/$FRONT"
ui_expect_log "Draft Faces: 1 face" "front face picked"
ui_step "angle 5"                          ui_type_in "Panel Draft input angle" "5"
ui_expect_log "Preview Draft: ok" "previewed"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_log "Added draft of 1 face(s), angle 5 deg" "draft added"
ui_expect_volume "Body Body1 (F2.b0): volume 48000.000 -> \([0-9.]*\) mm3" 46950.13603768891 \
  "the front leaning in" 1e-6
ui_stop_app

echo "=== Replace Face, Press Pull, Delete, Split Face"
ui_start_app --demo
ui_step "fit (F6)"                         ui_key F6
ui_step "offset plane (search)"            ui_command "Offset Plane"
ui_step "pick XY"                          ui_click_logged "Datum xy"
ui_step "25 up"                            ui_type_in "Panel Offset Plane input distance" "25"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_log "Added plane Plane1: origin (0, 0, 25), normal (0, 0, 1)" "Plane1 at z = 25"
ui_step "replace face (search)"            ui_command "Replace Face"
ui_step "pick the top face"                ui_click_pick "face F2.b0/$TOP"
ui_step "the target input"                 ui_click_logged "Panel Replace Face input target"
ui_step "pick Plane1"                      ui_click_logged "Datum F3"
ui_expect_log "Replace Face Target Faces: 1 plane [plane F3]" "Plane1 the target"
ui_expect_log "Preview Replace Face: ok" "previewed"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_volume "Body Body1 (F2.b0): volume 48000.000 -> \([0-9.]*\) mm3" 60000 "the top up to z = 25"
ui_step "hide Plane1"                      ui_click_logged "Browser eye Root/Construction/Plane1"

ui_step "press pull (Q)"                   ui_key q
ui_step "pick the front face"              ui_click_pick "face F2.b0/$FRONT"
ui_expect_log "Manipulator Press Pull distance at" "the distance has an arrow"
ui_step "5 out"                            ui_type_in "Panel Press Pull input distance" "5"
ui_expect_log "Preview Press Pull: ok" "previewed"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_log "Press Pull made offset face, distance 5" "an offset face"
ui_expect_volume "Body Body1 (F2.b0): volume 60000.000 -> \([0-9.]*\) mm3" 67500 "the front 5 mm out"

ui_step "press pull (Q)"                   ui_key q
ui_step "pick the top front edge"          ui_click_pick "edge F2.b0/E{$FRONT|F4:replace"
ui_step "radius 3"                         ui_type_in "Panel Press Pull input distance" "3"
ui_expect_log "Preview Press Pull: ok" "previewed"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_log "Press Pull made fillet, distance 3" "a fillet"
ui_expect_volume "Body Body1 (F2.b0): volume 67500.000 -> \([0-9.]*\) mm3" \
  "$(awk -v k="$CORNER" 'BEGIN { printf "%.6f", 67500 - k * 9 * 60 }')" "edge rounded"

ui_step "delete face (search)"             ui_command "Delete Face"
ui_step "pick the fillet's face"           ui_click_pick "face F2.b0/F6:fillet("
ui_expect_log "Preview Delete Face: ok" "previewed"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_volume "Body Body1 (F2.b0): volume [0-9.]* -> \([0-9.]*\) mm3" 67500 "the edge sharp again (delface_fillet)"

ui_step "split face (search)"              ui_command "Split Face"
ui_step "pick the right face"              ui_click_pick "face F2.b0/$RIGHT"
ui_step "the tool input"                   ui_click_logged "Panel Split Face input tool"
ui_step "pick XY"                          ui_click_logged "Datum xy"
ui_step "offset 10"                        ui_type_in "Panel Split Face input offset" "10"
ui_expect_log "Preview Split Face: ok" "previewed"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_log "Added split face of 1 face(s)" "the right face split at z = 10"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
ui_stop_app

echo "=== Replace Face onto a cylinder"
ui_start_app --open "$WORK/arch.mitcad"
ui_step "fit (F6)"                         ui_key F6
ui_step "replace face (search)"            ui_command "Replace Face"
ui_step "pick the block's top"             ui_click_pick "face F2.b0/$TOP"
ui_expect_log "Replace Face Source Faces: 1 face" "the top picked"
ui_step "the target input"                 ui_click_logged "Panel Replace Face input target"
ui_step "pick the cylinder's side"         ui_click_pick "face F3.b0/F3:side0"
ui_expect_log "Replace Face Target Faces: 1 face" "the cylinder the target"
ui_expect_log "Preview Replace Face: ok" "previewed"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_log "Added replace face of 1 face(s)" "replace face added"
# z = sqrt(900 - (y - 20)^2): 60 (20 sqrt(500) + 900 asin(2/3)).
ui_expect_volume "Body Body1 (F2.b0): volume 48000.000 -> \([0-9.]*\) mm3" 66238.10916625366 \
  "the top up to the cylinder" 1e-6
ui_mark
ui_step "double-click ReplaceFace1"        ui_double_click_logged "Timeline ReplaceFace1"
ui_expect_new " with Replace Face" "editing ReplaceFace1"
ui_expect_new "Preview Replace Face: ok" "its faces and target loaded"
ui_step "cancel (Esc)"                     ui_key Escape
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
ui_stop_app

echo "=== Move/Copy, Combine, Align, Split Body, materials"
ui_start_app --open "$WORK/cubes.mitcad"
ui_step "fit (F6)"                         ui_key F6
ui_step "move (M)"                         ui_key m
ui_step "pick Body2"                       ui_click_logged "Browser Root/Bodies/Body2"
ui_expect_log "Move/Copy Objects: 1 body [body F4.b0]" "Body2 picked"
ui_step "Translate"                        ui_choose "Panel Move/Copy input type" 1
ui_step "15 to the left"                   ui_type_in "Panel Move/Copy input x" "-15"
ui_expect_log "Preview Move/Copy: ok" "previewed"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_log "Moved 1 bodies (Translate)" "Body2 moved"

ui_step "combine (search)"                 ui_command "Combine"
ui_step "pick Body1"                       ui_click_logged "Browser Root/Bodies/Body1"
ui_step "pick Body2"                       ui_click_logged "Browser Root/Bodies/Body2"
ui_expect_log "Combine Tool Bodies: 1 body [body F4.b0]" "the tool picked"
ui_expect_log "Preview Combine: ok" "previewed"
ui_step "OK (Enter)"                       ui_key Return
# Two cubes overlapping by 5 mm: 8000 + 8000 - 2000.
ui_expect_volume "Body Body1 (F2.b0): volume 8000.000 -> \([0-9.]*\) mm3" 14000 "joined"
ui_expect_log "Removed body Body2 (F4.b0)" "the tool went"

ui_step "fit (F6)"                         ui_key F6
ui_step "move (M)"                         ui_key m
ui_step "pick Body1"                       ui_click_logged "Browser Root/Bodies/Body1"
ui_expect_log "Manipulator Move/Copy tx at" "free move's arrows"
ui_expect_log "Manipulator Move/Copy rz at" "and rings"
ui_mark
ui_step "drag the X arrow"                 ui_drag_from "Manipulator Move/Copy tx" 80 0
ui_expect_new "Manipulator Move/Copy tx dragged to" "arrow dragged"
ui_expect_new "Move/Copy X Distance: " "the distance followed"
ui_step "X 100"                            ui_type_in "Panel Move/Copy input tx" "100"
ui_step "create a copy"                    ui_click_logged "Panel Move/Copy input copy"
ui_expect_log "Move/Copy: Create Copy = on" "a copy"
ui_expect_log "Preview Move/Copy: ok" "previewed"
[ -n "$SHOT" ] && ui_capture "$SHOT"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_log "Copied 1 bodies (Free Move)" "copied"
ui_expect_volume "New body [A-Za-z0-9]* (F7.b0): volume \([0-9.]*\) mm3" 14000 "the copy"
COPY=$(sed -n 's/.*New body \([A-Za-z0-9]*\) (F7.b0).*/\1/p' "$UI_LOG" | tail -1)

ui_step "fit (F6)"                         ui_key F6
ui_step "move (M)"                         ui_key m
ui_step "pick the copy"                    ui_click_logged "Browser Root/Bodies/$COPY"
ui_step "Rotate"                           ui_choose "Panel Move/Copy input type" 2
ui_step "pick the Z axis"                  ui_click_logged "Datum z"
ui_expect_log "Move/Copy Axis: 1 axis [axis z]" "Z picked"
ui_expect_log "Preview Move/Copy: ok" "previewed"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_log "Moved 1 bodies (Rotate)" "the copy turned"

ui_step "fit (F6)"                         ui_key F6
ui_step "align (search)"                   ui_command "Align"
ui_step "pick the copy"                    ui_click_logged "Browser Root/Bodies/$COPY"
ui_step "the from input"                   ui_click_logged "Panel Align input from"
ui_step "pick a vertex of the copy"        ui_click_pick "vertex F7.b0/"
vertex=$(grep "Align From: 1 vertex" "$UI_LOG" | tail -1 | sed -n 's/.*\[vertex \(.*\) of F7.b0\]$/\1/p')
[ -n "$vertex" ] || ui_fail "no vertex picked for Align"
echo "ok   from $vertex"
ui_step "pick a vertex of Body1"           ui_click_pick "vertex F2.b0/"
ui_expect_log "Align To: 1 vertex" "a vertex of Body1 the target"
ui_expect_log "Preview Align: ok" "previewed"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_log "Aligned 1 body(ies)" "aligned"

ui_step "split body (search)"              ui_command "Split Body"
ui_step "pick Body1"                       ui_click_logged "Browser Root/Bodies/Body1"
ui_step "the tool input"                   ui_click_logged "Panel Split Body input tool"
ui_step "pick YZ"                          ui_click_logged "Datum yz"
ui_step "offset 10"                        ui_type_in "Panel Split Body input offset" "10"
ui_expect_log "Preview Split Body: ok" "previewed"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_volume "Body Body1 (F2.b0): volume 14000.000 -> \([0-9.]*\) mm3" 4000 "the piece below x = 10"
ui_expect_volume "New body [A-Za-z0-9]* (F10.b0): volume \([0-9.]*\) mm3" 10000 "the piece above"

ui_step "physical material (search)"      ui_command "Physical Material"
ui_step "pick Body1"                       ui_click_logged "Browser Root/Bodies/Body1"
ui_step "Aluminum"                         ui_choose "Panel Physical Material input value" 3
ui_step "OK (Enter)"                       ui_key Return
ui_expect_log "Material aluminum for 1 body(ies)" "aluminium"
ui_step "physical properties (search)"    ui_command "Physical Properties"
ui_expect_log "Physical properties: Body1 volume 4000 mass 0.0108 center (5, 10, 10)" \
  "4000 mm3 of aluminium weighs 10.8 g"
ui_focus_dialog "Properties"
ui_step "close the dialog (Esc)"           ui_key Escape
ui_focus_main
ui_step "appearance (search)"              ui_command "Appearance"
ui_step "pick Body1"                       ui_click_logged "Browser Root/Bodies/Body1"
ui_step "Paint - Red"                      ui_choose "Panel Appearance input value" 5
ui_step "OK (Enter)"                       ui_key Return
ui_expect_log "Appearance paint_red for 1 body(ies)" "painted red"

grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
ui_finish "UI modify test"
