#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands app/framework/CommandSessionPicks.cpp app/framework/CommandSession.cpp app/browser/LibraryPicker.cpp
# The MODIFY group (U4) through the real UI, with the volumes the model
# reports (analytic, or tools/cli/tests/f2_*.json, f4_*.json):
#   - Fillet's Edges picked twice in a row, the radius typed without a
#     click into its field (mitcad#467).
#   - Fillet with two sets of edges, each its own radius; Chamfer with two
#     sets, one of two distances, edited.
#   - Fillet options (P6): the radius handle dragged, an asymmetric set
#     edited into G2, a variable radius through a mid radius, Press Pull on
#     the fillet's face opening the fillet; Chamfer with miter corners.
#   - Fillet and Chamfer of All Edges (mitcad#793): the body picked in
#     place of edges, the whole block rounded; edited from the timeline it
#     comes back as All Edges with its body (no list of edges) and is
#     saved so again.
#   - Shell (shell_inside_open_top), Draft (draft_face).
#   - Replace Face up to a plane, Press Pull of an edge (a fillet) and of a
#     face (an offset), Delete of the fillet's face, Split Face; Replace
#     Face onto another body's cylinder (tools/cli/tests/f2_face_ops.json).
#   - Move/Copy: translate, a copy moved freely (its arrow dragged), a
#     rotation; Combine, Align (a vertex onto a vertex), Split Body (its
#     plane drawn as the split extends it while Extend Splitting Tool is
#     on, none while it is off, mitcad#712);
#     the Material tool, Physical Properties (with the selected body's
#     Bounding Box and face, edge and vertex counts, mitcad#1095) and
#     Appearance (its picker, mitcad#1059).
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-modify-test.sh [screenshot.png]
# check-all sources: tools/cli

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
# Parts built over the origin, beside which an origin plane is clicked:
# the origin as 100 mm Centered squares (ui_origin_in_millimeters,
# mitcad#1748).
ui_origin_in_millimeters

echo "=== Fillet: two edges picked in a row (mitcad#467)"
# The keyboard goes to the radius after the first edge (mitcad#338), the
# Edges input takes the next click in the view, and the radius is typed
# without a click into its field.
ui_start_app --demo
ui_step "fit (F6)"                         ui_key F6
ui_step "fillet (F)"                       ui_key f
ui_mark
ui_step "pick the top front edge"          ui_click_pick "edge F2.b0/$TOP_FRONT"
ui_expect_new "Fillet Edges: 1 edge" "the first edge"
ui_step "pick the right bottom edge"       ui_click_pick "edge F2.b0/$RIGHT_BOTTOM"
ui_expect_new "Fillet Edges: 2 edges" "the second edge picked right after the first"
grep -q "Parameter labels for Fillet:" <(tail -n +$((UI_MARK + 1)) "$UI_LOG") &&
  ui_fail "the view showed parameter labels before anything was typed"
ui_step "type 3 (no click into the field)" xdotool type --delay 40 "3"
ui_sync
typed=$(ui_typed_since "$UI_MARK" "Panel field sets.0.radius: ")
[ "$typed" = "3" ] || ui_fail "the radius field has '$typed', expected '3'"
echo "ok   the radius typed without a click"
ui_expect_new "Parameter labels for Fillet:" "typing showed the names to pick"
ui_expect_new "Preview Fillet: ok" "previewed"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Added fillet on 2 edge(s)" "fillet on both edges added"
ui_expect_volume "Body Body1 (F2.b0): volume 48000.000 -> \([0-9.]*\) mm3" \
  "$(awk -v k="$CORNER" 'BEGIN { printf "%.6f", 48000 - k * 9 * (60 + 40) }')" "both edges at radius 3"
ui_stop_app

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

echo "=== Fillet and Chamfer of All Edges (mitcad#793)"
# rounded_box r: the 60 x 40 x 20 block with every edge rounded at r (the
# inner block grown by a ball of radius r).
rounded_box() {
  awk -v r="$1" 'BEGIN { pi = atan2(0, -1); a = 60 - 2 * r; b = 40 - 2 * r; c = 20 - 2 * r
    printf "%.6f", a * b * c + 2 * r * (a * b + b * c + a * c) + pi * r * r * (a + b + c) + 4 / 3 * pi * r ^ 3 }'
}
# block_volume low high description: the block's newest volume logged
# since the mark lies between low and high (an edit's roll-back may log
# one on the way: waited for).
block_volume() {
  local volume=""
  for _ in $(seq 1 150); do
    volume=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" |
      sed -n 's/.*Body Body1 (F2\.b0): volume [0-9.]* -> \([0-9.]*\) mm3.*/\1/p' | tail -1)
    if [ -n "$volume" ] && awk -v v="$volume" -v l="$1" -v h="$2" 'BEGIN { exit !(v > l && v < h) }'; then
      echo "ok   $3: $volume mm3"
      return
    fi
    sleep 0.2
  done
  ui_fail "$3: volume ${volume:-not logged} mm3, expected between $1 and $2"
}
# near expected: the bounds 1e-5 of it either side.
near() { awk -v e="$1" 'BEGIN { printf "%.6f %.6f", e * (1 - 1e-5), e * (1 + 1e-5) }'; }
# body_shown command: the set's Body input is shown, the Edges input not.
body_shown() {
  grep -F "Panel $1 input sets.0.body at" "$UI_LOG" | tail -1 | grep -q hidden &&
    ui_fail "$1: the Body input is hidden"
  grep -F "Panel $1 input sets.0.edges at" "$UI_LOG" | tail -1 | grep -q hidden ||
    ui_fail "$1: the Edges input is shown with All Edges on"
  echo "ok   $1: Body shown in place of Edges"
}
ui_start_app --demo
ui_step "fit (F6)"                         ui_key F6
ui_step "fillet (F)"                       ui_key f
ui_mark
ui_step "All Edges"                        ui_click_logged "Panel Fillet input sets.0.all_edges"
ui_expect_new "Fillet: All Edges = on" "All Edges on"
ui_expect_new "Panel Fillet input sets.0.body at" "the Body input laid out"
body_shown Fillet
ui_step "the Body input"                   ui_click_logged "Panel Fillet input sets.0.body"
ui_mark
ui_step "pick Body1"                       ui_click_logged "Browser Root/Bodies/Body1"
ui_expect_new "Fillet Body: 1 body [body F2.b0]" "the block picked"
ui_step "radius 2"                         ui_type_in "Panel Fillet input sets.0.radius" "2"
ui_expect_new "Preview Fillet: ok" "previewed"
ui_mark
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Added fillet on all edges" "a fillet of all edges added"
# shellcheck disable=SC2046 # two numbers
block_volume $(near "$(rounded_box 2)") "every edge rounded at 2 mm"

ui_mark
ui_step "double-click Fillet1"             ui_double_click_logged "Timeline Fillet1"
ui_expect_new " with Fillet" "editing Fillet1"
ui_expect_new "Fillet Body: 1 body [body F2.b0]" "All Edges came back with its body"
ui_expect_new "Panel Fillet input sets.0.body at" "the Body input laid out"
body_shown Fillet
ui_expect_new "Fillet Edges: nothing" "the hidden Edges input empty"
tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -E "Fillet Edges: [0-9]+ (edge|face)" &&
  ui_fail "the edit loaded a list of edges"
echo "ok   no list of edges loaded"
ui_step "radius 1"                         ui_type_in "Panel Fillet input sets.0.radius" "1"
ui_expect_new "Preview Fillet: ok" "previewed"
ui_mark
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Edited F" "Fillet1 edited"
# shellcheck disable=SC2046 # two numbers
block_volume $(near "$(rounded_box 1)") "every edge still rounded, at 1 mm"
ui_mark
ui_step "double-click Fillet1 again"       ui_double_click_logged "Timeline Fillet1"
ui_expect_new "Fillet Body: 1 body [body F2.b0]" "saved as All Edges again"
ui_step "cancel (Esc)"                     ui_key Escape
ui_step "undo the edit (Ctrl+Z)"           ui_key ctrl+z
ui_step "undo the fillet (Ctrl+Z)"         ui_key ctrl+z
ui_expect_log "Undo: Add Fillet1" "back to the block"

ui_step "chamfer (search)"                 ui_command "Chamfer"
ui_mark
ui_step "All Edges"                        ui_click_logged "Panel Chamfer input sets.0.all_edges"
ui_expect_new "Chamfer: All Edges = on" "All Edges on"
ui_expect_new "Panel Chamfer input sets.0.body at" "the Body input laid out"
body_shown Chamfer
ui_step "the Body input"                   ui_click_logged "Panel Chamfer input sets.0.body"
ui_step "pick Body1"                       ui_click_logged "Browser Root/Bodies/Body1"
ui_expect_new "Chamfer Body: 1 body [body F2.b0]" "the block picked"
ui_step "distance 1"                       ui_type_in "Panel Chamfer input sets.0.distance" "1"
ui_expect_new "Preview Chamfer: ok" "previewed"
ui_mark
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Added chamfer (Equal Distance) on all edges" "a chamfer of all edges added"
# 48000 - 1/2 (4 * 60 + 4 * 40 + 4 * 20) with the corners a few mm3 apart.
block_volume 47740 47780 "every edge bevelled 1 mm"
ui_mark
ui_step "double-click Chamfer1"            ui_double_click_logged "Timeline Chamfer1"
ui_expect_new " with Chamfer" "editing Chamfer1"
ui_expect_new "Chamfer Body: 1 body [body F2.b0]" "All Edges came back with its body"
body_shown Chamfer
ui_expect_new "Chamfer Edges: nothing" "the hidden Edges input empty"
tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -E "Chamfer Edges: [0-9]+ (edge|face)" &&
  ui_fail "the edit loaded a list of edges"
echo "ok   no list of edges loaded"
ui_step "cancel (Esc)"                     ui_key Escape
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
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
ui_show_origin
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
ui_show_origin
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
ui_show_origin
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
ui_mark
ui_step "offset 10"                        ui_type_in "Panel Split Body input offset" "10"
ui_expect_log "Preview Split Body: ok" "previewed"
# The plane as the split extends it, drawn across Body1 (mitcad#712): a
# face at x = 10; none with Extend Splitting Tool off, again with it on.
ui_expect_extended_tool() {
  for _ in $(seq 1 30); do
    tail -n +$((${UI_MARK:-0} + 1)) "$UI_LOG" | grep -Eq "^Preview extended tool: $1" && { echo "ok   $2"; return; }
    sleep 0.2
  done
  ui_fail "$2: no 'Preview extended tool: $1' in the log since the mark"
}
ui_expect_extended_tool '10\.0,-?[0-9.]+,-?[0-9.]+ to 10\.0,' "the extended plane drawn at x = 10"
ui_mark
ui_step "Extend Splitting Tool off"        ui_click_logged "Panel Split Body input extend"
ui_expect_extended_tool 'none$' "not extended: no extended tool drawn"
ui_mark
ui_step "Extend Splitting Tool on"         ui_click_logged "Panel Split Body input extend"
ui_expect_extended_tool '10\.0,-?[0-9.]+,-?[0-9.]+ to 10\.0,' "extended again: drawn again"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_volume "Body Body1 (F2.b0): volume 14000.000 -> \([0-9.]*\) mm3" 4000 "the piece below x = 10"
ui_expect_volume "New body [A-Za-z0-9]* (F10.b0): volume \([0-9.]*\) mm3" 10000 "the piece above"

# The Material tool gives the selection a material (mitcad#655, mitcad#1059).
ui_step "pick Body1"                       ui_click_logged "Browser Root/Bodies/Body1"
ui_mark
ui_step "Material (search)"                ui_command "Material"
ui_expect_new "Material Bodies: 1 body [body F2.b0]" "the Material tool takes the selected Body1"
ui_step "the picker: Aluminum"             ui_picker_choose "Panel Material input material" "Material picker" "Aluminum"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Material aluminum set for 1 body(ies)" "aluminium"
# A command takes the selection it starts with: Body1 picked again.
ui_step "pick Body1 again"                 ui_click_logged "Browser Root/Bodies/Body1"
ui_step "physical properties (search)"    ui_command "Physical Properties"
ui_expect_log "Physical properties: Body1 volume 4000 mass 0.0108 center (5, 10, 10)" \
  "4000 mm3 of aluminium weighs 10.8 g"
# The window's Bounding Box and counts of the selected body (mitcad#1095):
# the piece of the joined cubes below x = 10 is a 10 x 20 x 20 block.
ui_expect_log "Physical properties window \"Properties\": Body1: Bounding Box 10 x 20 x 20 mm, Faces / Edges / Vertices 6 / 12 / 8; Total: Bounding Box 10 x 20 x 20 mm, Faces / Edges / Vertices 6 / 12 / 8" \
  "the window shows the box and 6 faces, 12 edges and 8 vertices"
ui_focus_dialog "Properties"
ui_step "close the dialog (Esc)"           ui_key Escape
ui_focus_main
# Body1 is still selected after Physical Properties (mitcad#655):
# Appearance starts with it, and a click on its row would take it out.
ui_mark
ui_step "appearance (search)"              ui_command "Appearance"
ui_expect_new "Appearance Bodies: 1 body [body F2.b0]" "Appearance takes the selected Body1"
ui_step "the picker: Paint - Red"          ui_picker_choose "Panel Appearance input value" "Appearance picker" "Paint - Red"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_log "Appearance paint_red for 1 body(ies)" "painted red"

grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
ui_finish "UI modify test"
