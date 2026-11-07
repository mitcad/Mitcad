#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/drawings app/browser app/MainWindow.cpp core/model/src/drawing core/model-drawing/src tools/cli
# check-all sources: core/model-api/src/drawings.rs core/model-api/src/drawing_annotations.rs core/model-api/src/drawing_tables.rs
# The drawing features of mitcad#729 through the real UI, in the Docked
# layout, on a 60 x 60 x 10 plate with four 6 mm holes on a circle of 20
# about its middle that mitcad-cli makes:
#   - File › Create Drawing (ISO, A3) and a top view at 1:1 at 150, 150
#     (the plate's corner at 120, 120 on the sheet).
#   - The drawing's Decimals (1) and Text Height (5 mm) in the sheet's
#     panel; a dimension of the plate's width then reads 60.0; its own text
#     height in its panel; its Replace Value (mitcad#1292): SEE NOTE drawn
#     in place of the value, the measured 60.0 beside the field, undone
#     and redone, then the field emptied: 60.0 again.
#   - Center Pattern: one hole clicked, every hole of the pattern marked.
#   - Edge Extension of one edge; a Line in the view (its pen Extra
#     Thick, mitcad#1402), an Arc on the sheet
#     and a Text in the view; the view dragged: the line drawn in it moves
#     with it.
#   - The view's Rotation in its panel (90), then Rotate 90° Right from its
#     menu, undone and redone.
#   - A section and a detail on the view, then its Orientation changed: no
#     longer refused, the three views still drawn.
#   - Its Scale typed (mitcad#1169): abc refused and not sent, 2:3 sent,
#     the model's scale 0.667 and the field showing it as 1:1.5 (1:X or X:1).
#   - A right click is the tool's Esc (mitcad#1404): two Lines on the sheet
#     and a third's first end; a right drag keeps it, the first right click
#     drops it and keeps the tool, the second ends the tool, neither opens
#     a menu; with no tool a right click opens the sheet's menu.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-drawing-features-test.sh [screenshot.png]

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-drawing-features.XXXXXX)
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
R='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'
FILE="$WORK/plate.mitcad"

# The plate (F1, F2) and its holes, one sketch and cut each (F3 … F10).
cat > "$WORK/plate.json" << EOF
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 60, "height": 60},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$R"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_circle", "sketch": "F3", "center": [50, 30], "diameter": 6},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F3", "region": "r{c1}"}],
    "extent": {"type": "through_all"}, "operation": "cut"}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_circle", "sketch": "F5", "center": [30, 50], "diameter": 6},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F5", "region": "r{c1}"}],
    "extent": {"type": "through_all"}, "operation": "cut"}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_circle", "sketch": "F7", "center": [10, 30], "diameter": 6},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F7", "region": "r{c1}"}],
    "extent": {"type": "through_all"}, "operation": "cut"}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_circle", "sketch": "F9", "center": [30, 10], "diameter": 6},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F9", "region": "r{c1}"}],
    "extent": {"type": "through_all"}, "operation": "cut"}}
]
EOF
"$CLI" run "$WORK/plate.json" --save "$FILE" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# sheet_at x y: the screen position of a sheet point (mm), from the last
# "Drawing sheet view x0,y0 x1,y1 x2,y2" (where the sheet points (0, 0),
# (100, 0) and (0, 100) are in the window).
sheet_at() {
  local view
  view=$(grep -o "Drawing sheet view [-0-9,]* [-0-9,]* [-0-9,]*" "$UI_LOG" | tail -1)
  [ -n "$view" ] || ui_fail "the app did not log the drawing's sheet"
  awk -v view="$view" -v x="$1" -v y="$2" -v wx="$X" -v wy="$Y" -v s="$UI_SCALE" 'BEGIN {
    split(view, part, " "); split(part[4], o, ","); split(part[5], a, ","); split(part[6], b, ",")
    px = o[1] + (a[1] - o[1]) * x / 100 + (b[1] - o[1]) * y / 100
    py = o[2] + (a[2] - o[2]) * x / 100 + (b[2] - o[2]) * y / 100
    printf "%d %d\n", wx + px * s + 0.5, wy + py * s + 0.5 }'
}
# sheet_click x y [button]: a click at a sheet point (mm), apart in time
# from a click just before near it (ui_apart: Qt would make the two a
# double-click).
sheet_click() {
  local x y
  ui_sync
  read -r x y <<< "$(sheet_at "$1" "$2")"
  ui_apart "$x" "$y"
  xdotool mousemove "$x" "$y" click "${3:-1}"
  ui_sync
}

# click_apart "logged place" [button]: ui_click_logged, apart in time from
# a click just before near it (the dimension's text lies where its placing
# click was: a click on it at once was a double-click).
click_apart() {
  local x y
  read -r x y <<< "$(ui_logged_at "$1")"
  ui_apart "$x" "$y"
  xdotool mousemove "$x" "$y" click "${2:-1}"
}

# sheet_drag fx fy tx ty: a left drag from one sheet point to another (mm).
sheet_drag() {
  local fx fy tx ty i
  read -r fx fy <<< "$(sheet_at "$1" "$2")"
  read -r tx ty <<< "$(sheet_at "$3" "$4")"
  ui_apart "$fx" "$fy"
  xdotool mousemove "$fx" "$fy" mousedown 1
  for i in 1 2 3 4 5 6 7 8; do
    xdotool mousemove $((fx + (tx - fx) * i / 8)) $((fy + (ty - fy) * i / 8))
    sleep 0.08
  done
  ui_sync
  xdotool mouseup 1
  ui_sync
}

# settled "logged place": waits until the place logged last for it stays
# the same over a sync and a short wait (a panel just built is laid out
# again once).
settled() {
  local before after
  for _ in $(seq 1 20); do
    before=$(grep -F "$1 at " "$UI_LOG" | tail -1)
    ui_sync
    sleep 0.3
    after=$(grep -F "$1 at " "$UI_LOG" | tail -1)
    [ -n "$after" ] && [ "$before" = "$after" ] && return 0
  done
  ui_fail "the place of '$1' did not settle"
}

# no_refusal: no drawing command refused since the mark.
no_refusal() {
  grep -q "Drawing command refused" <(tail -n +$((UI_MARK + 1)) "$UI_LOG") && ui_fail "a drawing command was refused"
  return 0
}

# annotations_drawn: the last "Drawing annotations drawn" line.
annotations_drawn() {
  grep -o "Drawing annotations drawn: .*" "$UI_LOG" | tail -1
}

# field_text "logged field" text: types a text into a panel's line field
# (all of it replaced) and sends it with Enter.
field_text() {
  settled "$1"
  ui_click_logged "$1"
  ui_sync
  ui_key ctrl+a
  xdotool type --delay 40 -- "$2"
  ui_key Return
  ui_sync
}

ui_start_display
ui_start_app --open "$FILE" --no-native-dialogs
ui_expect_log "plate.mitcad" "the design opened"

echo "--- Create Drawing and a top view"
ui_mark
ui_step "Create Drawing (search)"          ui_command "Create Drawing"
ui_expect_new "Create Drawing dialog opened" "its dialog"
ui_focus_dialog '^Create Drawing$'
ui_step "Template: ISO (Alt+T, Home)"     ui_key alt+t Home
ui_step "OK"                               ui_key Return
ui_focus_main
ui_expect_new "Drawing opened: D1 Drawing1, sheet 1 of 1" "its sheet over the view"
ui_expect_new "Drawing tool: Base View" "Base View is on"
ui_expect_new "Drawing sheet view " "the sheet placed"
# The Geometry group's new tools (mitcad#729).
for tool in Line Arc "Center Pattern" "Edge Extension"; do
  ui_expect_log "Drawing tool $tool at" "the $tool tool on the bar"
done
settled "Drawing panel drawingOption_direction"
ui_step "Orientation: Top"                 ui_click_logged "Drawing panel drawingOption_direction"
ui_key Down Return
ui_expect_new "Drawing option direction: top" "a top view next"
settled "Drawing panel drawingOption_scale"
ui_step "Scale: 1:1"                       ui_click_logged "Drawing panel drawingOption_scale"
ui_key Down Down Down Down Return
ui_expect_new "Drawing option scale: 1:1" "at 1:1"
ui_mark
ui_step "click the sheet at 150, 150"      sheet_click 150 150
ui_expect_new '"direction":"top"' "a top view asked for"
ui_expect_new "Drawing view 1 at" "the top view drawn"
ui_expect_new "4 center marks" "the holes' centre marks"
no_refusal
ui_step "end the tool (Esc)"               ui_key Escape
ui_step "and the selection (Esc)"          ui_key Escape

echo "--- The drawing's decimals and text height (mitcad#729)"
ui_mark
settled "Drawing panel drawingDefaultDecimals"
ui_step "Decimals: 1"                      ui_click_logged "Drawing panel drawingDefaultDecimals"
ui_key Down Down Return
ui_expect_new '"cmd":"edit_drawing"' "the drawing's decimals sent"
ui_expect_new '"decimals":1' "one decimal"
no_refusal
ui_mark
ui_step "Text Height: 5"                   field_text "Drawing panel drawingDefaultTextHeight" "5"
ui_expect_new '"text_height":5' "5 mm letters"
no_refusal
# The plate's width: its left and right edges below the holes.
ui_mark
settled "Drawing tool Dimension"
ui_step "Dimension"                        ui_click_logged "Drawing tool Dimension"
ui_expect_new "Drawing tool: Dimension" "the dimension tool is on"
ui_step "the left edge"                    sheet_click 120.5 125
ui_expect_new "Drawing picked line (1)" "the left edge picked"
ui_step "the right edge"                   sheet_click 179.5 125
ui_expect_new "Drawing picked line (2)" "the right edge picked"
ui_step "where the dimension goes"         sheet_click 150 100
ui_expect_new '"type":"dimension"' "a dimension asked for, its picks choosing its type"
ui_expect_new "Drawing annotations drawn: 1 = 60.0" "written with the drawing's one decimal"
no_refusal
ui_step "end the tool (Esc)"               ui_key Escape
ui_mark
ui_step "select the dimension"             click_apart "Drawing annotation 1"
ui_expect_new "Drawing annotation 1 selected" "the dimension selected"
ui_expect_new "Drawing panel drawingAnnotationHeight at" "its own text height in its panel"
ui_mark
ui_step "its Text Height: 2.5"             field_text "Drawing panel drawingAnnotationHeight" "2.5"
ui_expect_new '"height":2.5' "its own letters"
no_refusal

echo "--- A dimension's Replace Value (mitcad#1292)"
ui_expect_log "Drawing panel Replace Value: empty, placeholder 60.0" "the field empty, the measured value its placeholder"
ui_mark
ui_step "its Replace Value: SEE NOTE"      field_text "Drawing panel drawingAnnotationReplace" "SEE NOTE"
ui_expect_new '"format":{"replace":"SEE NOTE"}' "the text sent as its format's replace"
ui_expect_new "Drawing annotations drawn: 1 = SEE NOTE" "drawn with the text only"
ui_expect_new "Drawing panel Replace Value: SEE NOTE, measured 60.0 beside it" "the measured value beside the field"
ui_expect_new "Drawing panel drawingAnnotationMeasured at" "its Measured row"
no_refusal
# The sheet takes the keyboard back from the field (a click on the
# dimension), so that Ctrl+Z is the design's undo, not the field's.
ui_mark
settled "Drawing annotation 1"
ui_step "click the dimension"              click_apart "Drawing annotation 1"
ui_sync
ui_step "undo (Ctrl+Z)"                    ui_key ctrl+z
ui_expect_new "Drawing annotations drawn: 1 = 60.0" "the value back with one undo step"
ui_mark
ui_step "redo (Ctrl+Y)"                    ui_key ctrl+y
ui_expect_new "Drawing annotations drawn: 1 = SEE NOTE" "replaced again"
settled "Drawing annotation 1"
ui_step "select the dimension"             click_apart "Drawing annotation 1"
ui_expect_new "Drawing panel Replace Value: SEE NOTE, measured 60.0 beside it" "its panel shows the text and the value"
# An empty field: the measured value again.
ui_mark
settled "Drawing panel drawingAnnotationReplace"
ui_step "clear Replace Value"              ui_click_logged "Drawing panel drawingAnnotationReplace"
ui_sync
ui_key ctrl+a BackSpace Return
ui_sync
ui_expect_new '"format":{"replace":null}' "an empty field sends null"
ui_expect_new "Drawing annotations drawn: 1 = 60.0" "the measured value drawn again"
ui_expect_new "Drawing panel Replace Value: empty, placeholder 60.0" "the field empty again"
no_refusal
ui_step "Esc"                              ui_key Escape

echo "--- A centre pattern and an edge extension"
ui_mark
settled "Drawing tool Center Pattern"
ui_step "Center Pattern"                   ui_click_logged "Drawing tool Center Pattern"
ui_expect_new "Drawing tool: Center Pattern" "the tool is on"
ui_step "one hole's rim"                   sheet_click 173 150
ui_expect_new '"type":"center_pattern"' "a centre pattern asked for"
ui_expect_new "5 center marks" "the pattern drawn over the holes' own marks"
ui_expect_new "2 = center_pattern" "listed"
no_refusal
ui_mark
settled "Drawing tool Edge Extension"
ui_step "Edge Extension"                   ui_click_logged "Drawing tool Edge Extension"
ui_expect_new "Drawing tool: Edge Extension" "the tool is on"
settled "Drawing panel drawingOption_edges"
ui_step "Edges: One, Both Ways"            ui_click_logged "Drawing panel drawingOption_edges"
ui_key Down Return
ui_expect_new "Drawing option edges: one" "one edge"
ui_step "the plate's bottom edge"          sheet_click 150 120.5
ui_expect_new '"type":"edge_extension"' "an edge extension asked for"
ui_expect_new "3 = edge_extension" "drawn"
no_refusal

echo "--- A line in the view, an arc on the sheet, a text in the view"
ui_mark
settled "Drawing tool Line"
ui_step "Line"                             ui_click_logged "Drawing tool Line"
ui_expect_new "Drawing tool: Line" "the line tool is on"
# Its pen: Extra Thick (mitcad#1402), right after Thin and Thick.
settled "Drawing panel drawingOption_pen"
ui_step "Line: Extra Thick"                ui_click_logged "Drawing panel drawingOption_pen"
ui_key Down Down Return
ui_expect_new "Drawing option pen: extra_thick" "the extra thick pen"
ui_step "its first end, on the view"       sheet_click 125 175
ui_expect_new "Drawing point 1 at" "the first end"
ui_step "its second end"                   sheet_click 175 175
ui_expect_new '"type":"line"' "a line asked for"
ui_expect_new '"pen":"extra_thick"' "drawn with the extra thick pen"
ui_expect_new '"view":1' "in the view"
ui_expect_new "4 = line" "drawn"
no_refusal
ui_mark
settled "Drawing tool Arc"
ui_step "Arc"                              ui_click_logged "Drawing tool Arc"
ui_expect_new "Drawing tool: Arc" "the arc tool is on"
settled "Drawing panel drawingOption_attach"
ui_step "Drawn: On the Sheet"              ui_click_logged "Drawing panel drawingOption_attach"
ui_key Down Return
ui_expect_new "Drawing option attach: sheet" "on the sheet"
ui_step "its centre"                       sheet_click 40 40
ui_step "its start"                        sheet_click 50 40
ui_step "its end"                          sheet_click 40 50
ui_expect_new '"type":"arc"' "an arc asked for"
ui_expect_new '"sheet":0' "on the sheet"
ui_expect_new "5 = arc" "drawn"
no_refusal
ui_mark
settled "Drawing tool Text"
ui_step "Text"                             ui_click_logged "Drawing tool Text"
ui_expect_new "Drawing tool: Text" "the text tool is on"
settled "Drawing panel drawingOption_text"
ui_step "its text"                         ui_click_logged "Drawing panel drawingOption_text"
xdotool type --delay 40 -- "KEEP CLEAR"
ui_expect_new "Drawing option text: KEEP CLEAR" "the text typed"
settled "Drawing panel drawingOption_attach"
ui_step "Drawn: In the View Clicked"       ui_click_logged "Drawing panel drawingOption_attach"
ui_key Up Return
ui_expect_new "Drawing option attach: view" "in the view"
ui_step "where it goes, on the view"       sheet_click 130 165
ui_expect_new '"type":"text"' "a text asked for"
ui_expect_new '"view":1' "in the view"
ui_expect_new "6 = KEEP CLEAR" "drawn"
no_refusal
ui_step "end the tool (Esc)"               ui_key Escape
# The view dragged 10 mm right: the line drawn in it goes along. Pressed
# well inside the centre pattern's circle (radius 20 about the view's
# middle), clear of its lines: an annotation near the press (a few pixels
# of the screen, some millimetres of paper) would be dragged instead.
before=$(grep -o "Drawing annotation 4 at [0-9]*,[0-9]*" "$UI_LOG" | tail -1)
[ -n "$before" ] || ui_fail "no place of the line"
ui_mark
ui_step "drag the view right"              sheet_drag 157 143 167 143
ui_expect_new '"cmd":"edit_drawing_view","drawing":"D1","move":[' "the view moved"
no_refusal
ui_sync
after=$(grep -o "Drawing annotation 4 at [0-9]*,[0-9]*" "$UI_LOG" | tail -1)
[ "$before" != "$after" ] || ui_fail "the line drawn in the view did not move with it ($before)"
echo "ok   the line moved with its view ($before -> $after)"
[ -n "$SHOT" ] && ui_capture "$SHOT"

echo "--- The view rotated (mitcad#729)"
ui_step "Esc (the drag selected it)"       ui_key Escape
ui_mark
# Inside the centre pattern's circle again (its middle now at 160, 150).
ui_step "select the view"                  sheet_click 167 143
ui_expect_new "Drawing view 1 selected" "the view selected"
ui_expect_new "Drawing panel drawingViewRotation at" "its Rotation in its panel"
ui_mark
ui_step "Rotation: 90"                     field_text "Drawing panel drawingViewRotation" "90"
ui_expect_new '"rotation":90' "a quarter turn asked for"
ui_expect_new "Drawing sheet drawn: 1 views" "drawn turned"
no_refusal
annotations_drawn | grep -qF "1 = 60.0" || ui_fail "the dimension does not follow the view: $(annotations_drawn)"
echo "ok   the dimension still measures the plate: $(annotations_drawn)"
# The view's menu: a quarter turn back. A free place of the turned view
# (167, 143 turned a quarter about its middle).
ui_mark
ui_step "right-click the view"             sheet_click 167 157 3
ui_expect_new "Context menu: Projected View | Section View | Detail View | Break | Rotate 90° Left | Rotate 90° Right" "the view's menu"
ui_step "choose Rotate 90° Right"          ui_menu_choose "Rotate 90° Right"
ui_expect_new '"rotation":0' "turned back"
no_refusal
ui_mark
ui_step "undo (Ctrl+Z)"                    ui_key ctrl+z
ui_expect_new "Drawing sheet drawn: 1 views" "turned again"
ui_mark
ui_step "redo (Ctrl+Y)"                    ui_key ctrl+y
ui_expect_new "Drawing sheet drawn: 1 views" "turned back again"
ui_step "Esc"                              ui_key Escape

echo "--- The view's orientation changed under its section and detail (mitcad#729)"
ui_mark
settled "Drawing tool Section View"
ui_step "Section View"                     ui_click_logged "Drawing tool Section View"
ui_expect_new "Drawing tool: Section View" "the section tool is on"
ui_step "the line's first end, on the view" sheet_click 160 125
ui_step "its second end"                   sheet_click 160 175
ui_expect_new "Drawing placing view" "its view is placed next"
ui_step "where the section goes"           sheet_click 270 150
ui_expect_new '"type":"section"' "the section asked for"
ui_expect_new "Drawing view 2 at" "the section drawn"
no_refusal
ui_mark
settled "Drawing tool Detail View"
ui_step "Detail View"                      ui_click_logged "Drawing tool Detail View"
ui_expect_new "Drawing tool: Detail View" "the detail tool is on"
ui_step "the circle's middle, a hole"      sheet_click 180 150
ui_step "its edge"                         sheet_click 186 150
ui_expect_new "Drawing placing view" "its view is placed next"
ui_step "where the detail goes"            sheet_click 330 80
ui_expect_new '"type":"detail"' "the detail asked for"
ui_expect_new "Drawing view 3 at" "the detail drawn"
no_refusal
ui_step "end the tool (Esc)"               ui_key Escape
ui_step "and the selection (Esc)"          ui_key Escape
ui_mark
ui_step "select the top view"              sheet_click 167 143
ui_expect_new "Drawing view 1 selected" "the top view selected"
settled "Drawing panel drawingViewOrientation"
ui_mark
ui_step "Orientation: Front"               ui_click_logged "Drawing panel drawingViewOrientation"
ui_key Up Return
ui_expect_new '"direction":"front"' "the front asked for"
ui_expect_new "Drawing sheet drawn: 3 views" "drawn with its section and detail"
no_refusal
grep -F "Drawing sheet drawn" "$UI_LOG" | tail -1

echo "--- Any scale typed in the view's Scale (mitcad#1169)"
ui_expect_log "Drawing panel drawingViewScale at" "its Scale in its panel"
ui_mark
ui_step "Scale: abc"                       field_text "Drawing panel drawingViewScale" "abc"
ui_expect_new "Drawing scale refused: abc" "not a scale: refused"
ui_sync
grep -q '"cmd":"edit_drawing_view"' <(tail -n +$((UI_MARK + 1)) "$UI_LOG") && ui_fail "abc was sent to the model"
echo "ok   abc not sent"
ui_mark
ui_step "Scale: 2:3"                       field_text "Drawing panel drawingViewScale" "2:3"
ui_expect_new '"scale":"2:3"' "two thirds asked for"
# The model keeps 0.667; the field shows it as drawings write scales, 1:X
# or X:1 (ISO 5455): 1:1.5.
ui_expect_new "Drawing view 1 scale: 1:1.5 (0.667)" "the model's scale, shown as 1:1.5"
ui_expect_new "Drawing sheet drawn: 3 views" "drawn at 2:3"
no_refusal

echo "--- A right click is the tool's Esc (mitcad#1404)"
# Two lines on the sheet's empty upper left, then a third's first end: a
# right drag keeps it, the first right click drops it and keeps the tool,
# the second ends the tool, and one with no tool opens the sheet's menu.
ui_mark
settled "Drawing tool Line"
ui_step "Line"                             ui_click_logged "Drawing tool Line"
ui_expect_new "Drawing tool: Line" "the line tool is on"
settled "Drawing panel drawingOption_attach"
ui_step "Drawn: On the Sheet"              ui_click_logged "Drawing panel drawingOption_attach"
ui_key Down Return
ui_expect_new "Drawing option attach: sheet" "on the sheet"
ui_mark
ui_step "the first line's first end"       sheet_click 30 250
ui_step "its second end"                   sheet_click 80 250
ui_expect_new '"type":"line"' "the first line asked for"
no_refusal
ui_mark
ui_step "the second line's first end"      sheet_click 30 240
ui_step "its second end"                   sheet_click 80 240
ui_expect_new '"type":"line"' "the second line asked for"
no_refusal
ui_mark
ui_step "a third line's first end"         sheet_click 30 230
ui_expect_new "Drawing point 1 at" "a line in progress"
# A right drag (well over 4 px) is not a click: the line stays in progress.
read -r rx ry <<< "$(sheet_at 50 230)"
read -r tx ty <<< "$(sheet_at 70 230)"
ui_step "right drag"                       xdotool mousemove "$rx" "$ry" mousedown 3
for i in 1 2 3 4; do
  xdotool mousemove $((rx + (tx - rx) * i / 4)) $((ry + (ty - ry) * i / 4))
  sleep 0.08
done
xdotool mouseup 3
ui_sync
ui_expect_new "Drawing right drag: not a click" "a right drag is not a click"
grep -q "Drawing right-click\|Drawing picks dropped" <(tail -n +$((UI_MARK + 1)) "$UI_LOG") &&
  ui_fail "the right drag ended the line in progress"
ui_mark
ui_step "right-click"                      sheet_click 55 220 3
ui_expect_new "Drawing right-click: Esc of the tool" "the right click is the tool's Esc"
ui_expect_new "Drawing picks dropped: Line" "the line in progress ends"
ui_sync
grep -q "Drawing tool: selection" <(tail -n +$((UI_MARK + 1)) "$UI_LOG") &&
  ui_fail "the first right click ended the tool"
grep -q "Context menu:" <(tail -n +$((UI_MARK + 1)) "$UI_LOG") && ui_fail "a right click with a tool opened a menu"
echo "ok   the tool stays"
ui_mark
ui_step "right-click again"                sheet_click 55 210 3
ui_expect_new "Drawing right-click: Esc of the tool" "the right click is the tool's Esc"
ui_expect_new "Drawing tool: selection" "the tool ends"
ui_sync
grep -q "Context menu:" <(tail -n +$((UI_MARK + 1)) "$UI_LOG") && ui_fail "the right click that ended the tool opened a menu"
echo "ok   no menu"
ui_mark
ui_step "right-click with no tool"         sheet_click 60 200 3
ui_expect_new "Context menu: Sheet… | Add Revision… | Add Sheet" "the sheet's menu"
ui_step "Escape (close the menu)"          ui_key Escape
no_refusal
ui_crashed && ui_fail "crashed"

grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
ui_finish "UI drawing features test"
