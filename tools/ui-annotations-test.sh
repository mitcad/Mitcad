#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands app/framework app/browser app/view core/model-api/src core/model/src/document tools/cli
# check-all sources: app/MainWindowMeasurements.cpp app/MainWindowMeasureHistory.cpp app/MainWindowAnnotations.cpp core/model-base/src/iso286.rs app/MainWindowVisibility.cpp
# check-all sources: core/model-drawing/src third_party/fonts/iso-lettering
# Annotations (mitcad#702) through the real UI, in the Docked layout, on
# two cylinders of radius 5, 20 high (Body1 about Z through the origin,
# Body2 through (30, 40)), with an offset Gone of a block that is deleted
# (broken):
#   - Keep in the Measure panel: the round faces kept as AxisDistance1
#     (50), Body1's top rim as Diameter1 (Ø10).
#   - A double-click on AxisDistance1's text in the view: its editor; a ±
#     tolerance (0.1).
#   - Its text dragged along its dimension line, then the line dragged:
#     moved, kept relative to its picks.
#   - The Annotations window (command search): Gone marked broken;
#     Diameter1's row double-clicked, an H7 fit; the kind filter and the
#     search; both rows selected and changed together (Dark Grey, kept
#     selected through the refresh, two decimals, the group Shop);
#     Diameter1 put in the group Inspect; the group Shop hidden and shown
#     again.
#   - Export Snapshot of the group Inspect only.
#   - Saved and opened again: groups, tolerances and the text's place kept;
#     limits typed as sizes come back as exact deviations, and a colour
#     changed alone leaves the tolerance untouched.
#   - Datum Label… on Body1's top face (DatumA) and Frame… on Body2's round face
#     (Frame1: perpendicularity, Ø0.05 Ⓜ, to A) in the Measure panel;
#     both listed, the kind filter on frames, DatumA's row highlighting
#     its face; Add Frame… under AxisDistance1 (Frame2: parallelism to A,
#     in its group and colour); Frame1's zone narrowed in its editor.
#   - DatumA's pick picked again in the view (Pick Again…: no vertex
#     offered, a click on nothing leaving the view waiting; then Body2's
#     top face) and Frame1's plane picked (Pick…: Esc cancels and the
#     editor opens again; then Body1's top face), the editor opening again
#     after each; saved with them.
#   - The editor's rows (mitcad#713): Diameter1's has no Plane row; on a
#     block opened next, Tilt's (an angle, Distance its other kind) has
#     Pick… for the plane disabled, enabled for Distance and disabled
#     again for Angle; Flat, a frame under Rise whose pick is the origin,
#     shows no pick or plane row, and Placed set to On its leader shows
#     Pick Again…; OK asks for the pick first, and the pick sends `on:
#     null` with it in one edit; saved on its leader.
#   - 40 annotations on a block (mitcad#759): the Annotations window
#     scrolled to its last row, a light bulb there clicked off; the list
#     stays where it was after the refresh.
#   - Hole Callout in the Measure panel (mitcad#902) on a plate with four
#     counterbored through holes: HoleCallout1 "4x Ø6.6 THRU / ⌴ Ø11 ↧ 6.8"
#     written from the hole's feature, drawn in the bundled lettering, saved.
# tools/ui-windows-annotations-test.ps1 does the editor (Diameter1's
# without a Plane row too), the drag, the window's changes, the datum
# label, both frames and a hole callout in the Windows VM (not the picks
# again: its input reaches the main window's places only).
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-annotations-test.sh [screenshot.png] (with it, also
# <screenshot>-frames.png once the datum label and the frames are drawn)

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-annotations.XXXXXX)
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
R='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'

cat > "$WORK/axes.json" << EOF
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_circle", "sketch": "F1", "center": [0, 0], "diameter": 10},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "r{c1}"}],
    "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_circle", "sketch": "F3", "center": [30, 40], "diameter": 10},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F3", "region": "r{c1}"}],
    "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F5", "corner": [20, -30], "width": 10, "height": 10},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F5", "region": "$R"}],
    "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
  {"cmd": "add_measurement", "name": "Gone", "a": {"body": "F6.b0", "face": "F6:end($R)"},
    "b": {"body": "F6.b0", "face": "F6:start($R)"}},
  {"cmd": "delete_feature", "uid": "F6"}
]
EOF
"$CLI" run "$WORK/axes.json" --save "$WORK/axes.mitcad" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# wait_enabled "place": until the last "<place> at x,y" line is not
# " (disabled)".
wait_enabled() {
  local last
  for _ in $(seq 1 30); do
    last=$(grep -F "$1 at " "$UI_LOG" | tail -1)
    [ -n "$last" ] && [[ $last != *"(disabled)" ]] && return 0
    sleep 0.2
  done
  ui_fail "'$1' is not enabled: $last"
}

# logged_place "text": the last "<text> at x,y" in the window's coordinates.
logged_place() {
  grep -F "$1 at " "$UI_LOG" | tail -1 | sed -n 's/.* at \([0-9]*\),\([0-9]*\).*/\1 \2/p'
}

# focus_window: the Annotations window takes the keyboard.
focus_window() { ui_focus_dialog '^Annotations$'; }

ui_start_display
ui_start_app --open "$WORK/axes.mitcad"
ui_expect_log "Annotations: Gone broken: a pick is not there" "only the broken one yet"
ui_step "fit (F6)"                         ui_key F6

echo "--- Keep a measurement"
ui_mark
ui_step "measure (I)"                      ui_key i
ui_expect_new "Command Measure started" "measure started"
ui_step "pick Body1's round face"          ui_click_pick "face F2.b0/F2:side(c1)"
ui_step "pick Body2's round face"          ui_click_pick "face F4.b0/F4:side(c1)"
ui_expect_new "axis distance 50" "the axes 50 apart"
wait_enabled "Measure Keep"
ui_step "Keep"                             ui_click_logged "Measure Keep"
ui_expect_new "Annotation kept: AxisDistance1" "kept as the axes' distance"
ui_step "close (Enter)"                    ui_key Return
ui_expect_new "Closed Measure" "measure closed"
ui_mark
ui_step "measure (I)"                      ui_key i
ui_expect_new "Command Measure started" "measure started again"
ui_step "pick Body1's top rim"             ui_click_pick "edge F2.b0/E{F2:end(r{c1})|"
ui_expect_new "Inspect Measure: length 31.416, radius 5, diameter 10" "the rim measured"
wait_enabled "Measure Keep"
ui_step "Keep"                             ui_click_logged "Measure Keep"
ui_expect_new "Annotation kept: Diameter1" "kept as the rim's diameter"
ui_expect_new "Annotations drawn: AxisDistance1 = 50 (auto) | Diameter1 = Ø10 (auto)" "both drawn, in Auto"
ui_step "close (Enter)"                    ui_key Return
ui_expect_new "Closed Measure" "measure closed"

echo "--- A ± tolerance from the editor, opened in the view"
ui_wait_idle
ui_double_click_expect "Annotation label AxisDistance1" "Edit Annotation dialog opened: AxisDistance1 axis_distance" \
  "a double-click on its text opens its editor"
ui_mark
ui_focus_dialog '^Edit Annotation$'
ui_step "Tolerance: ± (Alt+T, Down)"       ui_key alt+t Down
ui_step "OK (Enter)"                       ui_key Return
ui_focus_main
ui_expect_new 'Edit Annotation dialog: {"tolerance":{"type":"symmetric","value":0.1}}' "± 0.1 asked for"
ui_expect_new "Annotations drawn: AxisDistance1 = 50 ±0.1 (auto) | Diameter1 = Ø10 (auto)" \
  "drawn with its tolerance"

echo "--- Its text dragged"
read -r lx ly <<< "$(logged_place "Annotation label AxisDistance1")"
[ -n "$lx" ] || ui_fail "no place of AxisDistance1's label"
read -r sx sy <<< "$(ui_logged_at "Annotation label AxisDistance1")"
ui_mark
ui_apart "$sx" "$sy"
xdotool mousemove "$sx" "$sy" mousedown 1
for i in 1 2 3 4 5 6 7 8; do
  xdotool mousemove $((sx + 10 * i)) "$sy"
  sleep 0.08
done
ui_sync
xdotool mouseup 1
ui_expect_new "Annotation AxisDistance1 dragged: text by" "the text dragged"
ui_expect_new "Annotation AxisDistance1 moved: " "moved, an undo step"
grep -F "Annotation AxisDistance1 moved: " "$UI_LOG" | tail -1 | grep -qF '"text_offset"' ||
  ui_fail "the drag did not give the text an offset"
ui_expect_new "Annotation label AxisDistance1 at" "its label placed again"
read -r nx ny <<< "$(logged_place "Annotation label AxisDistance1")"
[ $(((nx - lx) * (nx - lx) + (ny - ly) * (ny - ly))) -gt 900 ] ||
  ui_fail "the label stayed at $nx,$ny (was $lx,$ly)"
echo "ok   the label moved from $lx,$ly to $nx,$ny"

echo "--- Its dimension line dragged"
read -r gx gy <<< "$(ui_logged_at "Annotation line AxisDistance1")"
[ -n "$gx" ] || ui_fail "no place of AxisDistance1's dimension line"
ui_mark
ui_apart "$gx" "$gy"
xdotool mousemove "$gx" "$gy" mousedown 1
for i in 1 2 3 4 5 6 7 8; do
  xdotool mousemove $((gx + 3 * i)) $((gy + 3 * i))
  sleep 0.08
done
ui_sync
xdotool mouseup 1
ui_expect_new "Annotation AxisDistance1 dragged: line by" "the line dragged"
ui_expect_new "Annotation AxisDistance1 moved: " "moved, an undo step"
grep -F "Annotation AxisDistance1 moved: " "$UI_LOG" | tail -1 | grep -qF '"line_offset"' ||
  ui_fail "the drag did not give the line an offset"
[ -n "$SHOT" ] && ui_capture "$SHOT"

echo "--- The Annotations window: an H7 fit"
ui_mark
ui_step "Annotations (search)"             ui_command "Annotations"
ui_expect_new "Annotations window opened: 3 annotation(s)" "the window opened"
ui_expect_new "Annotations row Diameter1: diameter, Ø10, -, -, auto, shown" "Diameter1 listed"
ui_expect_new "Annotations row AxisDistance1: axis_distance, 50 ±0.1, ±0.1, -, auto, shown" "AxisDistance1 listed"
ui_expect_new "Annotations row Gone: offset, Broken, -, -, auto, broken: a pick is not there" "Gone marked broken"
ui_expect_new "Annotations row Diameter1 at" "its rows laid out"
ui_double_click_expect "Annotations row Diameter1" "Edit Annotation dialog opened: Diameter1 diameter" \
  "a double-click on a row opens its editor"
# A diameter (or a radius, its other kind) lies in its circle's plane: no
# Plane row (mitcad#713).
ui_expect_new "Edit Annotation pick a at" "its pick's button placed"
ui_sync
tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qF "Edit Annotation pick plane" &&
  ui_fail "a plane offered for a diameter"
echo "ok   no Plane row for a diameter"
ui_mark
ui_focus_dialog '^Edit Annotation$'
ui_step "Tolerance: fit (Alt+T, Down x4)"  ui_key alt+t Down Down Down Down
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new 'Edit Annotation dialog: {"tolerance":{"code":"H7","type":"fit"}}' "an H7 fit asked for"
ui_expect_new "Diameter1 = Ø10 H7 (auto)" "drawn with its fit code"
ui_expect_new "Annotations row Diameter1: diameter, Ø10 H7, H7, -, auto, shown" "the window lists its fit"

echo "--- The kind filter and the search"
ui_mark
focus_window
ui_step "Kind: Diameter (Alt+K, Down x3)"  ui_key alt+k Down Down Down
ui_expect_new "Annotations shown rows: Diameter1" "only the diameter left"
ui_step "Kind: all (Alt+K, Home)"          ui_key alt+k Home
ui_expect_new "Annotations shown rows: Gone, AxisDistance1, Diameter1" "every row again"
ui_step "click the search"                 ui_click_logged "Annotations search"
focus_window
ui_step "search 1"                         ui_type_text "1"
ui_expect_new "Annotations shown rows: AxisDistance1, Diameter1" "the broken one left out by the search"

echo "--- Several changed together"
ui_mark
ui_step "select AxisDistance1"             ui_click_logged "Annotations row AxisDistance1"
focus_window
ui_step "and the other (Ctrl+A)"           ui_key ctrl+a
ui_expect_new "Annotations selected: AxisDistance1, Diameter1" "both rows selected"
ui_expect_new "Annotations highlighted: AxisDistance1, Diameter1" "both highlighted in the view"
ui_mark
ui_step "Colour… (Alt+C)"                  ui_key alt+c
ui_expect_new "Annotation Colour dialog opened: 2 annotation(s) auto (Auto | Black | Dark Grey | Custom…)" \
  "the colours offered: a technical drawing's, no bright presets (mitcad#902)"
ui_focus_dialog '^Annotation Colour$'
ui_step "Dark Grey (Down x2, Enter)"       ui_key Down Down Return
ui_expect_new "Annotations drawn: AxisDistance1 = 50 ±0.1 (#495057) | Diameter1 = Ø10 H7 (#495057)" "both dark grey"
ui_expect_new "Annotations row Diameter1: diameter, Ø10 H7, H7, -, #495057, shown" "the rows refreshed"
# The refresh keeps both rows selected.
tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep "^Annotations selected: " | grep -vqF "Annotations selected: AxisDistance1, Diameter1" &&
  ui_fail "the selection changed with the refresh: $(grep "^Annotations selected: " "$UI_LOG" | tail -1)"
echo "ok   both rows still selected after the refresh"
ui_mark
focus_window
ui_step "Decimals… (Alt+D)"                ui_key alt+d
ui_expect_new "Annotation Decimals dialog opened: 2 dimension(s) null" "the decimals offered"
ui_focus_dialog '^Annotation Decimals$'
ui_step "2 (Down x3, Enter)"               ui_key Down Down Down Return
ui_expect_new "Annotations drawn: AxisDistance1 = 50.00 ±0.1 (#495057) | Diameter1 = Ø10.00 H7 (#495057)" \
  "both with two decimals"
ui_mark
focus_window
ui_step "Group… (Alt+R)"                   ui_key alt+r
ui_expect_new "Annotation Group dialog opened: 2 annotation(s)" "the group asked for"
ui_focus_dialog '^Annotation Group$'
ui_step "Shop"                             ui_type_text "Shop"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "| AxisDistance1 axis_distance in Shop | Diameter1 diameter in Shop" "both in Shop"
ui_mark
ui_step "select Diameter1 alone"           ui_click_logged "Annotations row Diameter1"
ui_expect_new "Annotations selected: Diameter1" "Diameter1 selected"
focus_window
ui_step "Group… (Alt+R)"                   ui_key alt+r
ui_focus_dialog '^Annotation Group$'
ui_step "Inspect"                          ui_type_text "Inspect"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "| AxisDistance1 axis_distance in Shop | Diameter1 diameter in Inspect" "Diameter1 in Inspect"

echo "--- A group hidden and shown"
ui_mark
focus_window
ui_step "Group filter: Shop (Alt+G, Down x3)" ui_key alt+g Down Down Down
ui_expect_new "Annotations group filter: Shop" "the filter on Shop"
ui_step "Hide Group (Alt+U)"               ui_key alt+u
ui_expect_new "Annotations hide group Shop: AxisDistance1" "the group hidden"
ui_expect_new "Annotations drawn: Diameter1 = Ø10.00 H7 (#495057)" "only Inspect's drawn"
ui_mark
focus_window
ui_step "Show Group (Alt+W)"               ui_key alt+w
ui_expect_new "Annotations show group Shop: AxisDistance1" "the group shown"
ui_expect_new "Annotations drawn: AxisDistance1 = 50.00 ±0.1 (#495057) | Diameter1 = Ø10.00 H7 (#495057)" \
  "both drawn again"
ui_step "close the window (Escape)"        ui_key Escape
ui_focus_main

echo "--- A snapshot of the group Inspect"
read -r _ _ _ _ vw vh <<< "$(grep -o "View area [0-9]* [0-9]* [0-9]* [0-9]*" "$UI_LOG" | tail -1)"
[ -n "$vw" ] || ui_fail "the app did not log its view area"
ui_mark
ui_step "Export Snapshot (search)"         ui_command "Export Snapshot"
ui_expect_new "Snapshot dialog opened: size 2x, transparent off, annotations all (2 shown)" "the dialog opened"
ui_focus_dialog '^Export Snapshot$'
ui_step "the file"                         ui_type_text "$WORK/inspect.png"
ui_step "Annotations: Inspect (Alt+A, Down Down)" ui_key alt+a Down Down
ui_expect_new "Snapshot dialog: annotations = group Inspect" "the group Inspect chosen"
ui_step "OK (Enter)"                       ui_key Return
ui_focus_main
ui_expect_new "Snapshot saved $WORK/inspect.png: $((2 * vw)) x $((2 * vh)), opaque, 1 annotation(s) of group Inspect" \
  "the snapshot written with the group's annotation only" 30
[ -s "$WORK/inspect.png" ] || ui_fail "no snapshot file"

echo "--- Saved and opened again"
ui_mark
ui_step "save (Ctrl+S)"                    ui_key ctrl+s
ui_expect_new "Saved $WORK/axes.mitcad" "saved"
for field in '"group": "Shop"' '"group": "Inspect"' '"code": "H7"' '"type": "symmetric"' '"text_offset"' '"decimals": 2'; do
  grep -qF "$field" "$WORK/axes.mitcad" || ui_fail "the design file has no $field"
done
ui_stop_app
ui_start_app --open "$WORK/axes.mitcad"
ui_expect_log "| AxisDistance1 axis_distance in Shop | Diameter1 diameter in Inspect" "read back in their groups"
ui_expect_log "Annotations drawn: AxisDistance1 = 50.00 ±0.1 (#495057) | Diameter1 = Ø10.00 H7 (#495057)" \
  "drawn as they were"

echo "--- Limits typed as sizes, and a colour alone"
ui_mark
ui_step "Annotations (search)"             ui_command "Annotations"
ui_expect_new "Annotations row AxisDistance1 at" "its rows laid out"
ui_double_click_expect "Annotations row AxisDistance1" "Edit Annotation dialog opened: AxisDistance1 axis_distance" \
  "its editor opened"
ui_mark
ui_focus_dialog '^Edit Annotation$'
ui_step "Tolerance: limits (Alt+T, Down Down)" ui_key alt+t Down Down
ui_step "OK (Enter)"                       ui_key Return
# 50.1 and 49.9 as typed, read back as deviations without rounding noise.
ui_expect_new 'Edit Annotation dialog: {"tolerance":{"lower":-0.1,"type":"limits","upper":0.1}}' \
  "the limits as exact deviations"
ui_expect_new "Annotations drawn: AxisDistance1 = 50.10/49.90 (#495057)" "drawn as the limit sizes"
ui_double_click_expect "Annotations row AxisDistance1" "Edit Annotation dialog opened: AxisDistance1 axis_distance" \
  "its editor opened again"
ui_mark
ui_focus_dialog '^Edit Annotation$'
ui_step "Colour: Black (Alt+C, Up)"        ui_key alt+c Up
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new 'Edit Annotation dialog: {"color":"#212529"}' "only the colour sent, the tolerance untouched"
ui_expect_new "Annotations drawn: AxisDistance1 = 50.10/49.90 (#212529)" "the limits as they were"
focus_window
ui_step "close the window (Escape)"        ui_key Escape
ui_focus_main

echo "--- A datum label and a feature control frame from the Measure panel"
ui_mark
ui_step "measure (I)"                      ui_key i
ui_expect_new "Command Measure started" "measure started"
ui_step "pick Body1's top face"            ui_click_pick "face F2.b0/F2:end(r{c1})"
wait_enabled "Measure Datum Label"
ui_mark
ui_step "Datum Label…"                           ui_click_logged "Measure Datum Label"
ui_expect_new "Datum Label dialog opened: face F2:end(r{c1}) of F2.b0 A" "the first free letter offered"
ui_focus_dialog '^Datum Label$'
ui_step "OK (Enter)"                       ui_key Return
ui_focus_main
ui_expect_new "Datum Label dialog: A" "A chosen"
ui_expect_new "Annotation kept: DatumA" "the datum label added"
ui_expect_new "| DatumA = A (auto)" "drawn with its letter, in Auto"
ui_step "close (Enter)"                    ui_key Return
ui_expect_new "Closed Measure" "measure closed"
ui_mark
ui_step "measure (I)"                      ui_key i
ui_expect_new "Command Measure started" "measure started again"
ui_step "pick Body2's round face"          ui_click_pick "face F4.b0/F4:side(c1)"
wait_enabled "Measure Frame"
ui_mark
ui_step "Frame…"                           ui_click_logged "Measure Frame"
ui_expect_new "Feature Control Frame dialog opened: face F4:side(c1) of F4.b0 {}" "the frame asked for"
ui_focus_dialog '^Feature Control Frame$'
ui_step "Perpendicularity (Down x7)"       ui_key Down Down Down Down Down Down Down
ui_step "Cylindrical zone (Alt+Y)"         ui_key alt+y
ui_step "Maximum material (Alt+M, Down)"   ui_key alt+m Down
ui_step "datum A (Alt+D, a)"               ui_key alt+d a
ui_step "OK (Alt+M, Enter)"                ui_key alt+m Return
ui_focus_main
ui_expect_new 'Feature Control Frame dialog: {"characteristic":"perpendicularity","datums":["A"],"diameter":true,"modifier":"M","value":0.05}' \
  "perpendicularity, a cylindrical zone of 0.05 at maximum material, to A"
ui_expect_new "Annotation kept: Frame1" "the frame added"
ui_expect_new "| Frame1 = ⟂ | Ø0.05 Ⓜ | A (auto)" "drawn in its cells"
ui_step "close (Enter)"                    ui_key Return
ui_expect_new "Closed Measure" "measure closed"

echo "--- Listed and filtered, its picks highlighted, a frame under a dimension, edited"
ui_mark
ui_step "Annotations (search)"             ui_command "Annotations"
ui_expect_log "Annotations row DatumA: datum, A, -, -, auto" "the datum label listed"
ui_expect_log "Annotations row Frame1: frame, ⟂ | Ø0.05 Ⓜ | A, Ø0.05 Ⓜ, -, auto" "the frame listed with its cells"
ui_expect_new "Annotations row Frame1 at" "its rows laid out"
focus_window
ui_step "Kind: Feature control frame (Alt+K, Down x5)" ui_key alt+k Down Down Down Down Down
ui_expect_new "Annotations shown rows: Frame1" "only the frame left"
ui_step "Kind: all (Alt+K, Home)"          ui_key alt+k Home
ui_expect_new "Annotations shown rows: Gone, AxisDistance1, Diameter1, DatumA, Frame1" "every row again"
ui_mark
ui_step "select DatumA"                    ui_click_logged "Annotations row DatumA"
ui_expect_new "Annotations selected: DatumA" "DatumA selected"
ui_expect_new "Annotation picks highlighted: DatumA: face F2:end(r{c1}) of F2.b0" "its face highlighted in the view"
ui_mark
ui_step "select AxisDistance1"             ui_click_logged "Annotations row AxisDistance1"
ui_expect_new "Annotations selected: AxisDistance1" "AxisDistance1 selected"
wait_enabled "Annotations Add Frame"
focus_window
ui_step "Add Frame… (Alt+F)"               ui_key alt+f
ui_expect_new "Feature Control Frame dialog opened: AxisDistance1 {}" "a frame under the dimension asked for"
ui_focus_dialog '^Feature Control Frame$'
ui_step "Parallelism (Down x6)"            ui_key Down Down Down Down Down Down
ui_step "datum A (Alt+D, a)"               ui_key alt+d a
ui_step "OK (Alt+M, Enter)"                ui_key alt+m Return
ui_expect_new 'Feature Control Frame dialog: {"characteristic":"parallelism","datums":["A"],"value":0.05}' \
  "parallelism of 0.05 to A"
ui_expect_new "Frame added under AxisDistance1" "the frame under the dimension"
ui_expect_new "Annotations row Frame2: frame, ∥ | 0.05 | A, 0.05, Shop, #212529, shown" \
  "in the dimension's group and colour"
ui_expect_new "| Frame2 = ∥ | 0.05 | A (#212529)" "drawn under the dimension"
# The rows' places are logged when they change (with Frame2's row): the
# last ones, once the window has settled, are the rows' now.
ui_sync
ui_double_click_expect "Annotations row Frame1" "Edit Annotation dialog opened: Frame1 frame" "the frame's editor opened"
ui_mark
ui_focus_dialog '^Edit Annotation$'
ui_step "Tolerance 0.02 (Alt+T)"           ui_key alt+t ctrl+a
ui_step "type 0.02"                        ui_type_text "0.02"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new 'Edit Annotation dialog: {"frame":{"characteristic":"perpendicularity","datums":["A"],"diameter":true,"modifier":"M","value":0.02}}' \
  "the narrower zone asked for"
ui_expect_new "| Frame1 = ⟂ | Ø0.02 Ⓜ | A (auto)" "drawn with it"
focus_window
ui_step "close the window (Escape)"        ui_key Escape
ui_focus_main
# The dimensions, the fitted diameter, the datum label and both frames
# together, for a look at them (mitcad#894).
[ -n "$SHOT" ] && ui_sync && ui_capture "${SHOT%.png}-frames.png"

echo "--- A datum label's pick and a frame's plane picked again in the view"
ui_wait_idle
ui_double_click_expect "Annotation label DatumA" "Edit Annotation dialog opened: DatumA datum" \
  "a double-click on its letter opens its editor"
ui_expect_new "Edit Annotation pick a at" "its pick's button placed"
ui_sync # the dialog settled: its buttons' places logged again if they moved
# A vertex of Body2 where Measure logged it (the camera has not moved).
# ui_pick_at fails in its own subshell here: stop on its status.
vertex_place=$(ui_pick_at "vertex F4.b0/") ||
  ui_fail "no vertex place of Body2 logged by Measure"
read -r px py <<< "$vertex_place"
ui_mark
ui_step "Pick Again…"                      ui_click_logged "Edit Annotation pick a"
ui_expect_new "Annotation pick started: DatumA a" "the view waits for the pick"
ui_expect_new "Pick places:" "the places to pick logged"
# A datum label marks a feature: no vertex offered. A click on nothing
# takes nothing, and the view waits on.
tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -q "^Pick vertex " &&
  ui_fail "vertices offered for a datum label's pick"
read -r _ _ vx vy vw vh <<< "$(grep -o "View area [0-9]* [0-9]* [0-9]* [0-9]*" "$UI_LOG" | tail -1)"
[ -n "$vh" ] || ui_fail "the app did not log its view area"
ui_step "click beside the bodies"          xdotool mousemove $((X + (vx + 30) * UI_SCALE)) $((Y + (vy + vh / 2) * UI_SCALE)) click 1
ui_expect_new "Annotation pick still waited for: DatumA a" "nothing picked, still waiting"
ui_sync
tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qF "Edit Annotation dialog opened: DatumA" &&
  ui_fail "the editor opened again after a refused pick"
echo "ok   still waiting for the pick"
# The vertex's place: no vertex taken (an edge through it may be).
ui_mark
ui_step "click the vertex's place"         xdotool mousemove "$px" "$py" click 1
ui_expect_new "Annotation pick" "the click handled"
ui_sync
tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -q "^Annotation picked: DatumA a: vertex " &&
  ui_fail "a vertex taken for a datum label"
if tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -q "^Annotation picked: DatumA a: "; then
  # An edge through it was taken: the editor opens again; Pick Again…
  # once more for the face.
  ui_expect_new "Edit Annotation dialog opened: DatumA datum" "its editor open again after the edge"
  ui_expect_new "Edit Annotation pick a at" "its pick's button placed again"
  ui_sync
  ui_mark
  ui_focus_dialog '^Edit Annotation$'
  ui_step "Pick Again… once more"          ui_click_logged "Edit Annotation pick a"
  ui_expect_new "Annotation pick started: DatumA a" "the view waits for the pick again"
fi
echo "ok   no vertex taken"
ui_step "pick Body2's top face"            ui_click_pick "face F4.b0/F4:end(r{c1})"
ui_expect_new "Annotation picked: DatumA a: face F4:end(r{c1}) of F4.b0" "picked again"
ui_expect_new "Edit Annotation dialog opened: DatumA datum" "its editor open again"
ui_focus_dialog '^Edit Annotation$'
ui_step "close the editor (Escape)"        ui_key Escape
ui_focus_main
ui_wait_idle
ui_double_click_expect "Annotation label Frame1" "Edit Annotation dialog opened: Frame1 frame" \
  "a double-click on its cells opens its editor"
ui_expect_new "Edit Annotation pick plane at" "the plane's button placed"
ui_sync
ui_mark
ui_step "Pick… (the plane)"                ui_click_logged "Edit Annotation pick plane"
ui_expect_new "Annotation pick started: Frame1 plane" "the view waits for a plane"
# Esc: no pick, and the editor opens again; then Pick… once more.
ui_focus_main
ui_step "cancel (Escape)"                  ui_key Escape
ui_expect_new "Annotation pick cancelled: Frame1" "the pick cancelled"
ui_expect_new "Edit Annotation dialog opened: Frame1 frame" "its editor open again"
ui_expect_new "Edit Annotation pick plane at" "the plane's button placed again"
ui_sync
ui_mark
ui_focus_dialog '^Edit Annotation$'
ui_step "Pick… (the plane) again"          ui_click_logged "Edit Annotation pick plane"
ui_expect_new "Annotation pick started: Frame1 plane" "the view waits for a plane again"
ui_expect_new "Pick places:" "the planar faces logged"
ui_step "pick Body1's top face"            ui_click_pick "face F2.b0/F2:end(r{c1})"
ui_expect_new "Annotation picked: Frame1 plane: face F2:end(r{c1}) of F2.b0" "its plane picked"
ui_expect_new "Edit Annotation dialog opened: Frame1 frame" "its editor open again"
ui_focus_dialog '^Edit Annotation$'
ui_step "close the editor (Escape)"        ui_key Escape
ui_focus_main

echo "--- Saved with them"
ui_mark
ui_step "save (Ctrl+S)"                    ui_key ctrl+s
ui_expect_new "Saved $WORK/axes.mitcad" "saved"
for field in '"letter": "A"' '"characteristic": "perpendicularity"' '"modifier": "M"' '"on": "AxisDistance1"'; do
  grep -qF "$field" "$WORK/axes.mitcad" || ui_fail "the design file has no $field"
done
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"

# wait_disabled "place": until the last "<place> at x,y" line is
# " (disabled)".
wait_disabled() {
  local last
  for _ in $(seq 1 30); do
    last=$(grep -F "$1 at " "$UI_LOG" | tail -1)
    [[ $last == *"(disabled)" ]] && return 0
    sleep 0.2
  done
  ui_fail "'$1' is not disabled: $last"
}

echo "--- The editor's rows (mitcad#713) on a block"
# A 20 mm block: Rise from the origin (a point) to its top face, the frame
# Flat under it (its pick the origin, only an anchor there), and Tilt, the
# angle between its top face and the side away from the origin (Distance
# its other kind), drawn apart from Rise and Flat.
cat > "$WORK/rows.json" << EOF
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 20, "height": 20},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$R"}],
    "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
  {"cmd": "add_annotation", "name": "Rise", "a": {"datum": "origin"}, "b": {"body": "F2.b0", "face": "F2:end($R)"}},
  {"cmd": "add_annotation", "name": "Flat", "a": {"datum": "origin"}, "on": "Rise",
    "frame": {"characteristic": "flatness", "value": 0.1}},
  {"cmd": "add_annotation", "name": "Tilt", "kind": "angle", "a": {"body": "F2.b0", "face": "F2:end($R)"},
    "b": {"body": "F2.b0", "face": "F2:side(c3[c2,c4])"}}
]
EOF
"$CLI" run "$WORK/rows.json" --save "$WORK/rows.mitcad" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli (the block)"; }
ui_stop_app
ui_start_app --open "$WORK/rows.mitcad"
ui_expect_log "Annotation label Flat at" "the frame drawn under Rise"
# The labels' places are logged again when the view's layout or the camera
# moves them: the last ones logged are where they are after the fit.
ui_step "fit (F6)"                         ui_key F6
ui_sync

echo "--- Tilt's Plane row follows its kind"
ui_mark
ui_step "Annotations (search)"             ui_command "Annotations"
ui_expect_new "Annotations row Tilt at" "its rows laid out"
ui_double_click_expect "Annotations row Tilt" "Edit Annotation dialog opened: Tilt angle" "Tilt's editor opened"
ui_expect_new "Edit Annotation pick plane at" "its plane's button placed"
ui_sync
grep -F "Edit Annotation pick plane at" "$UI_LOG" | tail -1 | grep -qF "(disabled)" ||
  ui_fail "Pick… (the plane) enabled for an angle"
echo "ok   Pick… disabled for an angle"
ui_focus_dialog '^Edit Annotation$'
ui_step "Kind: Distance (Alt+K, Home)"     ui_key alt+k Home
wait_enabled "Edit Annotation pick plane"
echo "ok   Pick… enabled for a distance"
ui_step "Kind: Angle (Alt+K, End)"         ui_key alt+k End
wait_disabled "Edit Annotation pick plane"
echo "ok   Pick… disabled again for the angle"
ui_mark
ui_step "cancel (Escape)"                  ui_key Escape
tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qF "Edit Annotation dialog: " &&
  ui_fail "the cancelled editor changed Tilt"
focus_window
ui_step "close the window (Escape)"        ui_key Escape
ui_focus_main

echo "--- Flat taken from under Rise onto a leader"
ui_wait_idle
# Flat's cells clear of Tilt's text (on the far side of the block), which
# is drawn after it and would take the click.
read -r fx fy <<< "$(logged_place "Annotation label Flat")"
read -r tx ty <<< "$(logged_place "Annotation label Tilt")"
[ -n "$fx" ] && [ -n "$tx" ] || ui_fail "no places of Flat's and Tilt's labels"
[ $(((fx - tx) * (fx - tx) + (fy - ty) * (fy - ty))) -gt 1600 ] ||
  ui_fail "Flat's label at $fx,$fy overlaps Tilt's at $tx,$ty"
echo "ok   Flat's label at $fx,$fy, Tilt's at $tx,$ty"
ui_double_click_expect "Annotation label Flat" "Edit Annotation dialog opened: Flat frame" \
  "a double-click on Flat's cells opens its editor"
ui_sync
# Under a dimension: no pick or plane row (Placed says where it stands).
tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qF "Edit Annotation pick " &&
  ui_fail "a pick or plane row shown for a frame under a dimension"
echo "ok   no pick or plane row under the dimension"
ui_focus_dialog '^Edit Annotation$'
ui_step "Placed: On its leader (Alt+P, Home)" ui_key alt+p Home
wait_enabled "Edit Annotation pick a"
echo "ok   Points at shown, Pick Again… enabled"
ui_mark
ui_step "OK (Enter) without a pick"        ui_key Return
ui_focus_main
# Its pick is the origin, which a leader cannot point at: the pick first,
# not an edit the model refuses.
ui_expect_new 'Edit Annotation dialog: {} (pick a with {"on":null})' "the pick asked for first"
ui_expect_new 'Annotation pick started: Flat a with {"on":null}' "the view waits for the leader's pick"
ui_expect_new "Pick places:" "the places to pick logged"
tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qF "Annotation command refused" &&
  ui_fail "an edit was refused: $(grep -F "Annotation command refused" "$UI_LOG" | tail -1)"
ui_mark
ui_step "pick the block's top face"        ui_click_pick "face F2.b0/F2:end("
ui_expect_new "Annotation picked: Flat a: face F2:end(" "the top face picked"
ui_expect_new "Edit Annotation dialog opened: Flat frame" "its editor open again"
grep -F "Annotation command: " "$UI_LOG" | tail -1 | grep -qF '"on":null' ||
  ui_fail "the pick went without on: null: $(grep -F "Annotation command: " "$UI_LOG" | tail -1)"
echo "ok   on: null sent with the new pick in one edit"
# On its leader now: its pick and its plane shown.
ui_expect_new "Edit Annotation pick plane at" "its plane's row shown on the leader"
ui_focus_dialog '^Edit Annotation$'
ui_step "close the editor (Escape)"        ui_key Escape
ui_focus_main
ui_mark
ui_step "save (Ctrl+S)"                    ui_key ctrl+s
ui_expect_new "Saved $WORK/rows.mitcad" "saved"
grep -qF '"on": "Rise"' "$WORK/rows.mitcad" && ui_fail "Flat still under Rise in the design file"
echo "ok   Flat on its leader in the design file"

echo "--- A light bulb far down the list: the list stays where it is (mitcad#759)"
# A 20 mm block with 40 distances from the origin to its top face: more
# rows than the window shows.
{
  printf '[\n  {"cmd": "sketch.create"},\n'
  printf '  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 20, "height": 20},\n'
  printf '  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "%s"}],\n' "$R"
  printf '    "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}}'
  for i in $(seq -w 1 40); do
    printf ',\n  {"cmd": "add_annotation", "name": "Row%s", "a": {"datum": "origin"}, "b": {"body": "F2.b0", "face": "F2:end(%s)"}}' \
      "$i" "$R"
  done
  printf '\n]\n'
} > "$WORK/many.json"
"$CLI" run "$WORK/many.json" --save "$WORK/many.mitcad" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli (40 annotations)"; }
ui_stop_app
ui_start_app --open "$WORK/many.mitcad"
ui_expect_log "many.mitcad" "the design opened"
ui_mark
ui_step "Annotations (search)"             ui_command "Annotations"
ui_expect_new "Annotations window opened: 40 annotation(s)" "the window lists the 40"
ui_expect_new "Annotations row Row01 at" "the first rows in sight"
ui_mark
focus_window
# Ctrl+End moves the current row without selecting (the table's extended
# selection); the first one only makes Row01 current when no row is.
# Home then goes to its first column and selects the row (Space would
# toggle the last column's light bulb).
ui_step "the last row (Ctrl+End, Home)"    ui_key ctrl+End ctrl+End Home
ui_expect_new "Annotations selected: Row40" "the last row selected"
ui_expect_new "Annotations row Row40 at" "the list scrolled down to it"
# The places after the scroll, settled: the last logged stays over a sync.
for _ in $(seq 1 20); do
  before=$(grep -F "Annotations shown Row38 at " "$UI_LOG" | tail -1)
  ui_sync
  sleep 0.3
  after=$(grep -F "Annotations shown Row38 at " "$UI_LOG" | tail -1)
  [ -n "$after" ] && [ "$before" = "$after" ] && break
done
read -r bx by <<< "$(logged_place "Annotations shown Row38")"
[ -n "$bx" ] || ui_fail "Row38's light bulb is not in sight after the scroll"
ui_mark
ui_step "Row38's light bulb off"           ui_click_logged "Annotations shown Row38"
for _ in $(seq 1 30); do
  tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qE "^Annotations row Row38: .*, hidden$" && break
  sleep 0.2
done
tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qE "^Annotations row Row38: .*, hidden$" ||
  ui_fail "Row38 was not hidden by its light bulb"
echo "ok   Row38 hidden"
ui_sync
tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qF "Annotations row Row01 at " &&
  ui_fail "the list jumped back to its top after the refresh"
read -r ax ay <<< "$(logged_place "Annotations shown Row38")"
[ "$ax,$ay" = "$bx,$by" ] || ui_fail "Row38's row moved from $bx,$by to $ax,$ay"
echo "ok   the list kept its place: Row38's light bulb still at $ax,$ay"
focus_window
ui_step "close the window (Escape)"        ui_key Escape
ui_focus_main

echo "--- A hole callout from the Measure panel (mitcad#902)"
# A 60 x 40 x 10 plate with four counterbored through holes (the hole
# feature F3): Hole Callout on one hole's counterbore writes all four.
cat > "$WORK/holes.json" << EOF
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 60, "height": 40},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$R"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "hole",
    "placement": {"type": "face", "body": "F2.b0", "face": "F2:end($R)",
                  "points": [[10, 10, 10], [50, 10, 10], [10, 30, 10], [50, 30, 10]]},
    "diameter": 6.6, "extent": {"type": "through_all"},
    "kind": {"type": "counterbore", "diameter": 11, "depth": 6.8}}}
]
EOF
"$CLI" run "$WORK/holes.json" --save "$WORK/holes.mitcad" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli (the plate)"; }
ui_stop_app
ui_start_app --open "$WORK/holes.mitcad"
ui_expect_log "Annotations: none" "the plate opened without annotations"
ui_step "fit (F6)"                         ui_key F6
ui_mark
ui_step "measure (I)"                      ui_key i
ui_expect_new "Command Measure started" "measure started"
ui_step "pick a hole's counterbore wall"   ui_click_pick "face F2.b0/F3:hole0.cbore_wall"
wait_enabled "Measure Hole Callout"
ui_mark
ui_step "Hole Callout"                     ui_click_logged "Measure Hole Callout"
ui_expect_new "Measure Hole Callout clicked" "Hole Callout clicked"
ui_expect_new "Annotation kept: HoleCallout1" "the hole callout added"
ui_expect_new "Annotations drawn: HoleCallout1 = 4x Ø6.6 THRU / ⌴ Ø11 ↧ 6.8 (auto)" \
  "written from the hole's feature: the count, THRU, the counterbore"
ui_expect_log "Annotation lettering: " "the annotations' lettering chosen"
# The bundled lettering when the build has it (third_party/fonts/iso-lettering).
if [ -e "$(dirname "$UI_APP")/fonts/lettering.ttf" ]; then
  grep -F "Annotation lettering: " "$UI_LOG" | head -1 | grep -qF "fonts/lettering.ttf" ||
    ui_fail "not the bundled lettering: $(grep -F "Annotation lettering: " "$UI_LOG" | head -1)"
  echo "ok   drawn in the bundled technical lettering"
fi
ui_expect_new "Annotation label HoleCallout1 at" "its text placed in the view"
ui_step "close (Enter)"                    ui_key Return
ui_expect_new "Closed Measure" "measure closed"
ui_mark
ui_step "save (Ctrl+S)"                    ui_key ctrl+s
ui_expect_new "Saved $WORK/holes.mitcad" "saved"
grep -qF '"kind": "hole_callout"' "$WORK/holes.mitcad" || ui_fail "the design file has no hole callout"
echo "ok   the hole callout in the design file"

grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
ui_finish "UI annotations test"
