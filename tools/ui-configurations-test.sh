#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser/ConfigurationsDialog.cpp app/browser/ConfigurationBar.cpp app/browser/ConfigurationsData.cpp app/browser/BrowserPanel.cpp app/browser/BrowserController.cpp
# check-all sources: app/MainWindow.cpp tools/cli core/model-base/src/configurations.rs core/model-base/src/configurations
# check-all sources: core/model/src/document/configurations.rs core/model-api/src/configurations.rs
# check-all sources: app/configurations/RulesPage.cpp app/configurations/RuleCanvas.cpp
# Configurations (mitcad#807) through the real UI, on a design mitcad-cli
# makes: a text parameter Color ('Black' or 'White', White), Length
# (300 mm … 2000 mm, step 10 mm, 1500 mm) and Extra (0 mm): a 20 x 20 mm
# extrusion takes Length + Extra as its distance; no configuration table
# yet.
#   1. Configurations…: each tab's lead line and the empty states of the
#      Options list, the Rows sheet and the Rules canvas (mitcad#967) are
#      logged; Add Option makes Color and then Length options.
#   2. Rows: Add Row (Row1, the current values), its Length edited to 800
#      mm, Apply: the body is 800 mm long.
#   3. The Configuration bar in the browser: Length = 1200 in its field;
#      the body follows (1200 mm).
#   4. The rule "Black is short" (when Color == 'Black', Length at most
#      1000 mm) set by a command (mitcad-cli set_configurations on the saved
#      design); opened again, the bar's Color set to Black moves Length
#      from 1200 to 1000 mm, the notice names the rule, the body follows and
#      the bar's range ends at 1000 mm.
#   5. Rows: a new row (Black, 1000 mm) whose Length is typed as 1500 is
#      refused with the rule's name.
#   6. Rows: Add Column… (mitcad#999) makes Extra a column of the table,
#      which becomes closed, each row's value 0 mm; Row1's Extra typed as
#      100 and Row1 applied: the body is 900 mm long. Remove Column takes
#      it out again, and the parameter stays (Add Column offers it).
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-configurations-test.sh [screenshot.png]

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-configurations.XXXXXX)
FILE=$WORK/shelf.mitcad
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT

R='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'
cat > "$WORK/model.json" << EOF
[
  {"cmd": "add_parameter", "name": "Color", "unit": "Text", "expression": "'White'",
   "values": ["'Black'", "'White'"]},
  {"cmd": "add_parameter", "name": "Length", "unit": "mm", "expression": "1500 mm",
   "min": "300 mm", "max": "2000 mm", "step": "10 mm"},
  {"cmd": "add_parameter", "name": "Extra", "unit": "mm", "expression": "0 mm"},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 20, "height": 20},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$R"}],
    "extent": {"type": "distance", "distance": "Length + Extra"}, "operation": "new_body"}}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# The rule, set by a command on the saved design (the table as the UI made
# it, with the rule).
cat > "$WORK/rule.json" << 'EOF'
[
  {"cmd": "set_configurations", "configurations": {
    "options": ["Color", "Length"], "closed": false,
    "rows": [{"name": "Row1", "values": {"Color": "'White'", "Length": "800 mm"}}],
    "rules": [{"name": "Black is short", "when": "Color == 'Black'",
               "then": [{"option": "Length", "max": "1000 mm"}]}]}}
]
EOF

# expect_volume expected description: the last body volume change logged
# since the mark.
expect_volume() {
  local volume
  for _ in $(seq 1 50); do
    volume=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" |
      sed -n 's/.*Body Body1 (F2.b0): volume [0-9.]* -> \([0-9.]*\) mm3.*/\1/p' | tail -1)
    [ -n "$volume" ] && break
    sleep 0.2
  done
  [ -n "$volume" ] || ui_fail "$2: no volume logged"
  awk -v v="$volume" -v e="$1" 'BEGIN { exit !((v - e) ^ 2 < (1e-6 * e) ^ 2 + 1e-4) }' ||
    ui_fail "$2: volume $volume mm3, expected $1"
  echo "ok   $2: $volume mm3"
}

# edit_row_cell "row column" text: double-click a cell of the Rows sheet,
# replace its text, Enter.
edit_row_cell() {
  ui_focus_dialog '^Configurations$'
  ui_double_click_logged "Configurations row $1"
  ui_sync
  ui_key ctrl+a
  xdotool type --delay 40 "$2"
  ui_key Return
}

# add_option: Add Option... and OK on the first user parameter offered.
add_option() {
  ui_focus_dialog '^Configurations$'
  ui_click_logged "Configurations add option"
  ui_focus_dialog '^Add Option$'
  ui_key Return
}

# open_add_column: Add Column... (mitcad#999), the dialog focused.
open_add_column() {
  ui_focus_dialog '^Configurations$'
  ui_click_logged "Configurations add column"
  ui_focus_dialog '^Add Column$'
}

ui_start_display
ui_start_app --open "$FILE" --no-native-dialogs
ui_expect_log "Browser Root at" "the design is open"

echo "--- 1. Two options"
ui_mark
ui_step "Configurations (search)"        ui_command "Configurations"
ui_expect_new "Configurations window opened" "the window opened"
ui_expect_new "Configurations options: none" "no options yet"
# The window explains itself (mitcad#967): a lead line per tab, and what an
# empty list, sheet or canvas shows in its place.
ui_expect_new "Configurations lead Options: Options are the choices a user of this design makes, such as Color or Length. Each option is a user parameter." \
  "the Options tab's lead line"
ui_expect_new "Configurations lead Rows: A row is a ready-made combination of the options, with its designation and part number." \
  "the Rows tab's lead line"
ui_expect_new "Configurations lead Rules: Rules take away values and combinations that are not allowed. Most designs need none." \
  "the Rules tab's lead line"
ui_expect_new "Configurations empty Options: No options yet. Add Option… makes a parameter an option." \
  "the empty Options list says how to add one"
ui_expect_new "Configurations empty Rows: No rows yet. Add Row saves the options' values as a row." \
  "the empty Rows sheet says how to add one"
ui_expect_new "Configurations empty Rules: No rules yet. A rule is, for example: when Color is Black, Length is at most 1000 mm." \
  "the empty Rules canvas says what a rule is"
ui_expect_new "Configurations add option at" "Add Option's place is logged"
ui_mark
ui_step "Add Option: Color"              add_option
ui_expect_new "Add Option dialog opened: Color, Length, Extra, New Parameter" "the user parameters are offered"
ui_expect_new "Configurations added option Color" "Color is an option"
ui_mark
ui_step "Add Option: Length"             add_option
ui_expect_new "Add Option dialog opened: Length, Extra, New Parameter" "Color is no longer offered"
ui_expect_new "Configurations added option Length" "Length is an option"
ui_expect_new "Configurations options: Color (Black, White) = White; Length (300 mm" \
  "two options in their order, with their domains"
ui_expect_new "Configuration bar: Color = White (Black | White); Length = 1500 mm" \
  "the browser's Configuration bar shows them"

echo "--- 2. A row, edited and applied"
ui_mark
ui_focus_dialog '^Configurations$'
ui_step "the Rows tab"                   ui_click_logged "Configurations tab Rows"
ui_expect_new "Configurations page Rows" "the Rows page"
ui_expect_new "Configurations add row at" "Add Row's place is logged"
ui_mark
ui_step "Add Row"                        ui_click_logged "Configurations add row"
ui_expect_new "Configurations added row: Row1" "Row1 added"
ui_expect_new "current Row1: Row1: Color = White, Length = 1500 mm" "Row1 has the current values"
ui_expect_new "Configurations row Row1 Length at" "its cells' places are logged"
ui_mark
ui_step "Row1 Length = 800"              edit_row_cell "Row1 Length" "800"
ui_expect_new "Configurations row Row1 Length: 800" "the cell is a model command"
ui_expect_new "current none: Row1: Color = White, Length = 800 mm" "Row1 keeps 800 mm"
ui_mark
ui_focus_dialog '^Configurations$'
ui_step "select Row1"                    ui_click_logged "Configurations row Row1 Name"
ui_step "Apply"                          ui_click_logged "Configurations apply"
ui_expect_new "Configurations applied Row1" "Row1 applied"
expect_volume 320000 "20 x 20 x 800"
ui_expect_new "Configuration bar: Color = White (Black | White); Length = 800 mm" "the bar shows the row's values"
[ -n "$SHOT" ] && ui_capture "$SHOT"
ui_focus_dialog '^Configurations$'
ui_step "close the window (Esc)"         ui_key Escape

echo "--- 3. The bar: Length typed, the body follows"
ui_focus_main
ui_mark
ui_step "the bar's Length field"         ui_click_logged "Configuration bar Length"
ui_sync
ui_key ctrl+a
xdotool type --delay 40 "1200"
ui_key Return
ui_expect_new "Configuration chosen: Length = 1200" "the field's value is chosen"
expect_volume 480000 "20 x 20 x 1200"
ui_expect_new "Configuration bar: Color = White (Black | White); Length = 1200 mm" "the bar shows 1200 mm"
ui_focus_main
ui_mark
ui_step "save (Ctrl+S)"                  ui_key ctrl+s
ui_expect_new "Saved $FILE" "saved"

echo "--- 4. A rule set by a command: its move is a notice"
ui_stop_app
"$CLI" run "$WORK/rule.json" --open "$FILE" --save "$FILE" > "$WORK/rule.log" 2>&1 ||
  { cat "$WORK/rule.log"; ui_fail "mitcad-cli set_configurations with the rule"; }
ui_start_app --open "$FILE" --no-native-dialogs
ui_mark
# set_configurations ran the cascade: 1200 mm is allowed while White.
ui_expect_log "Configuration bar: Color = White (Black | White); Length = 1200 mm" "the bar after the command"
ui_expect_log "Configuration bar Color at" "the Color chooser's place is logged"
ui_step "the bar's Color: Black"         ui_choose "Configuration bar Color" -1
ui_expect_new "Configuration chosen: Color = Black" "Black chosen"
ui_expect_new "Configuration moves: Length 1200 mm → 1000 mm: Black is short" \
  "the cascade's move is a notice that names the rule"
expect_volume 400000 "20 x 20 x 1000"
ui_expect_new "Configuration bar: Color = Black (Black | White); Length = 1000 mm (300 mm … 1000 mm" \
  "the bar offers only what the rule allows"

echo "--- 5. A value the rule refuses in the Rows sheet"
ui_focus_main
ui_mark
ui_step "Configurations (search)"        ui_command "Configurations"
ui_expect_new "Configurations window opened" "the window opened"
ui_focus_dialog '^Configurations$'
ui_step "the Rows tab"                   ui_click_logged "Configurations tab Rows"
ui_expect_new "Configurations add row at" "the Rows page"
ui_mark
ui_step "Add Row"                        ui_click_logged "Configurations add row"
ui_expect_new "Configurations added row: Row2" "Row2 added"
ui_expect_new "Row2: Color = Black, Length = 1000 mm" "Row2 has the current values"
ui_expect_new "Configurations row Row2 Length at" "its cells' places are logged"
ui_mark
ui_step "Row2 Length = 1500"             edit_row_cell "Row2 Length" "1500"
ui_expect_new "Configurations row Row2 Length: 1500 refused: " "the value is refused"
ui_expect_new "Black is short" "with the rule's name"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"

echo "--- 6. A parameter column (mitcad#999): added, a row's value typed, applied, removed"
ui_mark
ui_step "Add Column…"                    open_add_column
ui_expect_new "Add Column dialog opened: Extra, New Parameter" "the user parameters that are no option are offered"
ui_step "OK: Extra"                      ui_key Return
ui_expect_new "Configurations: column Extra added" "Extra is a column"
ui_expect_new "Configurations rows: closed, " "the table is closed with it"
ui_expect_new "Row1: Color = White, Length = 800 mm, Extra = 0 mm; Row2: Color = Black, Length = 1000 mm, Extra = 0 mm" \
  "each row has Extra's current value"
ui_expect_new "Configurations row Row1 Extra at" "the column's cells' places are logged"
ui_mark
ui_step "Row1 Extra = 100"               edit_row_cell "Row1 Extra" "100"
ui_expect_new "Configurations row Row1 Extra: 100" "the cell is a model command"
ui_expect_new "Row1: Color = White, Length = 800 mm, Extra = 100 mm" "Row1 sets Extra to 100 mm"
ui_mark
ui_focus_dialog '^Configurations$'
ui_step "select Row1"                    ui_click_logged "Configurations row Row1 Name"
ui_step "Apply"                          ui_click_logged "Configurations apply"
ui_expect_new "Configurations applied Row1" "Row1 applied"
expect_volume 360000 "20 x 20 x (800 + 100): the design follows the row's Extra"
ui_mark
ui_focus_dialog '^Configurations$'
ui_step "a cell of Extra"                ui_click_logged "Configurations row Row1 Extra"
ui_step "Remove Column"                  ui_click_logged "Configurations remove column"
ui_expect_new "Configurations: column Extra removed" "the column is out"
ui_expect_new "Row1: Color = White, Length = 800 mm; Row2: Color = Black, Length = 1000 mm" \
  "the rows no longer set Extra"
ui_mark
ui_step "Add Column… again"              open_add_column
ui_expect_new "Add Column dialog opened: Extra, New Parameter" "the parameter stays: it is offered again"
ui_step "cancel (Esc)"                   ui_key Escape
tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -q "Configurations: column .* added" &&
  ui_fail "the cancelled Add Column added a column"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"

ui_finish "UI configurations test"
