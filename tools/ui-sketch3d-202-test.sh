#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/browser app/commands app/MainWindowSketch3d.cpp app/MainWindow.cpp
# check-all sources: app/MainWindow.hpp app/MainWindowSelection.cpp app/OcctViewer.cpp app/OcctViewer.hpp
# check-all sources: app/framework/ModelShapes.cpp app/view/ViewController.cpp tools/cli
# check-all sources: core/model-api/src/sketch3d.rs core/sketch/src/spatial.rs core/sketch/src/spatial_convert.rs
# check-all sources: core/model/src/document/sketch_conversion.rs core/model/src/features/spatial_path.rs
# check-all sources: core/model/src/features/path.rs core/model/src/features/pipe.rs
# check-all sources: tools/ui-scripts/sketch3d-202.json
# The scenario of GitHub mitcad/Mitcad#202 end to end in the shared Qt UI
# (mitcad#1550, item 7): a sketch started on a face, turned into a 3D
# sketch, its route snapped onto geometry of another component on another
# plane, a Pipe along it. On tools/ui-scripts/sketch3d-202.json's design:
# Box1 (F1.b0) in the root, its top face at z 20; the component Post (O1)
# with a box (F2.b0) whose top corners are at z 50.
#   - Create Sketch (2D, the default) on Box1's top face; a 2-Point
#     Rectangle by two clicks.
#   - The Sketch Palette's 3D Sketch box: Sketch1 (F3) becomes a 3D sketch
#     in place, its frame still on the face (20 up, X and the face's
#     normal as before).
#   - The Home view (F11, the test's key) and Fit: the route is drawn in a
#     view that sees the face from above at an angle.
#   - Line (L) from a corner A of the rectangle by typed offsets 10, 0, 15:
#     the line AB leaves the face's plane.
#   - Tangent Arc (F9, the test's key) continuing AB at B, its end clicked
#     on a top corner V of Post's box: the Linked Point snap names the
#     vertex in Post, the end is attached to it through the occurrence O1.
#   - Finish; Pipe with Chain Selection off (A is also a corner of the
#     rectangle: chained, the route would branch there) along AB and the
#     arc, listed from A to V, smooth (no sharp corners), an 8 mm pipe:
#     pi 4^2 (|AB| + the arc's length), the arc from B along AB's direction
#     to V.
#   - Saved: Sketch1 is still a 3D sketch, the pipe's path is its two
#     curves; the pipe's start face lies at A and its end face at V (the
#     measure query, mitcad-cli).
# The Windows test is tools/ui-windows-sketch3d-202-test.ps1, the macOS one
# tools/ui-macos-sketch3d-202-test.sh (tools/ui-scripts/sketch3d-202.mitcad-ui).
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sketch3d-202-test.sh [screenshot.png]

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}
ROOT=$(cd "$(dirname "$0")/.." && pwd)
CLI=${UI_CLI:-$(dirname "$UI_APP")/../tools/cli/mitcad-cli}
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-sketch3d-202.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT
FILE=$WORK/route.mitcad

"$CLI" run "$ROOT/tools/ui-scripts/sketch3d-202.json" --save "$FILE" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# last_new "sed expression": what the expression prints of the last line
# logged since the mark that it matches.
last_new() { tail -n +$((UI_MARK + 1)) "$UI_LOG" | sed -n "$1" | tail -1; }

# esc_tool "Tool": Esc until the 3D sketch tool stops (its typed field, its
# chain, then the tool, one step back each), the sketch staying open.
esc_tool() {
  local i
  for i in 1 2 3 4; do
    ui_mark
    ui_step "Esc ($1, step $i)"          ui_key Escape
    ui_expect_new "3D sketch Esc: " "Esc steps back"
    ui_sync
    if tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qF "3D sketch tool $1: stopped"; then
      tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qF "Sketch finished" && ui_fail "Esc left the sketch"
      echo "ok   the $1 tool ends, the sketch stays open"
      return
    fi
  done
  ui_fail "Esc did not end the $1 tool"
}

ui_start_display
# Keys of the test's own: Tangent Arc and the Home view have none.
mkdir -p "$UI_CONFIG/Mitcad"
printf '[shortcuts]\nsketch.arc_tangent=F9\nview.home=F11\n' >> "$UI_CONFIG/Mitcad/Mitcad.conf"
ui_start_app --open "$FILE"
ui_step "fit (F6)"                       ui_key F6
ui_expect_log "Bodies shown: " "the bodies are shown"

echo "--- A 2D sketch on Box1's top face, a rectangle on it"
ui_mark
ui_step "create sketch (search)"         ui_command "Create Sketch"
ui_expect_new "Create Sketch kind: 2D" "2D is the default"
ui_expect_new "Create Sketch: select a plane" "Create Sketch asks for a plane or a face"
ui_expect_new "Pick face F1.b0/F1:top at" "Box1's top face can be picked"
ui_sync
ui_step "pick the top face"              ui_click_pick "face F1.b0/F1:top at"
ui_expect_new 'Sketch started on {"body":"F1.b0","face":"F1:top"}' "a 2D sketch on the face"
ui_expect_new "Sketch view " "the sketch view is logged"
ui_expect_new "Palette palette3dSketch at" "the Sketch Palette's 3D Sketch box"
ui_sync
ui_mark
ui_step "2-Point Rectangle (R)"          ui_key r
ui_step "first corner"                   ui_sketch_click 25 25
ui_step "opposite corner"                ui_sketch_click 45 40
ui_expect_new "Added rectangle " "a rectangle on the face"
ui_step "end the tool (Esc)"             ui_key Escape
ui_sync
[ -n "$SHOT" ] && ui_capture "${SHOT%.png}-2d.png"

echo "--- 3D Sketch checked: Sketch1 becomes a 3D sketch in place"
ui_mark
ui_step "check 3D Sketch"                ui_click_logged "Palette palette3dSketch"
ui_expect_new "Palette: 3D Sketch checked for F3" "the box asks for the conversion"
ui_expect_new "Converted Sketch1 (F3) to a 3D sketch: dof " "converted in place"
CONVERTED=$UI_MARK
ui_expect_new "View banner: 3D Sketch Sketch1" "the 3D editor takes it, under its own name"
ui_expect_new "3D sketch frame in the design: " "its frame is logged"
FRAME=$(last_new 's/^3D sketch frame in the design: //p')
[[ $FRAME == *", x axis (1.000000, 0.000000, 0.000000), normal (0.000000, 0.000000, 1.000000)" ]] ||
  ui_fail "the frame is not the face's: $FRAME"
read -r OX OY OZ <<< "$(sed -n 's/^origin (\([^,]*\), \([^,]*\), \([^)]*\)).*/\1 \2 \3/p' <<< "$FRAME")"
awk -v z="$OZ" 'BEGIN { exit !((z - 20) ^ 2 < 1e-12) }' || ui_fail "the frame is not on the top face: $FRAME"
echo "ok   the frame stays on the face, 20 up, X and the face's normal"
ui_expect_new "3D sketch entity F3/p" "the rectangle's corners are drawn where a click picks them"
A=$(tail -n +$((CONVERTED + 1)) "$UI_LOG" | sed -n 's/^3D sketch entity F3\/\(p[0-9]*\) at .*/\1/p' | head -1)
[ -n "$A" ] || ui_fail "no corner of the rectangle logged"
echo "ok   the route starts at the corner $A"

echo "--- The Home view and Fit: the face seen from above at an angle"
ui_mark
ui_step "Home (F11)"                     ui_key F11
ui_expect_new "View home" "the Home view"
ui_step "fit (F6)"                       ui_key F6
ui_expect_new "3D sketch entity F3/$A at" "the corner logged again where it is now"
ui_sync

echo "--- The line AB off the face by typed offsets"
ui_mark
ui_step "Line (L)"                       ui_key l
ui_expect_new "3D sketch tool Line: started" "Line draws in the converted sketch"
ui_expect_new "Pick vertex F2.b0/V{" "the vertices of Post's box are logged where a click meets them"
ui_mark
ui_step "click the corner $A"            ui_click_logged "3D sketch entity F3/$A"
ui_expect_new "3D sketch Line: pending point 1 at (" "the start is pending at the corner"
APOS=$(last_new 's/^3D sketch Line: pending point 1 at (\([^)]*\)).*/\1/p')
read -r AX AY AZ <<< "${APOS//,/}"
awk -v z="$AZ" 'BEGIN { exit !(z ^ 2 < 1e-12) }' || ui_fail "the corner is not in the face's plane: ($APOS)"
echo "ok   A at ($APOS), in the face's plane"
ui_expect_new "3D sketch field dx at" "the Offsets fields are shown"
ui_step "Delta X"                        ui_click_logged "3D sketch field dx"
ui_step "type 10"                        xdotool type --delay 40 -- "10"
ui_expect_new "3D sketch field dx: 10" "Delta X typed"
ui_step "Tab to Delta Y"                 ui_key Tab
ui_expect_new "3D sketch field focus: dy" "Tab reached Delta Y"
ui_step "type 0"                         xdotool type --delay 40 -- "0"
ui_expect_new "3D sketch field dy: 0" "Delta Y typed"
ui_step "Tab to Delta Z"                 ui_key Tab
ui_expect_new "3D sketch field focus: dz" "Tab reached Delta Z"
ui_step "type 15"                        xdotool type --delay 40 -- "15"
ui_expect_new "3D sketch field dz: 15" "Delta Z typed"
ui_mark
ui_step "Enter"                          ui_key Return
ui_expect_new '"offsets":["10' "the typed values go as offsets"
ui_expect_new "3D sketch Line: made c" "AB made, one step"
read -r L FROM B <<< "$(last_new 's/^3D sketch Line: made \(c[0-9]*\) from \(p[0-9]*\) to \(p[0-9]*\)$/\1 \2 \3/p')"
[ "$FROM" = "$A" ] || ui_fail "AB does not start at the corner $A: ${FROM:-none}"
echo "ok   AB is $L from $A to $B"
ui_expect_new "3D sketch entity F3/$B at" "B is drawn where a click picks it"
esc_tool Line

echo "--- A tangent arc from B onto a top corner of Post's box"
ui_mark
ui_step "Tangent Arc (F9)"               ui_key F9
ui_expect_new "3D sketch tool Tangent Arc: started" "Tangent Arc draws in the 3D sketch"
ui_expect_new "Pick vertex F2.b0/V{" "Post's vertices logged where a click meets them"
ui_sync
START=$(grep -n "^Pick places:" "$UI_LOG" | tail -1 | cut -d: -f1)
VNAME=$(tail -n +"${START:-1}" "$UI_LOG" | sed -n 's/^Pick vertex F2\.b0\/\(V{[^ ]*F2:top[^ ]*}\) at [0-9]*,[0-9]*$/\1/p' | head -1)
[ -n "$VNAME" ] || ui_fail "no top corner of Post's box to snap to"
echo "ok   the top corner $VNAME"
ui_mark
ui_step "click B, AB's end"              ui_click_logged "3D sketch entity F3/$B"
ui_expect_new "3D sketch Tangent Arc: continues $L at $B" "the arc continues AB at B"
ui_mark
ui_step "click the corner of Post"       ui_click_pick "vertex F2.b0/$VNAME at"
ui_expect_new "3D sketch snap: Linked Point: vertex $VNAME of F2.b0 in Post at (" \
  "the vertex of another component snaps, its badge names it and Post"
VPOS=$(last_new 's/^3D sketch snap: Linked Point: vertex .* at (\([^)]*\))$/\1/p')
read -r VX VY VZ <<< "${VPOS//,/}"
ui_expect_new '"cmd":"sketch3d.add_arc"' "one sketch3d.add_arc"
ui_expect_new '"mode":"tangent"' "a tangent arc"
ui_expect_new "\"vertex\":\"$VNAME\"" "its end on the vertex"
ui_expect_new '"occurrence":"O1"' "through Post's occurrence"
ui_expect_new "3D sketch Tangent Arc: made c" "the arc made, one step"
read -r R FROM V <<< "$(last_new 's/^3D sketch Tangent Arc: made \(c[0-9]*\) from \(p[0-9]*\) to \(p[0-9]*\)$/\1 \2 \3/p')"
[ "$FROM" = "$B" ] || ui_fail "the arc does not start at B ($B): ${FROM:-none}"
echo "ok   the arc is $R from $B to $V at ($VPOS)"
[ -n "$SHOT" ] && ui_capture "${SHOT%.png}-route.png"
esc_tool "Tangent Arc"
ui_mark
ui_step "finish (Ctrl+Enter)"            ui_key ctrl+Return
ui_expect_new "Sketch finished" "finished"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"

# Where A and V are in the design (the frame: its origin, X and the face's
# normal), V a top corner of Post's box; the route's length and the pipe's
# volume: AB (10, 0, 15), then the arc from B along AB's direction to V:
# the chord BV at an angle phi to that direction, the arc 2 phi round,
# |BV| phi / sin phi long.
read -r DAX DAY DAZ DVX DVY DVZ <<< "$(awk -v ox="$OX" -v oy="$OY" -v oz="$OZ" \
  -v ax="$AX" -v ay="$AY" -v az="$AZ" -v vx="$VX" -v vy="$VY" -v vz="$VZ" \
  'BEGIN { printf "%.6f %.6f %.6f %.6f %.6f %.6f\n", ox + ax, oy + ay, oz + az, ox + vx, oy + vy, oz + vz }')"
awk -v x="$DVX" -v y="$DVY" -v z="$DVZ" 'BEGIN {
  ok = ((x - 80) ^ 2 < 1e-6 || (x - 100) ^ 2 < 1e-6) && ((y - 60) ^ 2 < 1e-6 || (y - 80) ^ 2 < 1e-6) && (z - 50) ^ 2 < 1e-6
  exit !ok }' || ui_fail "V ($DVX, $DVY, $DVZ) is not a top corner of Post's box"
echo "ok   V at ($DVX, $DVY, $DVZ) in the design, a top corner of Post's box"
VOLUME=$(awk -v ax="$AX" -v ay="$AY" -v az="$AZ" -v vx="$VX" -v vy="$VY" -v vz="$VZ" 'BEGIN {
  pi = atan2(0, -1); ab = sqrt(10 * 10 + 15 * 15)
  cx = vx - (ax + 10); cy = vy - ay; cz = vz - (az + 15); c = sqrt(cx * cx + cy * cy + cz * cz)
  cosphi = (10 * cx + 15 * cz) / (ab * c); phi = atan2(sqrt(1 - cosphi * cosphi), cosphi)
  printf "%.6f", pi * 16 * (ab + c * phi / sin(phi)) }')

echo "--- Pipe along AB and the arc"
ui_mark
ui_step "pipe (search)"                  ui_command "Pipe"
ui_expect_new "Panel Pipe input chain at" "the Pipe panel"
ui_expect_new "Sketch entity F3/$L at" "the route's curves are where a click picks them"
ui_sync
ui_mark
ui_step "Chain Selection off"            ui_click_logged "Panel Pipe input chain"
ui_expect_new "Pipe: Chain Selection = off" "the picks are taken as they are (A also joins the rectangle)"
ui_sync
ui_mark
ui_step "pick AB"                        ui_click_logged "Sketch entity F3/$L"
ui_expect_new "Pipe Path listed: $L ($A to $B)" "AB from A"
ui_sync
ui_mark
ui_step "pick the arc"                   ui_click_logged "Sketch entity F3/$R"
ui_expect_new "Pipe Path listed: $L, $R ($A to $V)" "the route from the corner to the vertex"
ui_expect_new "Preview Pipe: ok" "previewed"
# The corners are logged when they change: AB alone had none, and the
# tangent arc adds none, so the last count stays 0.
grep -F "Pipe: the path has " "$UI_LOG" | tail -1 | grep -qF "Pipe: the path has 0 sharp corners" ||
  ui_fail "the route is not smooth: $(grep -F "Pipe: the path has " "$UI_LOG" | tail -1)"
echo "ok   the tangent arc makes the route smooth"
[ -n "$SHOT" ] && ui_capture "$SHOT"
ui_mark
ui_step "OK (Enter)"                     ui_key Return
ui_expect_new "Added pipe" "Pipe1 added"
ui_expect_volume "New body [^ ]* (F4.b0): volume \([0-9.]*\) mm3" "$VOLUME" "an 8 mm pipe along the whole route" 1e-3
ui_mark
ui_step "save (Ctrl+S)"                  ui_key ctrl+s
ui_expect_new "Saved $FILE" "saved"

echo "--- The saved design: a 3D sketch, the pipe from A to V"
cat > "$WORK/check.json" << EOF
[
  {"expect": {"query": {"query": "feature", "uid": "F3"}, "result": {"name": "Sketch1", "type": "spatial_sketch", "status": "ok"}}},
  {"expect": {"query": {"query": "feature", "uid": "F4"}, "result": {"name": "Pipe1", "type": "pipe", "status": "ok",
    "def": {"path": {"spatial_sketch": "F3", "curves": ["$L", "$R"]}}}}},
  {"expect": {"query": {"query": "measure", "a": {"body": "F4.b0", "face": "F4:start"}},
    "result": {"a": {"plane": {"origin": [$DAX, $DAY, $DAZ]}}}, "tolerance": 1e-4}},
  {"expect": {"query": {"query": "measure", "a": {"body": "F4.b0", "face": "F4:end"}},
    "result": {"a": {"plane": {"origin": [$DVX, $DVY, $DVZ]}}}, "tolerance": 1e-4}}
]
EOF
"$CLI" run "$WORK/check.json" --open "$FILE" > "$WORK/check.log" 2>&1 ||
  { cat "$WORK/check.log"; ui_fail "the saved design: not a 3D sketch, or the pipe not from the corner to Post's vertex"; }
echo "ok   Sketch1 stayed a 3D sketch; the pipe's path is $L and $R of it, from A to Post's vertex V"

grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
ui_crashed && ui_fail "the application crashed"
ui_finish "UI 3D sketch of GitHub mitcad/Mitcad#202"
