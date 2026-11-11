#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/browser app/MainWindowSketch3d.cpp app/MainWindow.cpp app/MainWindow.hpp
# check-all sources: app/MainWindowVisibility.cpp app/MainWindowSelection.cpp app/framework/ModelShapes.cpp
# check-all sources: core/model-api/src/sketch3d.rs core/model/src/document/browser.rs tools/cli
# check-all sources: core/sketch/src/spatial.rs core/model/src/document/spatial_sketches.rs
# check-all sources: app/OcctViewer.cpp app/OcctViewer.hpp
# check-all sources: core/sketch/src/spatial_convert.rs core/model/src/document/sketch_conversion.rs
# check-all sources: core/sketch/src/spatial_edit.rs
# check-all sources: app/commands/CommandSupport.cpp app/commands/SweepCommands.cpp
# check-all sources: core/model/src/features/path.rs core/model/src/features/rib.rs
# 3D sketches in the shared Qt UI (mitcad#1524), on a design that
# mitcad-cli makes: Box1 (F1, F1.b0) 40 x 30 x 20 at (20, 20), off the origin
# planes (a face on one would take the plane's pick); 3DSketch1 (F2) on the
# box's top face with a route made through the sketch3d commands (a line
# from the origin along X, 40 long, a tangent arc, a fit-point spline
# leaving the face, its last end Free so that its tangent handle shows),
# a reference curve of the box's upright edge and a
# closed triangle of lines in the face's plane; Sketch1 (F3), planar,
# after it.
#   - The Browser lists 3DSketch1 under Sketches with its degrees of
#     freedom.
#   - Create Sketch shows Sketch type 2D (the default) and 3D before the
#     plane is picked; the banner names the kind; 2D makes a planar sketch on YZ
#     (finished empty, it goes again).
#   - With 3D chosen, a pick of XZ makes an empty 3D sketch: the banner
#     says 3D Sketch; Finish takes the empty new sketch back.
#   - Edit Sketch of 3DSketch1 (the Timeline's double-click, rolled back
#     before Sketch1): its frame follows the face (20 up, the support's
#     placement applied once), its points and curves are drawn and picked
#     by their ids, the 3D Sketch Palette describes the point (local and
#     component coordinates) and the reference curve with its source, the
#     hover says what is under the pointer, the 2D tools are
#     disabled with the reason (Extrude takes its in-plane profiles, mitcad#1550), Esc clears
#     the selection and stays in the sketch, Delete removes a triangle
#     line, Finish puts the Timeline back as one Edit step, which undo
#     takes back.
#   - The light bulb hides and shows 3DSketch1.
#   - The drawing tools (mitcad#1525) in a new 3D sketch on XZ: Line (L)
#     starts on its XY plane; a click on the origin snaps (Origin) and is
#     pending, nothing committed; Delta X 40 and an explicit Delta Z 0
#     typed (Tab between the fields), Enter commits AB with Delta Y left
#     free (null: one degree of freedom); Axis and Length along +Z, 25,
#     commits BC from the shared end; Esc takes back the field, then the
#     chain, then the tool, the committed segments staying; Undo takes BC
#     back in one step; a Fit Point Spline of three clicks ended by Enter;
#     a Tangent Arc continuing AB at B with its Radius and Arc Length
#     typed; a Center Point Arc whose normal is picked from AB; a Line
#     from the origin whose end snaps onto a side face of the box with the
#     On Face badge (mitcad#1549), the relation and the face's record in
#     the palette's lists; Finish.
#   - Editing (mitcad#1526) of 3DSketch1 again: AB's length typed 50 in
#     the palette's list (W2), made driven, removed (W3); B dragged along
#     X (one undo step) and a drag taken back by Esc; a Length added from
#     the palette, Fix and Unfix by Shift+F; the spline's tangent handle
#     dragged when it shows one (W17); Delete of B naming its curves, then
#     Delete with Dependents, undone; Replace Reference's pick ended by
#     Esc; Finish as one step.
#   - Converting (mitcad#1544): Sketch1's Browser menu offers Convert to
#     3D Sketch, which opens it and turns it into a 3D sketch in place,
#     fully constrained, its constraints and dimensions in plane with In
#     plane badges, the 3D Sketch Palette's 3D Sketch box checked and
#     disabled with why; Undo is one step back to the 2D editor; the Sketch
#     Palette's 3D Sketch box converts it again; Finish.
#   - Fillet (mitcad#1548) of 3DSketch1's triangle (F7, the test's key):
#     a click on the corner p12 picks it without a commit, the pointer
#     moved into the corner drags the radius and a click takes it as one
#     sketch3d.fillet; the corner p14 in the same run with Radius 2 typed
#     and Enter; Esc takes back the field, then the tool, the fillets
#     staying; Undo takes p14's back in one step; Finish as one step.
#   - Plane-bound tools (mitcad#1550) in a new 3D sketch on XY: the SKETCH
#     tab's 2-Point Rectangle (R) from the origin with Width 30 and Height
#     20 typed, one sketch3d.add_rectangle with the origin's attachment
#     in one undo step, fully constrained; undone and redone; Center
#     Diameter Circle (C) on the YZ plane around that corner with Radius 8;
#     Esc takes back the field, then the tool; Finish. In a second 3D
#     sketch on XY, through the command search: Ellipse from the origin
#     with Major Radius 10 and Minor Radius 5 typed (one
#     sketch3d.add_ellipse); Circumscribed Polygon around the origin with
#     Radius 20 and its 6 sides (one sketch3d.add_polygon, 9 curves with
#     its construction circles); Text in a frame dragged above the
#     origin, its preview logged while it is dragged, "MitCad" typed (one
#     sketch3d.add_text with the frame, its construction rectangle);
#     Finish.
#   - Rib from 3DSketch1's curves (mitcad#1550): a click on the spline
#     that leaves the face is refused, saying it does not lie in the
#     sketch plane; a click on the line in the plane is taken; Esc.
#   - The palette and the badges (mitcad#1592): every relation and
#     dimension button whole across the Docked card; a selected point's
#     description scrolled into the card's sight; the On Face badge inside
#     the view; one In plane badge per shape (the converted rectangle's six
#     kept relations and its corner's in-plane Fix, the polygon, the
#     text's frame), clear of its shape and of the other badges, inside
#     the view; the 3D Sketch box disabled as Text
#     starts and no longer waiting for it as it ends; Finish of the
#     converted sketch keeps the Home view it was seen from, not the 2D
#     editor's top view.
# The Windows test is tools/ui-windows-sketch3d-test.ps1, the macOS one
# tools/ui-macos-sketch3d-test.sh (tools/ui-scripts/sketch3d.mitcad-ui).
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sketch3d-test.sh [screenshot.png]

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}
ROOT=$(cd "$(dirname "$0")/.." && pwd)
CLI=${UI_CLI:-$(dirname "$UI_APP")/../tools/cli/mitcad-cli}
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-sketch3d.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT
FILE=$WORK/route.mitcad

cat > "$WORK/model.json" << 'EOF'
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": {"type": "origin", "plane": "xy"},
    "corner": [20, 20], "length": 40, "width": 30, "height": 20, "operation": "new_body"}},
  {"cmd": "sketch3d.create", "support": {"body": "F1.b0", "face": "F1:top"}},
  {"cmd": "sketch3d.add_line", "sketch": "F2", "start": [0, 0, 0], "end": [38, 1, 0], "axis": "x",
    "length": 40, "start_source": "origin"},
  {"cmd": "sketch3d.add_arc", "sketch": "F2", "mode": "tangent", "from": "c3", "start": "p2",
    "end": [50, 10, 0], "radius": 10},
  {"cmd": "sketch3d.add_spline", "sketch": "F2", "points": ["p5", [50, 30, 10], [50, 40, 25]],
    "ends": ["natural", "free"]},
  {"cmd": "sketch3d.include", "sketch": "F2", "source": {"body": "F1.b0", "edge": "E{F1:side0|F1:side1}"}},
  {"expect": {"query": {"query": "sketch3d", "uid": "F2"}, "result": {
    "curves": [{"id": "c3", "type": "line"}, {"id": "c6", "type": "arc"}, {"id": "c9", "type": "fitted_spline"},
               {"id": "c10", "type": "reference"}]}}},
  {"cmd": "sketch3d.add_line", "sketch": "F2", "start": [0, -20, 0], "end": [20, -20, 0]},
  {"cmd": "sketch3d.add_line", "sketch": "F2", "start": "p12", "end": [10, -35, 0]},
  {"cmd": "sketch3d.add_line", "sketch": "F2", "start": "p14", "end": "p11"},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F3", "corner": [60, 0], "width": 10, "height": 10},
  {"expect": {"feature": "3DSketch1", "status": "ok"}}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# The ids the commands gave out, from the saved design: the reference
# curve, the triangle's last line and one of its points.
read -r REF LINE < <(python3 - "$FILE" << 'EOF'
import json, sys
doc = json.load(open(sys.argv[1]))
sketch = next(f for f in doc["features"] if f.get("type") == "spatial_sketch")
curves = sketch["curves"]
ref = next(c["id"] for c in curves if c["type"] == "reference")
line = [c["id"] for c in curves if c["type"] == "line"][-1]
print(ref, line)
EOF
)
[ -n "$REF" ] && [ -n "$LINE" ] || ui_fail "no reference curve or line in the saved 3D sketch"
echo "ok   the reference curve is $REF, the triangle's last line $LINE"

# The In plane badges the view placed last since the mark (mitcad#1592),
# one line each: "3D sketch badge In plane k1,k2 at x,y; box x0,y0 to
# x1,y1; shape x0,y0 to x1,y1; view x0,y0 to x1,y1". The places are logged
# together, the frame's line first.
last_badges() {
  tail -n +$((UI_MARK + 1)) "$UI_LOG" | awk '
    /^3D sketch frame at /{n = 0}
    /^3D sketch badge In plane /{line[++n] = $0}
    END{for (i = 1; i <= n; i++) print line[i]}'
}
# badges_apart "description": each of those badges whole inside the view,
# clear of its shape (the text in a frame, a small polygon) and of every
# other badge.
badges_apart() {
  local badges problem
  badges=$(last_badges)
  [ -n "$badges" ] || ui_fail "$1: no In plane badge logged"
  problem=$(awk '{
      ids = $6
      rest = $0
      sub(/.* at /, "", rest)
      gsub(/[^0-9-]+/, " ", rest)
      split(rest, v, " ")
      # v: at x, y; box x0, y0, x1, y1; shape x0, y0, x1, y1; view x0, y0, x1, y1
      bx0[NR] = v[3]; by0[NR] = v[4]; bx1[NR] = v[5]; by1[NR] = v[6]; id[NR] = ids
      if (v[3] < v[11] || v[4] < v[12] || v[5] > v[13] || v[6] > v[14])
        print ids " is cut by the view"
      if (!(v[5] <= v[7] || v[3] >= v[9] || v[6] <= v[8] || v[4] >= v[10]))
        print ids " covers its shape"
    }
    END{
      for (i = 1; i <= NR; i++)
        for (j = i + 1; j <= NR; j++)
          if (!(bx1[i] <= bx0[j] || bx0[i] >= bx1[j] || by1[i] <= by0[j] || by0[i] >= by1[j]))
            print id[i] " and " id[j] " overlap"
    }' <<< "$badges")
  [ -z "$problem" ] || ui_fail "$1: $problem
$badges"
  echo "ok   $1"
}

ui_start_display
# Keys of the test's own for some tools without one (Fit Point Spline goes
# through the command search, which S opens).
mkdir -p "$UI_CONFIG/Mitcad"
printf '[shortcuts]\nsketch.arc_center=F8\nsketch.arc_tangent=F9\nsketch.fillet=F7\n' >> "$UI_CONFIG/Mitcad/Mitcad.conf"
ui_start_app --open "$FILE"
ui_step "fit (F6)"                       ui_key F6
ui_expect_log "Browser Root/Sketches/3DSketch1 at" "the Browser lists the 3D sketch under Sketches"
ui_expect_log "Browser sketch DOF: " "the Browser knows the sketches' freedoms"
grep -F "Browser sketch DOF: " "$UI_LOG" | tail -1 | grep -qE "3DSketch1 [0-9]+" ||
  ui_fail "no degrees of freedom of 3DSketch1 in the Browser: $(grep -F "Browser sketch DOF: " "$UI_LOG" | tail -1)"
echo "ok   3DSketch1's degrees of freedom in the Browser"
ui_expect_log "Sketches shown: " "the sketches are drawn"
grep -F "Sketches shown: " "$UI_LOG" | tail -1 | grep -qF "F2" || ui_fail "3DSketch1 is not drawn"
echo "ok   3DSketch1 is drawn"

echo "--- Create Sketch: the kind before the plane, 2D by default"
ui_mark
ui_step "create sketch (search)"         ui_command "Create Sketch"
ui_expect_new "Create Sketch kind: 2D" "2D is the default"
ui_expect_new "View banner: Create Sketch: pick a plane" "the banner asks for the plane"
ui_expect_new "Create Sketch kind 3D at" "the 3D choice is on the card"
ui_mark
ui_step "choose 3D"                      ui_click_logged "Create Sketch kind 3D"
ui_expect_new "Create Sketch kind: 3D" "3D chosen"
ui_expect_new "View banner: Create 3D Sketch: pick a plane" "the banner names the kind"
ui_mark
ui_step "choose 2D again"                ui_click_logged "Create Sketch kind 2D"
ui_expect_new "Create Sketch kind: 2D" "2D chosen again"
ui_mark
ui_step "cancel (Esc)"                   ui_key Escape
ui_expect_new "Create Sketch cancelled" "cancelled"
ui_mark
ui_step "create a 2D sketch on YZ"       ui_create_sketch yz
ui_expect_new "Sketch started on yz" "a planar sketch, as before"
ui_mark
ui_step "finish it empty (Ctrl+Enter)"   ui_key ctrl+Return
ui_expect_new "Sketch finished" "finished"

echo "--- Create Sketch with 3D: an empty new 3D sketch"
ui_mark
ui_step "create sketch (search)"         ui_command "Create Sketch"
ui_expect_new "Create Sketch kind 3D at" "the 3D choice is on the card"
ui_step "choose 3D"                      ui_click_logged "Create Sketch kind 3D"
ui_expect_new "Create Sketch kind: 3D" "3D chosen"
ui_sync
ui_step "pick XZ"                        ui_click_logged "Datum xz"
ui_expect_new "3D sketch started on xz" "a 3D sketch on XZ"
ui_expect_new "(3DSketch2): 0 points, 0 curves" "it is empty"
ui_expect_new "View banner: 3D Sketch 3DSketch2" "the banner says it is a 3D sketch"
ui_expect_new "3D sketch frame at " "its frame is drawn"
[ -n "$SHOT" ] && ui_capture "${SHOT%.png}-new.png"
ui_mark
ui_step "finish it empty (Ctrl+Enter)"   ui_key ctrl+Return
ui_expect_new "Sketch finished" "finished"
ui_expect_new "Browser sketch DOF: " "the Browser follows"
grep -F "Browser sketch DOF: " "$UI_LOG" | tail -1 | grep -qF "3DSketch2" &&
  ui_fail "the empty new 3D sketch stayed: $(grep -F "Browser sketch DOF: " "$UI_LOG" | tail -1)"
echo "ok   the empty new 3D sketch was taken back"

echo "--- Edit Sketch of 3DSketch1, rolled back before Sketch1"
ui_double_click_expect "Timeline 3DSketch1" "Editing 3D sketch F2 (3DSketch1)" "3DSketch1 is edited"
ui_expect_new "3D sketch F2 (3DSketch1): " "its status is logged"
grep -E "3D sketch frame in the design: origin \([-0-9.]+, [-0-9.]+, 20\.000000\)" "$UI_LOG" > /dev/null ||
  ui_fail "the frame is not on the top face: $(grep -F "3D sketch frame in the design" "$UI_LOG" | tail -1)"
echo "ok   the frame follows the top face, 20 up"
ui_expect_new "View banner: 3D Sketch 3DSketch1" "the banner says 3D Sketch"
ui_expect_new "sketch.slot: a 2D sketch tool does not apply to a 3D sketch" "the planar-only tools are disabled"
ui_expect_new "3D sketch entity F2/p2 at" "its points are drawn where a click picks them"
ui_expect_new "3D sketch entity F2/$REF at" "the reference curve is drawn"
ui_expect_new "3D sketch tangent handle " "the fit-point spline's free end shows its tangent handle"
ui_expect_new "3D sketch edit controls across the card: " "the relations and dimensions are laid out on the card"
grep -F "3D sketch edit controls across the card: " "$UI_LOG" | tail -1 | grep -qF ": all whole" ||
  ui_fail "a relation or dimension button is cut at the card's edge: $(grep -F "3D sketch edit controls across the card: " "$UI_LOG" | tail -1)"
echo "ok   every relation and dimension button is whole across the card (mitcad#1592)"
[ -n "$SHOT" ] && ui_capture "$SHOT"
ui_mark
ui_step "click the route's end p2"       ui_click_logged "3D sketch entity F2/p2"
ui_expect_new "Selected: 1 sketch point [sketch point p2 of F2]" "p2 selected by its id"
ui_expect_new "selected Point p2 at 40.00, 0.00, 0.00 mm" "the palette gives its local coordinates"
grep -F "3D Sketch Palette: 3DSketch1" "$UI_LOG" | tail -1 | grep -qF ", 20.00 mm in the component" ||
  ui_fail "the point is not 20 up in the component: $(grep -F "3D Sketch Palette" "$UI_LOG" | tail -1)"
echo "ok   and where the face puts it, once"
ui_sync # the card laid out again, the description scrolled into sight
ui_expect_new "3D sketch selection description " "the selection's description is on the card"
grep -F "3D sketch selection description " "$UI_LOG" | tail -1 | grep -qE " at [0-9]+,[0-9]+$" ||
  ui_fail "the selected point's description is out of the card's sight: $(grep -F "3D sketch selection description " "$UI_LOG" | tail -1)"
echo "ok   the description is in the card's sight (mitcad#1592)"
ui_expect_new "3D sketch hover: Point p2" "the hover says what is under the pointer"
ui_mark
ui_step "click the reference curve"      ui_click_logged "3D sketch entity F2/$REF"
ui_expect_new "Reference curve $REF: edge E{F1:side0|F1:side1} of F1.b0; included in 3D" \
  "the reference curve with its source"
ui_mark
ui_step "Esc (clear the selection)"      ui_key Escape
ui_expect_new "Selected: nothing" "Esc cleared the selection"
ui_step "Esc again"                      ui_key Escape
ui_sync
tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qF "Sketch finished" && ui_fail "Esc left the sketch"
echo "ok   Esc stays in the sketch"
ui_mark
ui_step "click the triangle's line"      ui_click_logged "3D sketch entity F2/$LINE"
ui_expect_new "[sketch curve $LINE of F2]" "the line selected"
ui_step "Delete"                         ui_key Delete
ui_expect_new "3D sketch F2: deleted" "the line deleted"
ui_mark
ui_step "finish (Ctrl+Enter)"            ui_key ctrl+Return
ui_expect_new "Sketch finished" "finished"
ui_mark
ui_step "undo"                           ui_key ctrl+z
ui_expect_new "Undo: Edit 3DSketch1" "the edit is one undo step, the Timeline put back"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"

echo "--- The light bulb"
ui_mark
ui_step "hide 3DSketch1"                 ui_click_logged "Browser eye Root/Sketches/3DSketch1"
ui_expect_new "Visibility Root/Sketches/3DSketch1: hidden" "hidden"
ui_expect_new "Sketches shown: " "the view follows"
grep -F "Sketches shown: " "$UI_LOG" | tail -1 | grep -qF "F2" && ui_fail "3DSketch1 is still drawn"
echo "ok   3DSketch1 not drawn"
ui_mark
ui_step "show 3DSketch1"                 ui_click_logged "Browser eye Root/Sketches/3DSketch1"
ui_expect_new "Visibility Root/Sketches/3DSketch1: shown" "shown again"

echo "--- The drawing tools (mitcad#1525): a Line chain from the origin by typed values"
ui_mark
ui_step "create sketch (search)"         ui_command "Create Sketch"
ui_expect_new "Create Sketch kind 3D at" "the 3D choice is on the card"
ui_step "choose 3D"                      ui_click_logged "Create Sketch kind 3D"
ui_expect_new "Create Sketch kind: 3D" "3D chosen"
ui_sync
ui_step "pick XZ"                        ui_click_logged "Datum xz"
ui_expect_new "3D sketch started on xz" "a new 3D sketch on XZ"
ui_expect_new "3D sketch frame at " "its frame is drawn"
ui_mark
ui_step "Line (L)"                       ui_key l
ui_expect_new "3D sketch tool Line: started; on the XY plane" "Line draws in the 3D sketch, on its XY plane"
ui_mark
ui_step "click the origin"               ui_click_logged "3D sketch frame"
ui_expect_new "3D sketch snap: Origin" "the origin snaps"
ui_expect_new "3D sketch Line: pending point 1 at (0.000, 0.000, 0.000) (Origin)" "the start is pending, at the origin"
grep -q "3D sketch command: " <(tail -n +$((UI_MARK + 1)) "$UI_LOG") && ui_fail "the first point committed something"
echo "ok   nothing committed for the first point"
ui_expect_new "3D sketch field dx at" "the Offsets fields are shown"
[ -n "$SHOT" ] && ui_capture "${SHOT%.png}-line-start.png"
ui_mark
ui_step "type Delta X 40"                ui_click_logged "3D sketch field dx"
xdotool type --delay 40 -- "40"
ui_expect_new "3D sketch field dx: 40" "Delta X typed: driving"
ui_step "Tab to Delta Y"                 ui_key Tab
ui_expect_new "3D sketch field focus: dy" "Tab reached Delta Y"
ui_step "Tab to Delta Z"                 ui_key Tab
ui_expect_new "3D sketch field focus: dz" "Tab reached Delta Z"
ui_step "type Delta Z 0"                 xdotool type --delay 40 -- "0"
ui_expect_new "3D sketch field dz: 0" "an explicit zero typed"
ui_mark
ui_step "Enter"                          ui_key Return
ui_expect_new "\"offsets\":[\"40" "the typed values go as offsets"
tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -F "3D sketch command: " | head -1 | grep -qF ',null,' ||
  ui_fail "Delta Y was not left free: $(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -F "3D sketch command: " | head -1)"
echo "ok   Delta Y untouched is no dimension"
ui_expect_new "3D sketch Line: made c3 from p1 to p2" "AB made, one step"
ui_expect_new "2 points, 1 curves, dof 1" "B free in Y only"
ui_mark
ui_step "Axis and Length"                ui_click_logged "3D sketch control Axis and Length"
ui_expect_new "3D sketch input: Axis and Length" "the input mode switched"
ui_expect_new "3D sketch control Length axis Z at" "its axis choice is shown"
ui_step "along Z"                        ui_click_logged "3D sketch control Length axis Z"
ui_expect_new "3D sketch input axis: +Z" "along +Z"
ui_expect_new "3D sketch field length at" "the Length field is shown"
ui_step "type Length 25"                 ui_click_logged "3D sketch field length"
xdotool type --delay 40 -- "25"
ui_expect_new "3D sketch field length: 25" "Length typed"
ui_mark
ui_step "Enter"                          ui_key Return
ui_expect_new "\"axis\":\"z\"" "the axis goes with it"
ui_expect_new "3D sketch Line: made c5 from p2 to p4" "BC made from the shared end"
ui_expect_new "3 points, 2 curves, dof 1" "C follows B's free Y only"
[ -n "$SHOT" ] && ui_capture "${SHOT%.png}-line-chain.png"

echo "--- Esc layers and Undo"
ui_mark
ui_step "Esc (the field)"                ui_key Escape
ui_expect_new "3D sketch Esc: field length back to its measurement" "the field's value goes first"
ui_step "Esc (the chain)"                ui_key Escape
ui_expect_new "3D sketch Esc: Line pending points dropped" "then the chain"
ui_expect_new "3D sketch Line: chain ended" "Line waits for a first point"
ui_step "Esc (the tool)"                 ui_key Escape
ui_expect_new "3D sketch tool Line: stopped" "then the tool"
tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qF "Sketch finished" && ui_fail "Esc left the sketch"
grep -F "3D sketch F" "$UI_LOG" | tail -1 | grep -qF "3 points, 2 curves" ||
  ui_fail "Esc removed committed geometry: $(grep -F "3D sketch F" "$UI_LOG" | tail -1)"
echo "ok   the committed segments stay"
ui_mark
ui_step "undo"                           ui_key ctrl+z
ui_expect_new "Undo: Add Line in 3DSketch" "BC taken back, one step"
ui_expect_new "2 points, 1 curves" "AB stays"

echo "--- A Fit Point Spline and a Tangent Arc"
ui_mark
ui_step "Fit Point Spline (search)"      ui_command "Fit Point Spline"
ui_expect_new "3D sketch tool Fit Point Spline: started" "the spline tool draws in the 3D sketch"
ui_step "click its first point"          ui_view_click 42 62
ui_expect_new "3D sketch Fit Point Spline: pending point 1" "a pending point"
ui_step "click its second point"         ui_view_click 50 55
ui_expect_new "3D sketch Fit Point Spline: pending point 2" "a second"
ui_step "click its third point"          ui_view_click 58 60
ui_expect_new "3D sketch Fit Point Spline: pending point 3" "a third"
ui_mark
ui_step "Enter (ends it)"                ui_key Return
ui_expect_new "3D sketch Fit Point Spline: made c" "the spline made at once"
ui_step "Esc (the tool)"                 ui_key Escape
ui_expect_new "3D sketch tool Fit Point Spline: stopped" "the tool ends"
ui_mark
ui_step "Tangent Arc (F9)"               ui_key F9
ui_expect_new "3D sketch tool Tangent Arc: started" "Tangent Arc draws in the 3D sketch"
ui_expect_log "3D sketch entity " "the sketch's points are drawn (logged when they changed, with the spline)"
ui_step "click B, AB's end"              ui_click_logged "3D sketch entity $(grep -oE "3D sketch entity F[0-9]+/p2 at" "$UI_LOG" | tail -1 | sed 's/^3D sketch entity //; s/ at$//')"
ui_expect_new "3D sketch Tangent Arc: continues c3 at p2" "it continues AB at B"
ui_expect_new "3D sketch field radius at" "its Radius field is shown"
ui_step "type Radius 10"                 ui_click_logged "3D sketch field radius"
xdotool type --delay 40 -- "10"
ui_expect_new "3D sketch field radius: 10" "Radius typed"
ui_step "Tab to Arc Length"              ui_key Tab
ui_expect_new "3D sketch field focus: arc_length" "Tab reached Arc Length"
xdotool type --delay 40 -- "15"
ui_expect_new "3D sketch field arc_length: 15" "Arc Length typed"
ui_mark
ui_step "Enter"                          ui_key Return
ui_expect_new "\"mode\":\"tangent\"" "a tangent arc from c3"
ui_expect_new "3D sketch Tangent Arc: made c" "the arc made, its radius and arc length driving"
ui_mark
ui_step "Esc (the field)"                ui_key Escape
ui_expect_new "3D sketch Esc: field arc_length back to its measurement" "the field goes first"
ui_step "Esc (the chain)"                ui_key Escape
ui_expect_new "3D sketch Esc: Tangent Arc pending points dropped" "then the chain"
ui_step "Esc (the tool)"                 ui_key Escape
ui_expect_new "3D sketch tool Tangent Arc: stopped" "the tool ends"

echo "--- Center Point Arc: its normal picked from the line AB"
ui_mark
ui_step "Center Point Arc (F8)"          ui_key F8
ui_expect_new "3D sketch tool Center Point Arc: started" "Center Point Arc draws in the 3D sketch"
ui_expect_new "3D sketch control Pick Normal at" "its Normal row is shown"
ui_step "Pick Normal"                    ui_click_logged "3D sketch control Pick Normal"
ui_expect_new "3D sketch Center Point Arc: picking the normal" "the next click picks the normal"
AB=$(grep -oE "3D sketch entity F[0-9]+/c3 at" "$UI_LOG" | tail -1 | sed 's/^3D sketch entity //; s/ at$//')
ui_step "click AB"                       ui_click_logged "3D sketch entity $AB"
ui_expect_new "3D sketch Center Point Arc: normal from line c3 (1.000, 0.0" "the arc's normal along AB"
grep -q "3D sketch command: " <(tail -n +$((UI_MARK + 1)) "$UI_LOG") && ui_fail "picking the normal committed something"
echo "ok   nothing committed by the pick"
ui_step "Esc (the tool)"                 ui_key Escape
ui_expect_new "3D sketch tool Center Point Arc: stopped" "the tool ends"

echo "--- A line end snapped onto a face of the box (mitcad#1549): On Face"
ui_mark
ui_step "Line (L)"                       ui_key l
ui_expect_new "3D sketch tool Line: started" "Line draws again"
ui_expect_new "Pick face F1.b0/F1:side" "the box's faces are logged where a click meets them"
FACE=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -oE "Pick face F1\.b0/F1:side[0-9]+ at" | head -1 |
  sed 's/^Pick face F1\.b0\///; s/ at$//')
[ -n "$FACE" ] || ui_fail "no side face of the box to snap to"
ui_mark
ui_step "click the origin"               ui_click_logged "3D sketch frame"
ui_expect_new "3D sketch Line: pending point 1 at (0.000, 0.000, 0.000) (Point: p1)" "the start is pending on AB's start, the sketch's point before the origin under it"
ui_mark
ui_step "click the face $FACE"           ui_click_pick "face F1.b0/$FACE"
ui_expect_new "3D sketch snap: On Face: face $FACE of F1.b0" "the face snaps, its badge names it"
ui_expect_new "3D sketch snap badge On Face: face $FACE of F1.b0 at " "the badge is placed"
tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -F "3D sketch snap badge On Face: " | tail -1 | grep -qF ", inside the view " ||
  ui_fail "the On Face badge is cut by the view: $(grep -F "3D sketch snap badge On Face: " "$UI_LOG" | tail -1)"
echo "ok   the On Face badge is whole inside the view (mitcad#1592)"
ui_expect_new "\"end_source\":{\"body\":\"F1.b0\",\"face\":\"$FACE\"}" "the end goes onto the face"
ui_expect_new "3D sketch Line: made c" "the line made, one step"
ui_mark
ui_step "Esc (the chain)"                ui_key Escape
ui_expect_new "3D sketch Line: chain ended" "the chain ends"
ui_step "Esc (the tool)"                 ui_key Escape
ui_expect_new "3D sketch tool Line: stopped" "the tool ends"
ui_expect_new " On Face p" "the On Face relation is in the list"
ui_expect_new "face $FACE of F1.b0; body F1.b0; its point slides on the face" "the face's record is listed"
[ -n "$SHOT" ] && ui_capture "${SHOT%.png}-on-face.png"
ui_mark
ui_step "finish (Ctrl+Enter)"            ui_key ctrl+Return
ui_expect_new "Sketch finished" "finished"

echo "--- 3D sketch editing (mitcad#1526): 3DSketch1's dimensions, relations, drags and references"
ui_double_click_expect "Timeline 3DSketch1" "Editing 3D sketch F2 (3DSketch1)" "3DSketch1 is edited again"
ui_expect_new "3D sketch item k3 Length c3: 40.00 mm" "the list gives AB's length, driving"
ui_expect_new "3D sketch item k3 field " "its value field is on the card (in sight or below it)"
ui_expect_new "3D sketch edit control coincident " "the relations are on the card"
[ -n "$SHOT" ] && ui_capture "${SHOT%.png}-edit.png"
ui_mark
ui_step "drag along X"                   ui_click_logged "3D sketch edit control drag axis X"
ui_expect_new "3D sketch drag on: along X" "drags go along X"
ui_sync # the card settled: its rows logged again where they are now
ui_mark
ui_step "AB's length to 50 (W2)"         ui_click_logged "3D sketch item k3 field"
ui_key ctrl+a
xdotool type --delay 40 -- "50"
ui_key Return
ui_expect_new "3D sketch edit value k3 = 50: done" "the value set, one step"
ui_expect_new "3D sketch item k3 Length c3: 50.00 mm" "AB is 50 long"
ui_expect_new "3D sketch entity F2/p2 at" "B moved in the view"
ui_expect_new "3D sketch item k3 remove " "the list laid out again"
ui_mark
ui_step "k3 driven"                      ui_click_logged "3D sketch item k3 driven"
ui_expect_new "3D sketch edit driven k3: done" "k3 only measures"
ui_expect_new "3D sketch item k3 Length c3: 50.00 mm; driven" "the list says driven"
ui_expect_new "3D sketch item k3 remove " "the list laid out again"
ui_mark
ui_step "remove k3 (W3)"                 ui_click_logged "3D sketch item k3 remove"
ui_expect_new "3D sketch edit remove k3: done" "the length removed"
grep -F "3D sketch item k3 " <(tail -n +$((UI_MARK + 1)) "$UI_LOG") | grep -qv "remove" &&
  ui_fail "k3 is still listed"
echo "ok   AB's length gone: B is free along X"

echo "--- Dragging B along X (chosen above), then a drag taken back by Esc"
ui_sync
ui_mark
ui_step "drag B"                         ui_drag_from "3D sketch entity F2/p2" 60 0
ui_expect_new "3D sketch drag: started p2 along X" "the drag started on B"
ui_expect_new "3D sketch drag preview: " "it was previewed"
ui_expect_new "3D sketch drag: p2 taken; p2 at" "taken on release"
grep -F "3D sketch drag: p2 taken; p2 at (50.000" <(tail -n +$((UI_MARK + 1)) "$UI_LOG") &&
  ui_fail "B did not move from its 50"
echo "ok   B moved along X"
ui_mark
ui_step "undo the drag"                  ui_key ctrl+z
ui_expect_new "Undo: Drag in 3DSketch1" "one undo step"
ui_mark
read -r DX DY <<< "$(ui_logged_at "3D sketch entity F2/p2")"
ui_apart "$DX" "$DY"
ui_step "press B and move"               xdotool mousemove "$DX" "$DY" mousedown 1
for i in 1 2 3 4 5; do
  xdotool mousemove $((DX + 8 * i)) "$DY"
  sleep 0.08
done
ui_sync
ui_step "Esc while dragging"             ui_key Escape
xdotool mouseup 1
ui_expect_new "3D sketch drag: cancelled, nothing changed" "Esc took the drag back"
ui_sync
tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qF "3D sketch drag: p2 taken" && ui_fail "the cancelled drag was taken"
echo "ok   no step from the cancelled drag"

echo "--- A Length from the palette, Fix and Unfix by the SKETCH tab's key"
ui_mark
ui_step "click AB"                       ui_click_logged "3D sketch entity F2/c3"
ui_expect_new "[sketch curve c3 of F2]" "AB selected"
ui_sync # the card settled after the list changed
ui_step "Length"                       ui_click_logged "3D sketch edit control length"
ui_expect_new "3D sketch edit length: done, made k" "a true length on AB"
ui_mark
ui_step "click A"                        ui_click_logged "3D sketch entity F2/p1"
ui_expect_new "[sketch point p1 of F2]" "A selected"
ui_step "Fix (Shift+F)"                  ui_key shift+f
ui_expect_new "3D sketch edit fix: done" "A fixed"
ui_step "Unfix (Shift+F again)"          ui_key shift+f
ui_expect_new "3D sketch edit unfix: done" "A's Fix removed, its attachment stays"

HANDLE=$(grep -oE "3D sketch tangent handle c[0-9]+/p[0-9]+" "$UI_LOG" | tail -1)
if [ -n "$HANDLE" ]; then
  echo "--- The spline's tangent handle (W17)"
  ui_mark
  ui_step "drag the handle"              ui_drag_from "$HANDLE" 0 -40
  ui_expect_new "3D sketch drag: started handle" "the handle's drag"
  ui_expect_new "3D sketch drag: handle c" "taken: the end tangent changed"
  ui_step "undo"                         ui_key ctrl+z
  ui_expect_new "Undo: Drag in 3DSketch1" "one step"
fi

echo "--- Delete with Dependents, a Replace Reference pick ended by Esc"
ui_mark
ui_step "click B"                        ui_click_logged "3D sketch entity F2/p2"
ui_step "Delete"                         ui_key Delete
ui_expect_new "3D sketch delete: p2 is used by c3" "the curves that use B are named first"
tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qF "3D sketch F2: deleted" && ui_fail "B was deleted at once"
ui_expect_new "3D sketch edit control delete dependents " "Delete with Dependents offered"
ui_step "Delete with Dependents"         ui_click_logged "3D sketch edit control delete dependents"
ui_expect_new "3D sketch F2: deleted [" "B deleted with its curves"
ui_mark
ui_step "undo"                           ui_key ctrl+z
ui_expect_new "Undo: Delete in 3DSketch1" "one undo step"
ui_sync # the card laid out again without the Delete notice
ui_mark
ui_step "Replace on the reference curve" ui_click_logged "3D sketch reference $REF replace"
ui_expect_new "3D sketch replace $REF: pick a source" "the next click picks its source"
ui_step "Esc"                            ui_key Escape
ui_expect_new "3D sketch source pick: ended" "Esc ends the pick, nothing changed"
tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qF "3D sketch edit replace" && ui_fail "a reference was replaced"
ui_mark
ui_step "finish (Ctrl+Enter)"            ui_key ctrl+Return
ui_expect_new "Sketch finished" "finished"
ui_mark
ui_step "undo"                           ui_key ctrl+z
ui_expect_new "Undo: Edit 3DSketch1" "the edit is one step, the Timeline put back"

echo "--- Sketch1 turned into a 3D sketch in place (mitcad#1544)"
ui_mark
ui_step "right-click Sketch1's row"      ui_click_logged "Browser Root/Sketches/Sketch1" 3
ui_expect_new "Context menu: " "the row's menu"
grep -F "Context menu: " "$UI_LOG" | tail -1 | grep -qF "Convert to 3D Sketch" ||
  ui_fail "no Convert to 3D Sketch in Sketch1's menu: $(grep -F "Context menu: " "$UI_LOG" | tail -1)"
echo "ok   Convert to 3D Sketch offered"
ui_step "Convert to 3D Sketch"           ui_menu_choose "Convert to 3D Sketch"
ui_expect_new "Editing sketch F3 (Sketch1)" "the sketch is opened first"
ui_expect_new "Converted Sketch1 (F3) to a 3D sketch: dof 0, in plane k1, k2, k3, k4, k5, k6" \
  "converted, fully constrained, its constraints and dimensions kept in plane"
ui_expect_new "View banner: 3D Sketch Sketch1" "the 3D editor takes it, under its own name"
ui_expect_new "3D Sketch Palette 3D Sketch at" "the palette's 3D Sketch box"
grep -F "3D Sketch Palette 3D Sketch at" "$UI_LOG" | tail -1 |
  grep -qF "checked yes enabled yes: Uncheck to turn it back into a 2D sketch." ||
  ui_fail "the 3D Sketch box is not checked and enabled with why: $(grep -F "3D Sketch Palette 3D Sketch at" "$UI_LOG" | tail -1)"
echo "ok   checked, and enabled to turn it back (mitcad#1550)"
ui_expect_new "3D sketch badge In plane k" "the kept relations carry In plane badges"
ui_sync # the places logged where the converted sketch is drawn
# Seven: its six kept relations (in plane k1-k6) and the in-plane Fix
# the conversion made for its fixed corner (its `fixes`).
[[ $(last_badges) =~ ^3D\ sketch\ badge\ In\ plane\ k[0-9]+(,k[0-9]+){6}\ at\  ]] &&
  [ "$(last_badges | wc -l)" = 1 ] ||
  ui_fail "not one In plane badge for the rectangle's six kept relations and its corner's in-plane Fix: $(last_badges)"
echo "ok   one In plane badge for the rectangle, its six kept relations and the in-plane Fix of its fixed corner (mitcad#1592)"
badges_apart "the badge clear of the rectangle, inside the view"
[ -n "$SHOT" ] && ui_capture "${SHOT%.png}-converted.png"
ui_mark
ui_step "undo (Ctrl+Z)"                  ui_key ctrl+z
ui_expect_new "Undo: Convert Sketch1 to 3D Sketch" "one undo step"
ui_expect_new "Sketch editor follows the history: Sketch1 is a 2D sketch" "the 2D editor again"
ui_expect_new "Palette palette3dSketch at" "the Sketch Palette's 3D Sketch box"
[ -n "$SHOT" ] && ui_capture "${SHOT%.png}-before-conversion.png"
ui_mark
ui_step "check 3D Sketch"                ui_click_logged "Palette palette3dSketch"
ui_expect_new "Palette: 3D Sketch checked for F3" "the box asks for the conversion"
ui_expect_new "Converted Sketch1 (F3) to a 3D sketch: dof 0" "converted again"
ui_expect_new "3D Sketch Palette 3D Sketch at" "the 3D Sketch Palette's box placed afresh"

echo "--- Sketch1 back to 2D by unchecking 3D Sketch (mitcad#1550)"
ui_mark
ui_step "uncheck 3D Sketch"              ui_click_logged "3D Sketch Palette 3D Sketch"
ui_expect_new "Converted Sketch1 (F3) to a 2D sketch: dof 0" \
  "back to 2D, fully constrained, in place"
ui_expect_new "Palette palette3dSketch at" "the 2D editor takes it again"
[ -n "$SHOT" ] && ui_capture "${SHOT%.png}-back-to-2d.png"
ui_mark
ui_step "undo (Ctrl+Z)"                  ui_key ctrl+z
ui_expect_new "Undo: Convert Sketch1 to 2D Sketch" "one undo step"
ui_expect_new "Sketch editor follows the history: Sketch1 is a 3D sketch" "the 3D editor again"
# The 3D sketch seen from the Home view; Finish keeps it, not the top view
# the 2D editor turned to (mitcad#1592).
ui_mark
ui_step "the Home view"                  ui_click_logged "View home"
ui_expect_new "View home" "the Home view"
ui_sync
ui_mark
ui_step "finish (Ctrl+Enter)"            ui_key ctrl+Return
ui_expect_new "Sketch finished" "finished"
ui_expect_new "Sketch left with the 3D sketch's view: the view looks along (" "the 3D sketch's view is kept"
grep -F "Sketch left with " "$UI_LOG" | tail -1 | grep -qF "along (0.000, 0.000, -1.000)" &&
  ui_fail "the 2D sketch's top view came back: $(grep -F "Sketch left with " "$UI_LOG" | tail -1)"
echo "ok   the Home view stays after Finish (mitcad#1592)"
ui_mark
ui_step "right-click Sketch1's row"      ui_click_logged "Browser Root/Sketches/Sketch1" 3
ui_expect_new "Context menu: " "the row's menu"
grep -F "Context menu: " "$UI_LOG" | tail -1 | grep -qF "Convert to 2D Sketch" ||
  ui_fail "no Convert to 2D Sketch in the 3D sketch's menu: $(grep -F "Context menu: " "$UI_LOG" | tail -1)"
echo "ok   Convert to 2D Sketch offered"
ui_step "Esc (the menu)"                 ui_key Escape
ui_sync
grep -F "Browser sketch DOF: " "$UI_LOG" | tail -1 | grep -qE "(: |, )Sketch1 0(,|$)" ||
  ui_fail "Sketch1 is not fully constrained in the Browser: $(grep -F "Browser sketch DOF: " "$UI_LOG" | tail -1)"
echo "ok   Sketch1 fully constrained as a 3D sketch"

echo "--- Fillet (mitcad#1548): the triangle's corners, a dragged radius and a typed one"
ui_double_click_expect "Timeline 3DSketch1" "Editing 3D sketch F2 (3DSketch1)" "3DSketch1 is edited for the fillets"
ui_expect_new "3D sketch entity F2/p12 at" "the triangle's corner p12 is drawn"
ui_expect_new "3D sketch entity F2/p14 at" "and its corner p14"
ui_mark
ui_step "Sketch Fillet (F7)"             ui_key F7
ui_expect_new "3D sketch tool Fillet: started" "Fillet runs in the 3D sketch"
ui_expect_new "3D sketch field radius at" "its Radius field is shown"
ui_mark
ui_step "click the corner p12"           ui_click_logged "3D sketch entity F2/p12"
ui_expect_new "3D sketch Fillet: corner p12 of c13 and c15 picked" "the corner picked by its point"
ui_expect_new "3D sketch Fillet: drag or type the radius at p12" "the radius follows the pointer"
grep -q "3D sketch command: " <(tail -n +$((UI_MARK + 1)) "$UI_LOG") && ui_fail "picking the corner committed something"
echo "ok   nothing committed by the pick"
# Into the corner: a third of the way toward the middle of the other two
# corners on the screen, near the corner's bisector.
read -r BX BY <<< "$(ui_logged_at "3D sketch entity F2/p12")"
read -r AX AY <<< "$(ui_logged_at "3D sketch entity F2/p11")"
read -r CX CY <<< "$(ui_logged_at "3D sketch entity F2/p14")"
TX=$((BX + ((AX + CX) / 2 - BX) * 3 / 10))
TY=$((BY + ((AY + CY) / 2 - BY) * 3 / 10))
ui_step "drag the radius into the corner" xdotool mousemove "$TX" "$TY"
ui_sync
[ -n "$SHOT" ] && ui_capture "${SHOT%.png}-fillet.png"
ui_mark
ui_step "click (takes the radius)"       xdotool click 1
ui_expect_new "\"cmd\":\"sketch3d.fillet\"" "one sketch3d.fillet"
ui_expect_new "\"points\":[\"p12\"]" "the corner by its point"
ui_expect_new " at p12, radius " "p12 rounded at the dragged radius, one step"
ui_mark
ui_step "click the corner p14"           ui_click_logged "3D sketch entity F2/p14"
ui_expect_new "3D sketch Fillet: corner p14 of c15 and c16 picked" "the next corner in the same run"
ui_step "type Radius 2"                  ui_click_logged "3D sketch field radius"
xdotool type --delay 40 -- "2"
ui_expect_new "3D sketch field radius: 2" "Radius typed"
ui_mark
ui_step "Enter"                          ui_key Return
ui_expect_new "\"radius\":\"2" "the typed radius goes as typed"
ui_expect_new " at p14, radius " "p14 rounded at the typed radius"
ui_mark
ui_step "Esc (the field)"                ui_key Escape
ui_expect_new "3D sketch Esc: field radius back to its measurement" "the field's value goes first"
ui_step "Esc (the tool)"                 ui_key Escape
ui_expect_new "3D sketch tool Fillet: stopped" "then the tool"
tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qF "Sketch finished" && ui_fail "Esc left the sketch"
echo "ok   the fillets stay"
ui_mark
ui_step "undo"                           ui_key ctrl+z
ui_expect_new "Undo: Fillet in 3DSketch1" "p14's fillet taken back, one step"
ui_mark
ui_step "finish (Ctrl+Enter)"            ui_key ctrl+Return
ui_expect_new "Sketch finished" "finished"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
ui_mark
ui_step "undo"                           ui_key ctrl+z
ui_expect_new "Undo: Edit 3DSketch1" "the edit is one step, the Timeline put back"

echo "--- Plane-bound tools (mitcad#1550): a rectangle on XY from the origin, a circle on YZ"
ui_mark
ui_step "create sketch (search)"         ui_command "Create Sketch"
ui_expect_new "Create Sketch kind 3D at" "the 3D choice is on the card"
ui_step "choose 3D"                      ui_click_logged "Create Sketch kind 3D"
ui_expect_new "Create Sketch kind: 3D" "3D chosen"
ui_sync
ui_step "pick XY"                        ui_click_logged "Datum xy"
ui_expect_new "3D sketch started on xy" "a new 3D sketch on XY"
ui_expect_new "3D sketch frame at " "its frame is drawn"
ui_mark
ui_step "2-Point Rectangle (R)"          ui_key r
ui_expect_new "3D sketch tool 2-Point Rectangle: started; on the XY plane" "the SKETCH tab's rectangle draws in the 3D sketch"
ui_mark
ui_step "click the origin"               ui_click_logged "3D sketch frame"
ui_expect_new "3D sketch 2-Point Rectangle: pending point 1 at (0.000, 0.000, 0.000) (Origin)" "the first corner at the origin"
grep -q "3D sketch command: " <(tail -n +$((UI_MARK + 1)) "$UI_LOG") && ui_fail "the first corner committed something"
echo "ok   nothing committed for the first corner"
ui_expect_new "3D sketch field width at" "the Width field is shown"
ui_step "type Width 30"                  ui_click_logged "3D sketch field width"
xdotool type --delay 40 -- "30"
ui_expect_new "3D sketch field width: 30" "Width typed"
ui_step "Tab to Height"                  ui_key Tab
ui_expect_new "3D sketch field focus: height" "Tab reached Height"
ui_step "type Height 20"                 xdotool type --delay 40 -- "20"
ui_expect_new "3D sketch field height: 20" "Height typed"
ui_mark
ui_step "Enter"                          ui_key Return
ui_expect_new "\"cmd\":\"sketch3d.add_rectangle\"" "one sketch3d.add_rectangle"
ui_expect_new "\"plane\":\"xy\"" "on the active plane"
ui_expect_new "\"width\":\"30" "the typed width drives"
ui_expect_new "\"cmd\":\"sketch3d.attach\"" "the origin snap attaches its corner in the same step"
ui_expect_new "3D sketch 2-Point Rectangle: made c" "the rectangle made"
ui_expect_new "4 points, 4 curves, dof 0" "fully constrained by the origin, Width and Height"
[ -n "$SHOT" ] && ui_capture "${SHOT%.png}-rectangle.png"
# The rectangle took the typed values: Esc ends the tool, or first takes
# back a value the field still holds.
ui_mark
ui_step "Esc"                            ui_key Escape
ui_sync
if ! tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qF "3D sketch tool 2-Point Rectangle: stopped"; then
  ui_expect_new "3D sketch Esc: field height back to its measurement" "the field goes first"
  ui_step "Esc (the tool)"               ui_key Escape
fi
ui_expect_new "3D sketch tool 2-Point Rectangle: stopped" "the tool ends"
ui_mark
ui_step "undo"                           ui_key ctrl+z
ui_expect_new "Undo: Add Rectangle in 3DSketch" "the rectangle is one undo step"
ui_step "redo"                           ui_key ctrl+y
ui_expect_new "Redo: Add Rectangle in 3DSketch" "and back"
ui_mark
ui_step "Center Diameter Circle (C)"     ui_key c
ui_expect_new "3D sketch tool Center Diameter Circle: started" "the circle draws in the 3D sketch"
ui_expect_new "3D sketch control Plane YZ at" "the plane choice is shown"
ui_step "the YZ plane"                   ui_click_logged "3D sketch control Plane YZ"
ui_expect_new "3D sketch placement: on the YZ plane" "the YZ plane through the anchor"
ui_mark
ui_step "click the corner at the origin" ui_click_logged "3D sketch frame"
ui_expect_new "3D sketch Center Diameter Circle: pending point 1 at (0.000, 0.000, 0.000)" "its centre on the corner"
ui_expect_new "3D sketch field radius at" "the Radius field is shown"
ui_step "type Radius 8"                  ui_click_logged "3D sketch field radius"
xdotool type --delay 40 -- "8"
ui_expect_new "3D sketch field radius: 8" "Radius typed"
ui_mark
ui_step "Enter"                          ui_key Return
ui_expect_new "\"cmd\":\"sketch3d.add_circle\"" "one sketch3d.add_circle"
ui_expect_new "\"plane\":\"yz\"" "bound to the YZ plane"
ui_expect_new "3D sketch Center Diameter Circle: made c" "the circle made on YZ"
ui_expect_new "4 points, 5 curves, dof 0" "its centre shared, its radius typed"
[ -n "$SHOT" ] && ui_capture "${SHOT%.png}-circle.png"
# The circle took the typed radius: Esc ends the tool, or first takes back
# a value the field still holds.
ui_mark
ui_step "Esc"                            ui_key Escape
ui_sync
if ! tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qF "3D sketch tool Center Diameter Circle: stopped"; then
  ui_expect_new "3D sketch Esc: field radius back to its measurement" "the field goes first"
  ui_step "Esc (the tool)"               ui_key Escape
fi
ui_expect_new "3D sketch tool Center Diameter Circle: stopped" "the tool ends"
ui_mark
ui_step "finish (Ctrl+Enter)"            ui_key ctrl+Return
ui_expect_new "Sketch finished" "finished"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"

echo "--- Rib from 3DSketch1's curves (mitcad#1550): in its sketch plane taken, out of it refused"
ui_mark
ui_step "rib (search)"                   ui_command "Rib"
ui_expect_new "Sketch entity F2/c9 at" "3DSketch1's curves are where a click picks them"
ui_expect_new "Sketch entity F2/c3 at" "the line AB too"
ui_mark
ui_step "the spline leaving the face"    ui_click_logged "Sketch entity F2/c9"
ui_expect_new "not taken: This curve of 3DSketch1 does not lie in its sketch plane" \
  "a curve out of the sketch plane is refused, saying why"
tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qF "Rib Curves: 1 sketch curve" && ui_fail "the spline was taken"
echo "ok   nothing taken"
ui_mark
ui_step "the line AB in the plane"       ui_click_logged "Sketch entity F2/c3"
ui_expect_new "Rib Curves: 1 sketch curve [sketch curve c3 of F2]" "a curve in the sketch plane is taken"
[ -n "$SHOT" ] && ui_capture "${SHOT%.png}-rib.png"
ui_mark
ui_step "cancel (Esc)"                   ui_key Escape
ui_expect_new "Command Rib cancelled" "nothing added"

echo "--- Plane-bound tools (mitcad#1550): an ellipse, a polygon and a text in a dragged frame on XY"
# The pointer to a place of the view off the origin by dx, dy (logical
# pixels, left of the first sketch's rectangle and the box): a typed size
# goes toward the pointer, which a click on a field took out of the view.
off_origin() {
  local fx fy
  read -r fx fy <<< "$(ui_logged_at "3D sketch frame")"
  xdotool mousemove $((fx + $1 * UI_SCALE)) $((fy + $2 * UI_SCALE))
  ui_sync
}
# Esc until the tool ends: the field that has the keyboard goes back first.
end_tool() {
  ui_mark
  ui_step "Esc"                          ui_key Escape
  ui_sync
  grep -qF "3D sketch tool $1: stopped" <(tail -n +$((UI_MARK + 1)) "$UI_LOG") ||
    ui_step "Esc (the tool)"             ui_key Escape
  ui_expect_new "3D sketch tool $1: stopped" "the $1 tool ends"
}
ui_mark
ui_step "create sketch (search)"         ui_command "Create Sketch"
ui_expect_new "Create Sketch kind 3D at" "the 3D choice is on the card"
ui_step "choose 3D"                      ui_click_logged "Create Sketch kind 3D"
ui_expect_new "Create Sketch kind: 3D" "3D chosen"
ui_sync
ui_step "pick XY"                        ui_click_logged "Datum xy"
ui_expect_new "3D sketch started on xy" "another 3D sketch on XY"
ui_expect_new "3D sketch frame at " "its frame is drawn"

ui_mark
ui_step "Ellipse (search)"               ui_command "Ellipse"
ui_expect_new "3D sketch tool Ellipse: started; on the XY plane" "the SKETCH tab's Ellipse draws in the 3D sketch"
ui_mark
ui_step "click the origin"               ui_click_logged "3D sketch frame"
ui_expect_new "3D sketch Ellipse: pending point 1 at (0.000, 0.000, 0.000)" "the center at the origin"
ui_expect_new "3D sketch field major at" "the Major Radius field is shown"
ui_step "type Major Radius 10"           ui_click_logged "3D sketch field major"
off_origin -150 20
xdotool type --delay 40 -- "10"
ui_expect_new "3D sketch field major: 10" "Major Radius typed"
ui_mark
ui_step "Enter"                          ui_key Return
ui_expect_new "3D sketch Ellipse: pending point 2 at" "the major axis' end 10 from the center"
grep -q "3D sketch command: " <(tail -n +$((UI_MARK + 1)) "$UI_LOG") && ui_fail "the major axis committed something"
echo "ok   nothing committed before the minor radius"
ui_expect_new "3D sketch field minor at" "the Minor Radius field is shown"
ui_step "type Minor Radius 5"            ui_click_logged "3D sketch field minor"
off_origin -120 60
xdotool type --delay 40 -- "5"
ui_expect_new "3D sketch field minor: 5" "Minor Radius typed"
ui_mark
ui_step "Enter"                          ui_key Return
ui_expect_new "\"cmd\":\"sketch3d.add_ellipse\"" "one sketch3d.add_ellipse"
ui_expect_new "\"plane\":\"xy\"" "on the active plane"
ui_expect_new "\"major_value\":\"10" "the typed major radius drives"
ui_expect_new "\"minor_value\":\"5" "the typed minor radius drives"
ui_expect_new "3D sketch Ellipse: made c" "the ellipse made"
ui_expect_new " points, 1 curves, dof " "the ellipse is the sketch's curve"
[ -n "$SHOT" ] && ui_capture "${SHOT%.png}-ellipse.png"
end_tool "Ellipse"

ui_mark
ui_step "Circumscribed Polygon (search)" ui_command "Circumscribed Polygon"
ui_expect_new "3D sketch tool Circumscribed Polygon: started; on the XY plane" "the polygon draws in the 3D sketch"
ui_mark
ui_step "click the origin"               ui_click_logged "3D sketch frame"
ui_expect_new "3D sketch Circumscribed Polygon: pending point 1 at (0.000, 0.000, 0.000)" "its center at the origin"
ui_expect_new "3D sketch field radius at" "the Radius field is shown"
ui_step "type Radius 20"                 ui_click_logged "3D sketch field radius"
off_origin -100 40
xdotool type --delay 40 -- "20"
ui_expect_new "3D sketch field radius: 20" "Radius typed"
ui_mark
ui_step "Enter"                          ui_key Return
ui_expect_new "\"cmd\":\"sketch3d.add_polygon\"" "one sketch3d.add_polygon"
ui_expect_new "\"sides\":6" "six sides, the default"
ui_expect_new "\"radius\":\"20" "the typed radius drives"
ui_expect_new "3D sketch Circumscribed Polygon: made c" "the polygon made"
ui_expect_new " points, 9 curves, dof " "six sides and two construction circles beside the ellipse"
[ -n "$SHOT" ] && ui_capture "${SHOT%.png}-polygon.png"
end_tool "Circumscribed Polygon"

ui_mark
ui_step "Text (search)"                  ui_command "Text"
ui_expect_new "3D sketch tool Text: started; on the XY plane" "the Text tool draws in the 3D sketch"
ui_expect_new "3D Sketch Palette 3D Sketch box: enabled no: Finish or Escape the active tool first." \
  "the 3D Sketch box disabled as the tool starts, not at the sketch's next change (mitcad#1592)"
ui_mark
read -r tx ty <<< "$(ui_logged_at "3D sketch frame")"
# Above the origin, clear of the polygon and of the box; 220 to its left
# was over the Browser, which took the press.
tx=$((tx - 60 * UI_SCALE)); ty=$((ty - 150 * UI_SCALE))
ui_step "drag a frame"                   xdotool mousemove "$tx" "$ty" mousedown 1
for i in 1 2 3 4 5 6 7 8; do
  xdotool mousemove $((tx + 10 * i * UI_SCALE)) $((ty + 5 * i * UI_SCALE))
  sleep 0.08
done
ui_expect_new "3D sketch Text: frame preview from (" "the frame is previewed while it is dragged"
[ -n "$SHOT" ] && ui_capture "${SHOT%.png}-text-frame.png"
ui_sync
xdotool mouseup 1
ui_expect_new "3D sketch Text: frame to (" "the release ends the frame"
ui_expect_new "3D sketch Text: pending point 1 at" "the text starts at the press"
grep -q "3D sketch command: " <(tail -n +$((UI_MARK + 1)) "$UI_LOG") && ui_fail "the frame committed something"
echo "ok   nothing committed before the text is typed"
ui_expect_log "3D sketch field text at" "the Text field is shown (since the tool started)"
ui_sync # the card laid out again for the typing step (its places logged anew)
ui_step "type the text"                  ui_click_logged "3D sketch field text"
xdotool type --delay 40 -- "MitCad"
ui_expect_new "3D sketch field text: MitCad" "the text typed"
ui_mark
ui_step "Enter"                          ui_key Return
ui_expect_new "\"cmd\":\"sketch3d.add_text\"" "one sketch3d.add_text"
ui_expect_new "\"frame\":[" "with its dragged frame"
ui_expect_new "\"text\":\"MitCad\"" "the text typed"
ui_expect_new "3D sketch Text: made t" "the text made"
ui_expect_new " points, 13 curves, dof " "its frame a construction rectangle"
ui_sync # the places logged with the text
# One In plane badge per shape (the ellipse's and the polygon's, the text's
# frame), clear of the text and of each other (mitcad#1592: one per
# relation stacked over the polygon and over the text).
[ "$(last_badges | wc -l)" -le 3 ] ||
  ui_fail "more In plane badges than shapes:
$(last_badges)"
echo "ok   one In plane badge per shape"
badges_apart "the polygon's and the text frame's badges apart, off the text, inside the view"
[ -n "$SHOT" ] && ui_capture "${SHOT%.png}-text.png"
ui_mark
end_tool "Text"
ui_expect_new "3D Sketch Palette 3D Sketch box: enabled " "the 3D Sketch box follows the tool's end"
grep -F "3D Sketch Palette 3D Sketch box: " "$UI_LOG" | tail -1 | grep -qF "active tool" &&
  ui_fail "the 3D Sketch box still waits for the ended tool: $(grep -F "3D Sketch Palette 3D Sketch box: " "$UI_LOG" | tail -1)"
echo "ok   the 3D Sketch box no longer waits for a tool (mitcad#1592)"
ui_mark
ui_step "finish (Ctrl+Enter)"            ui_key ctrl+Return
ui_expect_new "Sketch finished" "finished"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"

ui_crashed && ui_fail "the application crashed"
ui_finish "UI 3D sketches"
