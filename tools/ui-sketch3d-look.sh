#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Pictures of 3D sketches in the shared Qt UI (mitcad#1550, item 8): a
# one-off visual check, not a recurring test (its name is not
# ui-*-test.sh, so no check picks it up). It drives the app as
# tools/ui-sketch3d-test.sh and tools/ui-sketch3d-202-test.sh do, waits for
# the same log lines, and saves one picture of the screen per state; a
# step whose log line does not come ends it with "FAIL: <step>: ..." and a
# non-zero status. Five starts of the app:
#
# 1. The 3D Sketch Palette, in the Floating layout (the default) and the
#    Docked one, on the design of tools/ui-sketch3d-test.sh (Box1, and
#    3DSketch1 on its top face: a line, a tangent arc, a fit-point spline
#    leaving the face, a reference to the box's upright edge, a triangle in
#    the face's plane), 3DSketch1 opened from the Timeline:
#      <dir>/palette-floating.png         the palette as the sketch opens
#      <dir>/palette-floating-line.png    with the Line tool's rows
#      <dir>/palette-docked.png
#      <dir>/palette-docked-point.png     the point p2 selected and described
#      <dir>/palette-docked-line.png
# 2. Each drawing tool in a new 3D sketch on XY of an empty design, its
#    preview drawn with the pointer where the next click goes (Docked, the
#    layout the UI tests click in; tools without a key of their own run by
#    the command search or by the script's keys F5 Point, F8 3-Point Arc):
#      <dir>/tool-point.png      Point, the pointer on the origin (its snap)
#      <dir>/tool-line.png       Line from the origin
#      <dir>/tool-arc.png        3-Point Arc after two points
#      <dir>/tool-spline.png     Fit Point Spline after three points
#      <dir>/tool-rectangle.png  2-Point Rectangle after its first corner
#      <dir>/tool-circle.png     Center Diameter Circle after its center
#      <dir>/tool-ellipse.png    Ellipse after its center and major axis
#      <dir>/tool-polygon.png    Circumscribed Polygon after its center
#      <dir>/tool-text-frame.png Text, its frame being dragged
#      <dir>/tool-text.png       "MitCad", Text Height 10, made in its frame
#      <dir>/tools-finished.png  the sketch finished, Home view
# 3. Editing, On Face and Rib on the design of 1 (Docked):
#      <dir>/dimension-edit.png  AB's length typed 50 in the palette's list
#      <dir>/dimension-done.png  the value taken, B moved
#      <dir>/on-face-snap.png    a Line's end on a side face of the box: the
#                                On Face badge before the click
#      <dir>/on-face.png         the line made, its On Face relation and the
#                                face's record listed
#      <dir>/rib-refused.png     Rib: the spline leaving the face refused
#      <dir>/rib.png             Rib: the line in the sketch plane taken
# 4. The GitHub mitcad/Mitcad#202 design (tools/ui-scripts/sketch3d-202.json:
#    Box1, and the component Post with a taller box; Docked): a 2D sketch on
#    Box1's top face turned into a 3D sketch by the 3D Sketch box, back,
#    and again; a route from a corner of its rectangle to a top corner of
#    Post's box; a Pipe along it:
#      <dir>/convert-2d.png      the 2D sketch on the face, the box unchecked
#      <dir>/convert-3d.png      3D Sketch checked: the 3D editor, in place
#      <dir>/convert-back-2d.png unchecked: the 2D editor again
#      <dir>/pipe-route.png      the line AB off the face and the tangent arc
#                                onto Post's vertex
#      <dir>/pipe-preview.png    Pipe's panel with the route, previewed
#      <dir>/pipe.png            Pipe1 added
# The designs are <dir>/route.mitcad and <dir>/route-202.mitcad, the app's
# logs <dir>/<start>.log (palette-floating, palette-docked, tools, editing,
# route-202).
#
# What to look for: the palette's sections readable and not cut off in both
# layouts; each tool's preview where the pointer is, its typed fields in
# the palette; badges (Origin, On Face, In plane) next to their places; the
# 3D Sketch box checked in the 3D editor and unchecked in the 2D one; the
# pipe following the route from the rectangle's corner to Post's vertex.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sketch3d-look.sh <dir>

source "$(dirname "$0")/ui-test-lib.sh"

DIR=${1:?usage: tools/ui-sketch3d-look.sh <dir>}
mkdir -p "$DIR"
DIR=$(cd "$DIR" && pwd)
ROOT=$(cd "$(dirname "$0")/.." && pwd)
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
RUN=look
trap 'cp -f "$UI_LOG" "$DIR/$RUN.log" 2> /dev/null; ui_cleanup' EXIT
FILE=$DIR/route.mitcad
FILE202=$DIR/route-202.mitcad

# The design of tools/ui-sketch3d-test.sh: Box1 (F1, F1.b0) 40 x 30 x 20 at
# (20, 20); 3DSketch1 (F2) on its top face: the line c3 from the origin
# along X, 40 long (its length k3), the tangent arc c6, the fit-point
# spline c9 leaving the face, the reference curve c10 of the box's upright
# edge, a closed triangle of lines; Sketch1 (F3), planar, after it.
cat > "$DIR/route.json" << 'EOF'
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": {"type": "origin", "plane": "xy"},
    "corner": [20, 20], "length": 40, "width": 30, "height": 20, "operation": "new_body"}},
  {"cmd": "sketch3d.create", "support": {"body": "F1.b0", "face": "F1:top"}},
  {"cmd": "sketch3d.add_line", "sketch": "F2", "start": [0, 0, 0], "end": [38, 1, 0], "axis": "x",
    "length": 40, "start_source": "origin"},
  {"cmd": "sketch3d.add_arc", "sketch": "F2", "mode": "tangent", "from": "c3", "start": "p2",
    "end": [50, 10, 0], "radius": 10},
  {"cmd": "sketch3d.add_spline", "sketch": "F2", "points": ["p5", [50, 30, 10], [50, 40, 25]]},
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
"$CLI" run "$DIR/route.json" --save "$FILE" > "$DIR/cli.log" 2>&1 ||
  { cat "$DIR/cli.log"; ui_fail "mitcad-cli (the 3D sketch design)"; }
"$CLI" run "$ROOT/tools/ui-scripts/sketch3d-202.json" --save "$FILE202" > "$DIR/cli-202.log" 2>&1 ||
  { cat "$DIR/cli-202.log"; ui_fail "mitcad-cli (the #202 design)"; }

# shot name: the mouse off the window (no hover, no tooltip), the screen
# saved.
shot() {
  xdotool mousemove 5000 5000
  ui_sync
  sleep 1
  ui_capture "$DIR/$1.png"
  echo "ok   $DIR/$1.png"
}

# shot_at name x% y%: the pointer at that place of the 3D view (a tool's
# preview follows it), the screen saved.
shot_at() {
  xdotool mousemove $(ui_view_at "$2" "$3")
  ui_sync
  sleep 1
  ui_sync
  ui_capture "$DIR/$1.png"
  echo "ok   $DIR/$1.png"
}

# shot_here name: the screen saved with the pointer where it is (a snap
# badge under it, a frame being dragged).
shot_here() {
  ui_sync
  sleep 1
  ui_capture "$DIR/$1.png"
  echo "ok   $DIR/$1.png"
}

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

# tool "Tool" step command...: starts a 3D sketch tool (a key or the
# command search) and waits for it.
tool() {
  local name=$1 step=$2
  shift 2
  ui_mark
  ui_step "$step"                          "$@"
  ui_expect_new "3D sketch tool $name: started" "$name draws in the 3D sketch"
}

# view_place "Tool" n x% y%: a click at that place of the 3D view, logged
# as the tool's pending point n.
view_place() {
  ui_mark
  ui_step "$1: point $2"                   ui_view_click "$3" "$4"
  ui_expect_new "3D sketch $1: pending point $2 at" "$1's point $2 pending"
}

# view_commit "Tool" x% y%: the last click of a tool, which makes its
# curve.
view_commit() {
  ui_mark
  ui_step "$1: the last point"             ui_view_click "$2" "$3"
  ui_expect_new "3D sketch $1: made " "$1 made"
}

# new_3d_sketch plane: Create Sketch with 3D chosen, a pick of the origin
# plane.
new_3d_sketch() {
  ui_mark
  ui_step "create sketch (search)"         ui_command "Create Sketch"
  ui_expect_new "Create Sketch kind 3D at" "the 3D choice is on the card"
  ui_step "choose 3D"                      ui_click_logged "Create Sketch kind 3D"
  ui_expect_new "Create Sketch kind: 3D" "3D chosen"
  ui_sync
  ui_step "pick ${1^^}"                    ui_click_logged "Datum $1"
  ui_expect_new "3D sketch started on $1" "a new 3D sketch on ${1^^}"
  ui_expect_new "3D sketch frame at " "its frame is drawn"
}

finish_sketch() {
  ui_mark
  ui_step "finish (Ctrl+Enter)"            ui_key ctrl+Return
  ui_expect_new "Sketch finished" "finished"
  grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
  return 0
}

# edit_3dsketch1: 3DSketch1 opened from the Timeline (rolled back before
# Sketch1).
edit_3dsketch1() {
  ui_double_click_expect "Timeline 3DSketch1" "Editing 3D sketch F2 (3DSketch1)" "3DSketch1 is edited"
  ui_expect_new "3D Sketch Palette: 3DSketch1 | " "the 3D Sketch Palette describes it"
  ui_expect_new "3D sketch entity F2/p2 at" "its points are drawn where a click picks them"
}

end_start() {
  ui_crashed && ui_fail "the application crashed"
  cp -f "$UI_LOG" "$DIR/$RUN.log"
  ui_stop_app
}

ui_start_display
# Keys of the script's own for the tools without one (S is the command
# search) and the Home view.
mkdir -p "$UI_CONFIG/Mitcad"
printf '[shortcuts]\nsketch.point=F5\nsketch.arc=F8\nsketch.arc_tangent=F9\nview.home=F11\n' \
  >> "$UI_CONFIG/Mitcad/Mitcad.conf"

echo "=== 1. The 3D Sketch Palette, Floating and Docked"
for layout in floating docked; do
  RUN=palette-$layout
  ui_start_app --open "$FILE" --chrome="$layout"
  ui_step "fit (F6)"                       ui_key F6
  ui_expect_log "Timeline 3DSketch1 at" "$layout: the Timeline lists 3DSketch1"
  edit_3dsketch1
  shot "palette-$layout"
  if [ "$layout" = docked ]; then
    ui_mark
    ui_step "click the route's end p2"     ui_click_logged "3D sketch entity F2/p2"
    ui_expect_new "Selected: 1 sketch point [sketch point p2 of F2]" "p2 selected by its id"
    ui_expect_new "selected Point p2 at 40.00, 0.00, 0.00 mm" "the palette describes it"
    shot "palette-$layout-point"
    ui_mark
    ui_step "Esc (clear the selection)"    ui_key Escape
    ui_expect_new "Selected: nothing" "the selection cleared"
  fi
  tool Line "Line (L)"                     ui_key l
  shot "palette-$layout-line"
  end_start
done

echo "=== 2. The drawing tools in a new 3D sketch on XY"
RUN=tools
ui_start_app
new_3d_sketch xy

echo "--- Point"
tool Point "Point (F5)"                    ui_key F5
ui_mark
ui_step "the pointer on the origin"        xdotool mousemove $(ui_logged_at "3D sketch frame")
ui_expect_new "3D sketch snap: Origin" "the origin snaps under the pointer"
shot_here tool-point
ui_mark
ui_step "Point: a click"                   ui_view_click 45 22
ui_expect_new "3D sketch Point: made p" "a point made"
esc_tool Point

echo "--- Line"
tool Line "Line (L)"                       ui_key l
ui_mark
ui_step "Line: the origin"                 ui_click_logged "3D sketch frame"
ui_expect_new "3D sketch Line: pending point 1 at (0.000, 0.000, 0.000)" "the start pending at the origin"
shot_at tool-line 66 44
view_commit Line 66 44
esc_tool Line

echo "--- 3-Point Arc"
tool "3-Point Arc" "3-Point Arc (F8)"      ui_key F8
view_place "3-Point Arc" 1 15 62
view_place "3-Point Arc" 2 33 62
shot_at tool-arc 24 52
view_commit "3-Point Arc" 24 52
esc_tool "3-Point Arc"

echo "--- Fit Point Spline"
tool "Fit Point Spline" "Fit Point Spline (search)" ui_command "Fit Point Spline"
view_place "Fit Point Spline" 1 50 70
view_place "Fit Point Spline" 2 60 62
view_place "Fit Point Spline" 3 70 70
shot_at tool-spline 80 62
ui_mark
ui_step "Enter (ends it)"                  ui_key Return
ui_expect_new "3D sketch Fit Point Spline: made c" "the spline made"
esc_tool "Fit Point Spline"

echo "--- 2-Point Rectangle, on the sketch's XY plane"
tool "2-Point Rectangle" "2-Point Rectangle (R)" ui_key r
view_place "2-Point Rectangle" 1 12 18
shot_at tool-rectangle 16 40
view_commit "2-Point Rectangle" 16 40
esc_tool "2-Point Rectangle"

echo "--- Center Diameter Circle"
tool "Center Diameter Circle" "Center Diameter Circle (C)" ui_key c
view_place "Center Diameter Circle" 1 60 26
shot_at tool-circle 66 26
view_commit "Center Diameter Circle" 66 26
esc_tool "Center Diameter Circle"

echo "--- Ellipse"
tool Ellipse "Ellipse (search)"            ui_command "Ellipse"
view_place Ellipse 1 32 80
view_place Ellipse 2 42 80
shot_at tool-ellipse 32 74
view_commit Ellipse 32 74
esc_tool Ellipse

echo "--- Circumscribed Polygon"
tool "Circumscribed Polygon" "Circumscribed Polygon (search)" ui_command "Circumscribed Polygon"
view_place "Circumscribed Polygon" 1 75 82
shot_at tool-polygon 85 82
view_commit "Circumscribed Polygon" 85 82
esc_tool "Circumscribed Polygon"

echo "--- Text in a dragged frame"
tool Text "Text (search)"                  ui_command "Text"
ui_mark
read -r tx ty <<< "$(ui_view_at 10 66)"
read -r ux uy <<< "$(ui_view_at 12 86)"
ui_apart "$tx" "$ty"
ui_step "press where the frame starts"     xdotool mousemove "$tx" "$ty" mousedown 1
for i in 1 2 3 4 5 6 7 8; do
  xdotool mousemove $((tx + (ux - tx) * i / 8)) $((ty + (uy - ty) * i / 8))
  sleep 0.08
done
ui_expect_new "3D sketch Text: frame preview from (" "the frame is previewed while it is dragged"
shot_here tool-text-frame
xdotool mouseup 1
ui_expect_new "3D sketch Text: frame to (" "the release ends the frame"
ui_expect_new "3D sketch Text: pending point 1 at" "the text starts at the press"
ui_expect_log "3D sketch field text at" "the Text field is shown (since the tool started)"
ui_sync # the card laid out again (its places logged anew)
ui_step "the Text field"                   ui_click_logged "3D sketch field text"
xdotool type --delay 40 -- "MitCad"
ui_expect_new "3D sketch field text: MitCad" "the text typed"
# A text height that shows in the picture (5 mm is a few pixels here).
ui_step "Tab to Text Height"               ui_key Tab
ui_expect_new "3D sketch field focus: text_height" "Tab reached Text Height"
ui_step "type Text Height 10"              xdotool type --delay 40 -- "10"
ui_expect_new "3D sketch field text_height: 10" "Text Height typed"
ui_mark
ui_step "Enter"                            ui_key Return
ui_expect_new "\"cmd\":\"sketch3d.add_text\"" "one sketch3d.add_text"
ui_expect_new "3D sketch Text: made t" "the text made"
shot tool-text
esc_tool Text

finish_sketch
ui_mark
ui_step "Home (F11)"                       ui_key F11
ui_expect_new "View home" "the Home view"
ui_step "fit (F6)"                         ui_key F6
shot tools-finished
end_start

echo "=== 3. Editing, On Face and Rib"
RUN=editing
ui_start_app --open "$FILE"
ui_step "fit (F6)"                         ui_key F6
ui_expect_log "Timeline 3DSketch1 at" "the Timeline lists 3DSketch1"

echo "--- A dimension being edited: AB's length in the palette's list"
edit_3dsketch1
ui_expect_new "3D sketch item k3 Length c3: 40.00 mm" "the list gives AB's length, driving"
ui_expect_new "3D sketch item k3 field " "its value field is on the card (in sight or below it)"
ui_sync # the card settled: its rows logged again where they are now
# The next row's field into sight too, so that k3's row shows whole.
ui_sketch3d_reveal "3D sketch item k5 field"
ui_mark
ui_step "AB's length field"                ui_click_logged "3D sketch item k3 field"
ui_key ctrl+a
xdotool type --delay 40 -- "50"
shot dimension-edit
ui_step "Enter"                            ui_key Return
ui_expect_new "3D sketch edit value k3 = 50: done" "the value set, one step"
ui_expect_new "3D sketch item k3 Length c3: 50.00 mm" "AB is 50 long"
shot dimension-done
finish_sketch

echo "--- A line's end on a side face of the box: On Face"
new_3d_sketch xz
tool Line "Line (L)"                       ui_key l
ui_expect_new "Pick face F1.b0/F1:side" "the box's faces are logged where a click meets them"
FACE=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -oE "Pick face F1\.b0/F1:side[0-9]+ at" | head -1 |
  sed 's/^Pick face F1\.b0\///; s/ at$//')
[ -n "$FACE" ] || ui_fail "no side face of the box to snap to"
ui_mark
ui_step "Line: the origin"                 ui_click_logged "3D sketch frame"
ui_expect_new "3D sketch Line: pending point 1 at (0.000, 0.000, 0.000)" "the start is pending at the origin"
ui_mark
ui_step "the pointer on the face $FACE"    xdotool mousemove $(ui_pick_at "face F1.b0/$FACE")
ui_expect_new "3D sketch snap: On Face: face $FACE of F1.b0" "the face snaps, its badge names it"
shot_here on-face-snap
ui_mark
ui_step "click the face"                   xdotool click 1
ui_expect_new "\"end_source\":{\"body\":\"F1.b0\",\"face\":\"$FACE\"}" "the end goes onto the face"
ui_expect_new "3D sketch Line: made c" "the line made"
esc_tool Line
ui_expect_log " On Face p" "the On Face relation is in the list"
ui_expect_log "face $FACE of F1.b0; body F1.b0; its point slides on the face" "the face's record is listed"
shot on-face
finish_sketch

echo "--- Rib from 3DSketch1's curves"
ui_mark
ui_step "rib (search)"                     ui_command "Rib"
ui_expect_new "Sketch entity F2/c9 at" "3DSketch1's curves are where a click picks them"
ui_expect_new "Sketch entity F2/c3 at" "the line AB too"
ui_mark
ui_step "the spline leaving the face"      ui_click_logged "Sketch entity F2/c9"
ui_expect_new "not taken: This curve of 3DSketch1 does not lie in its sketch plane" "refused, saying why"
shot rib-refused
ui_mark
ui_step "the line AB in the plane"         ui_click_logged "Sketch entity F2/c3"
ui_expect_new "Rib Curves: 1 sketch curve [sketch curve c3 of F2]" "taken"
shot rib
ui_mark
ui_step "cancel (Esc)"                     ui_key Escape
ui_expect_new "Command Rib cancelled" "nothing added"
end_start

echo "=== 4. The #202 design: conversion and back, a route, a Pipe along it"
RUN=route-202
ui_start_app --open "$FILE202"
ui_step "fit (F6)"                         ui_key F6
ui_expect_log "Bodies shown: " "the bodies are shown"

echo "--- A 2D sketch on Box1's top face, a rectangle on it"
ui_mark
ui_step "create sketch (search)"           ui_command "Create Sketch"
ui_expect_new "Create Sketch kind: 2D" "2D is the default"
ui_expect_new "Pick face F1.b0/F1:top at" "Box1's top face can be picked"
ui_sync
ui_step "pick the top face"                ui_click_pick "face F1.b0/F1:top at"
ui_expect_new 'Sketch started on {"body":"F1.b0","face":"F1:top"}' "a 2D sketch on the face"
ui_expect_new "Palette palette3dSketch at" "the Sketch Palette's 3D Sketch box"
ui_sync
ui_mark
ui_step "2-Point Rectangle (R)"            ui_key r
ui_step "first corner"                     ui_sketch_click 25 25
ui_step "opposite corner"                  ui_sketch_click 45 40
ui_expect_new "Added rectangle " "a rectangle on the face"
ui_step "end the tool (Esc)"               ui_key Escape
shot convert-2d

echo "--- 3D Sketch checked, unchecked, checked again"
ui_mark
ui_step "check 3D Sketch"                  ui_click_logged "Palette palette3dSketch"
ui_expect_new "Palette: 3D Sketch checked for F3" "the box asks for the conversion"
ui_expect_new "Converted Sketch1 (F3) to a 3D sketch: dof " "converted in place"
ui_expect_new "View banner: 3D Sketch Sketch1" "the 3D editor takes it"
ui_expect_new "3D Sketch Palette 3D Sketch at" "the 3D Sketch Palette's box"
shot convert-3d
ui_sync # the card settled
ui_mark
ui_step "uncheck 3D Sketch"                ui_click_logged "3D Sketch Palette 3D Sketch"
ui_expect_new "Converted Sketch1 (F3) to a 2D sketch: dof " "back to 2D, in place"
ui_expect_new "Palette palette3dSketch at" "the 2D editor takes it again"
shot convert-back-2d
ui_sync # the palette settled
ui_mark
ui_step "check 3D Sketch again"            ui_click_logged "Palette palette3dSketch"
ui_expect_new "Palette: 3D Sketch checked for F3" "the box asks for the conversion"
ui_expect_new "Converted Sketch1 (F3) to a 3D sketch: dof " "a 3D sketch again"
CONVERTED=$UI_MARK
ui_expect_new "3D sketch entity F3/p" "the rectangle's corners are drawn where a click picks them"
A=$(tail -n +$((CONVERTED + 1)) "$UI_LOG" | sed -n 's/^3D sketch entity F3\/\(p[0-9]*\) at .*/\1/p' | head -1)
[ -n "$A" ] || ui_fail "no corner of the rectangle logged"
echo "ok   the route starts at the corner $A"

echo "--- The route: the line AB off the face, a tangent arc onto Post's vertex"
ui_mark
ui_step "Home (F11)"                       ui_key F11
ui_expect_new "View home" "the Home view"
ui_step "fit (F6)"                         ui_key F6
ui_expect_new "3D sketch entity F3/$A at" "the corner logged again where it is now"
ui_sync
tool Line "Line (L)"                       ui_key l
ui_mark
ui_step "Line: the corner $A"              ui_click_logged "3D sketch entity F3/$A"
ui_expect_new "3D sketch Line: pending point 1 at (" "the start is pending at the corner"
ui_expect_new "3D sketch field dx at" "the Offsets fields are shown"
ui_step "Delta X"                          ui_click_logged "3D sketch field dx"
ui_step "type 10"                          xdotool type --delay 40 -- "10"
ui_expect_new "3D sketch field dx: 10" "Delta X typed"
ui_step "Tab to Delta Y"                   ui_key Tab
ui_expect_new "3D sketch field focus: dy" "Tab reached Delta Y"
ui_step "type 0"                           xdotool type --delay 40 -- "0"
ui_expect_new "3D sketch field dy: 0" "Delta Y typed"
ui_step "Tab to Delta Z"                   ui_key Tab
ui_expect_new "3D sketch field focus: dz" "Tab reached Delta Z"
ui_step "type 15"                          xdotool type --delay 40 -- "15"
ui_expect_new "3D sketch field dz: 15" "Delta Z typed"
ui_mark
ui_step "Enter"                            ui_key Return
ui_expect_new "3D sketch Line: made c" "AB made"
read -r L FROM B <<< "$(last_new 's/^3D sketch Line: made \(c[0-9]*\) from \(p[0-9]*\) to \(p[0-9]*\)$/\1 \2 \3/p')"
[ -n "$L" ] && [ -n "$B" ] || ui_fail "the line AB's ids not logged"
ui_expect_new "3D sketch entity F3/$B at" "B is drawn where a click picks it"
esc_tool Line
tool "Tangent Arc" "Tangent Arc (F9)"      ui_key F9
ui_expect_new "Pick vertex F2.b0/V{" "Post's vertices logged where a click meets them"
ui_sync
START=$(grep -n "^Pick places:" "$UI_LOG" | tail -1 | cut -d: -f1)
VNAME=$(tail -n +"${START:-1}" "$UI_LOG" | sed -n 's/^Pick vertex F2\.b0\/\(V{[^ ]*F2:top[^ ]*}\) at [0-9]*,[0-9]*$/\1/p' | head -1)
[ -n "$VNAME" ] || ui_fail "no top corner of Post's box to snap to"
ui_mark
ui_step "click B, AB's end"                ui_click_logged "3D sketch entity F3/$B"
ui_expect_new "3D sketch Tangent Arc: continues $L at $B" "the arc continues AB at B"
ui_mark
ui_step "click the corner of Post"         ui_click_pick "vertex F2.b0/$VNAME at"
ui_expect_new "3D sketch snap: Linked Point: vertex $VNAME of F2.b0 in Post at (" "Post's vertex snaps"
ui_expect_new "3D sketch Tangent Arc: made c" "the arc made"
read -r R FROM V <<< "$(last_new 's/^3D sketch Tangent Arc: made \(c[0-9]*\) from \(p[0-9]*\) to \(p[0-9]*\)$/\1 \2 \3/p')"
[ -n "$R" ] || ui_fail "the arc's id not logged"
esc_tool "Tangent Arc"
shot pipe-route
finish_sketch

echo "--- Pipe along AB and the arc"
# Finish puts back the camera the 2D sketch started with (the top view):
# the Home view again, so that the pipe is seen leaving the face.
ui_mark
ui_step "Home (F11)"                       ui_key F11
ui_expect_new "View home" "the Home view"
ui_step "fit (F6)"                         ui_key F6
ui_mark
ui_step "pipe (search)"                    ui_command "Pipe"
ui_expect_new "Panel Pipe input chain at" "the Pipe panel"
ui_expect_new "Sketch entity F3/$L at" "the route's curves are where a click picks them"
ui_sync
ui_mark
ui_step "Chain Selection off"              ui_click_logged "Panel Pipe input chain"
ui_expect_new "Pipe: Chain Selection = off" "the picks are taken as they are"
ui_sync
ui_mark
ui_step "pick AB"                          ui_click_logged "Sketch entity F3/$L"
ui_expect_new "Pipe Path listed: $L ($A to $B)" "AB from A"
ui_sync
ui_mark
ui_step "pick the arc"                     ui_click_logged "Sketch entity F3/$R"
ui_expect_new "Pipe Path listed: $L, $R ($A to $V)" "the route from the corner to the vertex"
ui_expect_new "Preview Pipe: ok" "previewed"
shot pipe-preview
ui_mark
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Added pipe" "Pipe1 added"
ui_mark
ui_step "save (Ctrl+S)"                    ui_key ctrl+s
ui_expect_new "Saved $FILE202" "saved"
shot pipe
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
end_start

ui_finish "3D sketch pictures in $DIR"
