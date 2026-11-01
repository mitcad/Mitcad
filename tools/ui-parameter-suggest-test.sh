#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/ParameterField.cpp app/framework/ParameterField.hpp
# check-all sources: app/framework/ParameterSuggest.cpp app/framework/ParameterSuggest.hpp
# check-all sources: app/framework/CommandPanel.cpp app/sketch/SketchController.cpp
# check-all sources: app/browser/ParametersDialog.cpp app/MainWindow.cpp tools/cli
# Parameter suggestions in expression fields and the fx browser
# (mitcad#567), through the real UI:
#   - A design (mitcad-cli): the user parameters width (30 mm), thickness
#     (4 mm, "Wall thickness") and thickness2 (6 mm); Sketch1 (F1) with a
#     40 x 25 rectangle (its width k5 d1, its height k6 d2) and a free line
#     from (0, 40) to (30, 40); Extrude1 (F2) 10 of the rectangle (d3).
#   - Edit Extrude1: `width / 2 + thi` lists thickness and thickness2;
#     Enter puts thickness in place of `thi` without confirming the panel
#     (19 mm); ` + th` lists the similar names, Esc closes the list only,
#     `ickness2` is typed on; OK keeps 25 mm (volume 25000 mm3).
#   - Edit Extrude1 again: `2 * `, then fx without typing a name; `wall`
#     finds thickness by its comment; a click puts it in after `2 * `
#     (8 mm); OK (volume 8000 mm3).
#   - Edit Sketch1: k5's existing dimension completed to width + thickness
#     (34 mm); a new dimension on the free line: `2 * thicknes`, Down to
#     thickness2, Enter, Enter: 2 * thickness2.
#   - Edit Parameters: d3's expression completed to 3 * thickness (12
#     mm); thickness2 renamed rim; d3's editor then lists rim, says when
#     nothing matches, and Esc closes the list before the cell.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-parameter-suggest-test.sh [screenshot.png]

source "$(dirname "$0")/ui-test-lib.sh"

# fit_view: fits the view (F6), then zooms out a little: a fit to the
# small rectangle leaves its dimensions' values outside the view.
fit_view() {
  ui_key F6
  ui_sync
  for _ in 1 2 3 4 5 6; do
    xdotool mousemove $(ui_view_at 50 50) click 5
    ui_sync
  done
}

SHOT=${1:-}
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-parameter-suggest.XXXXXX)
FILE=$WORK/suggest.mitcad
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT

RECT='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'
cat > "$WORK/model.json" << EOF
[
  {"cmd": "add_parameter", "name": "width", "expression": "30 mm", "unit": "mm", "comment": ""},
  {"cmd": "add_parameter", "name": "thickness", "expression": "4 mm", "unit": "mm", "comment": "Wall thickness"},
  {"cmd": "add_parameter", "name": "thickness2", "expression": "6 mm", "unit": "mm", "comment": ""},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 40, "height": 25},
  {"cmd": "sketch.set_dimension_text", "sketch": "F1", "dimension": "k5", "text": [20, -10]},
  {"cmd": "sketch.set_dimension_text", "sketch": "F1", "dimension": "k6", "text": [50, 12]},
  {"cmd": "sketch.add_line", "sketch": "F1", "start": [0, 40], "end": [30, 40]},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$RECT"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# since_mark "text": whether a line logged after the mark has the text.
since_mark() { tail -n +$((${UI_MARK:-0} + 1)) "$UI_LOG" | grep -qF -- "$1"; }

ui_start_display
ui_start_app --open "$FILE"
ui_step "fit (F6), zoom out"     fit_view
ui_expect_log "Timeline Extrude1 at" "the timeline is logged"
grep -q "Failed features: [^n]" "$UI_LOG" && ui_fail "a feature failed"

echo "--- A feature field: completion after an operator, Enter and Esc"
ui_mark
ui_step "edit Extrude1 (double-click)"   ui_double_click_logged "Timeline Extrude1"
ui_expect_new "Editing F2 with Extrude" "Extrude1 opens for editing" 30
ui_step "type width / 2 + thi"           ui_type_in "Panel Extrude input distance" "width / 2 + thi"
ui_expect_new 'Parameter suggestions for Extrude Distance "thi": thickness, thickness2' \
  "the names that start with thi, user parameters, in order"
ui_mark
ui_step "Enter (the list's)"             ui_key Return
ui_expect_new "Parameter suggestion inserted thickness into Extrude Distance: width / 2 + thickness" \
  "thickness in place of thi, the rest kept"
ui_expect_new "Extrude Distance: width / 2 + thickness = 19" "the distance follows"
ui_sync
since_mark "Edited F2" && ui_fail "Enter in the list confirmed the panel"
echo "ok   the panel stays open"
ui_mark
ui_step "type + th"                      xdotool type --delay 60 " + th"
ui_expect_new 'Parameter suggestions for Extrude Distance "th": thickness, thickness2, width' \
  "similar names, then a name that contains th"
ui_step "Esc (the list's)"               ui_key Escape
ui_expect_new "Parameter suggestions closed for Extrude Distance (Esc)" "Esc closed the list"
ui_sync
since_mark "Command Extrude cancelled" && ui_fail "Esc in the list cancelled the panel"
ui_step "type ickness2"                  xdotool type --delay 60 "ickness2"
ui_expect_new "Extrude Distance: width / 2 + thickness + thickness2 = 25" "typed on in the same panel"
ui_mark
ui_step "OK (Enter)"                     ui_key Return
ui_expect_new "Edited F2" "Extrude1 edited" 30
ui_expect_volume "Body Body1 (F2.b0): volume [0-9.]* -> \([0-9.]*\) mm3" 25000 "40 x 25 x 25 mm"

echo "--- The fx browser: no name typed, found by its comment"
ui_mark
ui_step "edit Extrude1 (double-click)"   ui_double_click_logged "Timeline Extrude1"
ui_expect_new "Editing F2 with Extrude" "Extrude1 opens for editing again" 30
ui_step "type 2 * "                      ui_type_in "Panel Extrude input distance" "2 * "
ui_expect_new "Parameter fx Extrude Distance at" "the field's fx button is logged"
ui_mark
ui_step "click fx"                       ui_click_logged "Parameter fx Extrude Distance"
ui_expect_new "Parameter browser opened for Extrude Distance" "the browser opened"
ui_expect_new 'Parameter suggestions for Extrude Distance "": thickness, thickness2, width, d1, d2' \
  "every parameter, the user's first"
ui_step "type wall"                      xdotool type --delay 60 "wall"
ui_expect_new 'Parameter suggestions for Extrude Distance "wall": thickness' "thickness found by its comment"
ui_expect_new "Parameter suggestion thickness at" "its row is logged"
ui_step "click thickness"                ui_click_logged "Parameter suggestion thickness"
ui_expect_new "Parameter browser inserted thickness into Extrude Distance: 2 * thickness" \
  "put in after the pending 2 * "
ui_expect_new "Extrude Distance: 2 * thickness = 8" "the distance follows"
[ -n "$SHOT" ] && ui_capture "$SHOT"
ui_mark
ui_step "OK (Enter)"                     ui_key Return
ui_expect_new "Edited F2" "Extrude1 edited again" 30
ui_expect_volume "Body Body1 (F2.b0): volume [0-9.]* -> \([0-9.]*\) mm3" 8000 "40 x 25 x 8 mm"

echo "--- Sketch dimensions: an existing one and a new one"
ui_mark
ui_step "edit Sketch1 (double-click)"    ui_double_click_logged "Timeline Sketch1"
ui_expect_new "Editing sketch F1 (Sketch1)" "Sketch1 is edited"
ui_step "fit (F6), zoom out"     fit_view
ui_expect_new "Dimension k5 text at" "the width's value placed"
ui_mark
ui_double_click_expect "Dimension k5 text" "Editing dimension k5" "the width's value editor"
ui_expect_new "Sketch value editor at" "it opened"
ui_step "type width + th"                xdotool type --delay 60 "width + th"
ui_expect_new 'Parameter suggestions for dimension d1 "th": thickness, thickness2, width' \
  "the dimension's editor lists them"
ui_step "Enter (the list's)"             ui_key Return
ui_expect_new "Parameter suggestion inserted thickness into dimension d1: width + thickness" "thickness put in"
ui_step "Enter"                          ui_key Return
ui_expect_new "Dimension k5 = width + thickness (d1)" "the width is width + thickness"
ui_mark
ui_step "sketch dimension (D)"           ui_key d
ui_step "pick the free line"             ui_sketch_click 15 40
ui_step "place its value"                ui_sketch_click 15 48
ui_expect_new "Sketch value editor at" "the new dimension's editor"
ui_step "type 2 * thicknes"              xdotool type --delay 60 "2 * thicknes"
ui_expect_new 'Parameter suggestions for sketch dimension "thicknes": thickness, thickness2' "both listed"
ui_step "Down"                           ui_key Down
ui_expect_new "Parameter suggestion highlighted: thickness2" "Down moves to thickness2"
ui_step "Enter (the list's)"             ui_key Return
ui_expect_new "Parameter suggestion inserted thickness2 into sketch dimension: 2 * thickness2" "thickness2 put in"
ui_step "Enter"                          ui_key Return
ui_expect_new "length d4 = 2 * thickness2" "the new dimension is 2 * thickness2"
ui_step "end the tool (Esc)"             ui_key Escape
ui_step "finish sketch (Ctrl+Enter)"     ui_key ctrl+Return
ui_expect_new "Sketch finished" "Sketch1 finished" 30
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"

echo "--- The parameter table's expressions"
ui_focus_main
ui_mark
ui_step "Edit Parameters (search)"     ui_command "Edit Parameters"
ui_expect_new "Parameters dialog opened" "the dialog opened"
ui_expect_new "Parameters d3 expression at" "its cells are logged"
ui_focus_dialog '^Parameters$'
ui_mark
ui_step "edit d3's expression"           ui_double_click_logged "Parameters d3 expression"
ui_sync
ui_key ctrl+a
ui_step "type 3 * thi"                   xdotool type --delay 60 "3 * thi"
ui_expect_new 'Parameter suggestions for d3 expression "thi": thickness, thickness2' "the cell's editor lists them"
ui_step "Enter (the list's)"             ui_key Return
ui_expect_new "Parameter suggestion inserted thickness into d3 expression: 3 * thickness" "thickness put in"
ui_step "Enter (the cell's)"             ui_key Return
ui_expect_new "Parameter d3 expression: 3 * thickness = 12 mm" "d3 is 3 * thickness"
ui_mark
ui_focus_dialog '^Parameters$'
ui_step "rename thickness2 to rim"       ui_double_click_logged "Parameters thickness2 name"
ui_sync
ui_key ctrl+a
xdotool type --delay 40 "rim"
ui_key Return
ui_expect_new "Parameter thickness2 name: rim = 6 mm" "renamed"
ui_mark
ui_focus_dialog '^Parameters$'
ui_step "edit d3's expression"           ui_double_click_logged "Parameters d3 expression"
ui_sync
ui_key ctrl+a
ui_step "type zz"                        xdotool type --delay 60 "zz"
ui_expect_new 'Parameter suggestions for d3 expression "zz": none' "nothing matches zz"
ui_key ctrl+a
ui_step "type ri"                        xdotool type --delay 60 "ri"
ui_expect_new 'Parameter suggestions for d3 expression "ri": rim' "the renamed parameter is offered"
ui_step "Esc (the list's)"               ui_key Escape
ui_expect_new "Parameter suggestions closed for d3 expression (Esc)" "Esc closed the list"
ui_sync
since_mark "Parameter d3 expression: ri" && ui_fail "the cell took the text"
ui_step "Esc (the cell's)"               ui_key Escape
ui_step "close the parameters (Esc)"     ui_key Escape
ui_focus_main
since_mark "Parameter d3 expression: " && ui_fail "a cancelled cell edit changed d3"
echo "ok   the cancelled edit left d3 as it was"

grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
ui_finish "UI parameter suggestions test"
