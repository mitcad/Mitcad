#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/ParameterField.cpp app/framework/ParameterField.hpp
# check-all sources: app/framework/ParameterSuggest.cpp app/framework/ParameterSuggest.hpp
# check-all sources: app/framework/CommandPanel.cpp app/sketch/SketchController.cpp
# check-all sources: app/browser/ParametersDialog.cpp app/MainWindow.cpp tools/cli
# Inline names and new user parameters from expression fields
# (mitcad#568), through the real UI:
#   - A design (mitcad-cli) without user parameters: Sketch1 (F1) with a
#     40 x 25 rectangle (its width k5 d1, its height k6 d2); Extrude1 (F2)
#     10 of it (d3).
#   - Sketch1, the width: `width` typed names nothing, so the list offers
#     `width = …` (the dimension's own parameter) and Create User
#     Parameter; the first gives `width = `, then 30 mm: the width's
#     parameter is width (the existing form, mitcad#231).
#   - The height: `wid` lists width; Enter, Enter: the height uses width.
#   - The height again: `gap`, Down to Create User Parameter, Enter: the
#     editor proposes gap; 15 deg in mm is refused with the model's reason,
#     12 is taken: gap (12 mm) is a user parameter and the height uses it.
#   - The height again: `gap2`, Create User Parameter: width (taken) and
#     1x (invalid) are refused, Cancel leaves gap2 in the field.
#   - Undo and Redo in the sketch: the height's change, then Add Parameter
#     gap; Redo both.
#   - Extrude1: `2 * depth`, Create User Parameter, 6: depth is put in, the
#     distance is 2 * depth (12 mm), OK (30 x 12 x 12 mm).
#   - Edit Parameters lists depth and gap; d3's editor offers depth.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-parameter-create-test.sh [screenshot.png]

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
WORK=$(mktemp -d /tmp/mitcad-ui-parameter-create.XXXXXX)
FILE=$WORK/create.mitcad
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT

RECT='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'
cat > "$WORK/model.json" << EOF
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 40, "height": 25},
  {"cmd": "sketch.set_dimension_text", "sketch": "F1", "dimension": "k5", "text": [20, -10]},
  {"cmd": "sketch.set_dimension_text", "sketch": "F1", "dimension": "k6", "text": [50, 12]},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$RECT"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# since_mark "text": whether a line logged after the mark has the text.
since_mark() { tail -n +$((${UI_MARK:-0} + 1)) "$UI_LOG" | grep -qF -- "$1"; }

# edit_height: fits the view (the dimensions' places are logged with it)
# and double-clicks the height's value.
edit_height() {
  ui_mark
  ui_step "fit (F6), zoom out"     fit_view
  ui_sync
  ui_double_click_expect "Dimension k6 text" "Editing dimension k6" "the height's value editor"
  ui_expect_new "Sketch value editor at" "it opened"
}

ui_start_display
ui_start_app --open "$FILE"
ui_step "fit (F6), zoom out"     fit_view
ui_expect_log "Timeline Sketch1 at" "the timeline is logged"
grep -q "Failed features: [^n]" "$UI_LOG" && ui_fail "a feature failed"

echo "--- name = value, discoverable in the list (mitcad#231's form)"
ui_mark
ui_step "edit Sketch1 (double-click)"    ui_double_click_logged "Timeline Sketch1"
ui_expect_new "Editing sketch F1 (Sketch1)" "Sketch1 is edited"
ui_step "fit (F6), zoom out"     fit_view
ui_expect_new "Dimension k5 text at" "the width's value placed"
ui_mark
ui_double_click_expect "Dimension k5 text" "Editing dimension k5" "the width's value editor"
ui_expect_new "Sketch value editor at" "it opened"
ui_step "type width"                     xdotool type --delay 60 "width"
ui_expect_new 'Parameter suggestions for dimension d1 "width": none; actions: name, create' \
  "an unknown name: name this value, or a new user parameter"
ui_step "Enter (name this value)"        ui_key Return
ui_expect_new "Parameter naming for dimension d1: width = " "the field has width = "
ui_step "type 30 mm"                     xdotool type --delay 60 "30 mm"
ui_step "Enter"                          ui_key Return
ui_expect_new "Dimension k5 = width = 30 mm (width)" "the width's parameter is width, 30 mm"

echo "--- width again in a second dimension"
edit_height
ui_step "type wid"                       xdotool type --delay 60 "wid"
ui_expect_new 'Parameter suggestions for dimension d2 "wid": width' "width offered"
ui_step "Enter (the list's)"             ui_key Return
ui_expect_new "Parameter suggestion inserted width into dimension d2: width" "width put in"
ui_step "Enter"                          ui_key Return
ui_expect_new "Dimension k6 = width (width)" "the height uses width"

echo "--- Create User Parameter from a dimension"
edit_height
ui_key ctrl+a
ui_step "type gap"                       xdotool type --delay 60 "gap"
ui_expect_new 'Parameter suggestions for dimension width "gap": none; actions: name, create' "gap names nothing"
ui_step "Down"                           ui_key Down
ui_mark
ui_step "Enter (create)"                 ui_key Return
ui_expect_new "Create User Parameter opened for dimension width: name gap, unit mm" "the editor proposes gap"
ui_focus_dialog '^Create User Parameter$'
ui_mark
ui_step "expression 15 deg"              ui_type_text "15 deg"
ui_key Return
ui_expect_new "Create User Parameter gap refused: " "an angle in millimetres is refused"
since_mark "Created user parameter" && ui_fail "a refused parameter was made"
ui_mark
ui_step "expression 12"                  ui_type_text "12"
ui_key Return
ui_expect_new "Created user parameter gap = 12 mm (mm) for dimension width" "gap made"
ui_expect_new "Parameter created inserted gap into dimension width: gap" "its reference put in"
ui_focus_main
ui_mark
ui_step "Enter"                          ui_key Return
ui_expect_new "Dimension k6 = gap (gap)" "the height uses gap"

echo "--- Taken and invalid names, Cancel"
edit_height
ui_key ctrl+a
ui_step "type gap2"                      xdotool type --delay 60 "gap2"
ui_expect_new 'Parameter suggestions for dimension gap "gap2": none; actions: name, create' "gap2 names nothing"
ui_step "Down"                           ui_key Down
ui_mark
ui_step "Enter (create)"                 ui_key Return
ui_expect_new "Create User Parameter opened for dimension gap: name gap2" "the editor proposes gap2"
ui_focus_dialog '^Create User Parameter$'
ui_step "to the name"                    ui_key shift+Tab
ui_mark
ui_step "name width"                     ui_type_text "width"
ui_key Return
ui_expect_new "Create User Parameter width refused: width exists already" "a taken name is refused"
ui_mark
ui_step "name 1x"                        ui_type_text "1x"
ui_key Return
ui_expect_new "Create User Parameter 1x refused: 1x is not a valid name" "an invalid name is refused"
ui_mark
ui_step "Cancel (Esc)"                   ui_key Escape
ui_expect_new "Create User Parameter cancelled for dimension gap: gap2" "cancelled, gap2 still in the field"
since_mark "Created user parameter" && ui_fail "a parameter was made"
ui_focus_main
ui_step "close the editor (Esc)"         ui_key Escape
ui_expect_new "Sketch value editor closed" "the editor closed"

echo "--- Undo and Redo in the sketch"
ui_mark
ui_step "undo (Ctrl+Z)"                  ui_key ctrl+z
ui_expect_new "Undo: Change Dimension in" "the height's change taken back"
ui_mark
ui_step "undo (Ctrl+Z)"                  ui_key ctrl+z
ui_expect_new "Undo: Add Parameter gap" "then gap"
ui_mark
ui_step "redo (Ctrl+Y)"                  ui_key ctrl+y
ui_expect_new "Redo: Add Parameter gap" "gap again"
ui_mark
ui_step "redo (Ctrl+Y)"                  ui_key ctrl+y
ui_expect_new "Redo: Change Dimension in" "and the height's change"
ui_mark
ui_step "finish sketch (Ctrl+Enter)"     ui_key ctrl+Return
ui_expect_new "Sketch finished" "Sketch1 finished" 30

echo "--- Create User Parameter from a feature field"
ui_mark
ui_step "edit Extrude1 (double-click)"   ui_double_click_logged "Timeline Extrude1"
ui_expect_new "Editing F2 with Extrude" "Extrude1 opens for editing" 30
ui_step "type 2 * depth"                 ui_type_in "Panel Extrude input distance" "2 * depth"
ui_expect_new 'Parameter suggestions for Extrude Distance "depth": none; actions: create' \
  "depth names nothing; not the field's leading name, so only a new parameter"
ui_mark
ui_step "Enter (create)"                 ui_key Return
ui_expect_new "Create User Parameter opened for Extrude Distance: name depth, unit mm" "the editor proposes depth"
ui_focus_dialog '^Create User Parameter$'
ui_step "expression 6"                   ui_type_text "6"
ui_key Return
ui_expect_new "Created user parameter depth = 6 mm (mm) for Extrude Distance" "depth made"
ui_expect_new "Parameter created inserted depth into Extrude Distance: 2 * depth" "the formula kept around it"
ui_focus_main
ui_expect_new "Extrude Distance: 2 * depth = 12" "the distance follows"
[ -n "$SHOT" ] && ui_capture "$SHOT"
ui_mark
ui_step "OK (Enter)"                     ui_key Return
ui_expect_new "Edited F2" "Extrude1 edited" 30
ui_expect_volume "Body Body1 (F2.b0): volume [0-9.]* -> \([0-9.]*\) mm3" 4320 "30 x 12 x 12 mm"

echo "--- The new parameters in the table and in its suggestions"
ui_focus_main
ui_mark
ui_step "Edit Parameters (search)"     ui_command "Edit Parameters"
ui_expect_new "Parameters dialog opened" "the dialog opened"
ui_expect_new "depth = 6 mm (6 mm)" "depth listed"
ui_expect_new "gap = 12 mm (12 mm)" "gap listed"
ui_expect_new "Parameters d3 expression at" "its cells are logged"
ui_focus_dialog '^Parameters$'
ui_mark
ui_step "edit d3's expression"           ui_double_click_logged "Parameters d3 expression"
ui_sync
ui_key ctrl+a
ui_step "type de"                        xdotool type --delay 60 "de"
ui_expect_new 'Parameter suggestions for d3 expression "de": depth' "depth offered in the table"
ui_step "Esc (the list's)"               ui_key Escape
ui_step "Esc (the cell's)"               ui_key Escape
ui_step "close the parameters (Esc)"     ui_key Escape
ui_focus_main

grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
ui_finish "UI parameter create test"
