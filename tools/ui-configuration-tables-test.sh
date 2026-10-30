#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser/ConfigurationsDialog.cpp app/browser/ConfigurationsData.cpp app/configurations
# check-all sources: app/browser/ConfigurationRulesPage.cpp app/framework/ParameterSuggest.cpp tools/cli
# check-all sources: core/model-base/src/configurations.rs core/model-base/src/configurations
# check-all sources: core/model/src/document/configurations.rs core/model/src/document/configuration_tables.rs
# check-all sources: core/model/src/document/tables.rs core/model-api/src/configurations.rs
# Configurations and the design's tables (mitcad#849) through the real UI,
# on a design mitcad-cli makes: the table Screws (Size, dk, Length: M3 5.5
# 10, M4 7 16, M5 8.5 20), a text option Size ('M4', no domain yet) and a
# number option Length (16 mm) that a 10 x 10 mm extrusion takes as its
# distance; no rows, no rules.
#   1. Configurations…, Options: Size selected, Edit Domain…, Table Column
#      (Alt+T; Screws.Size is offered), OK: Size's values are the column.
#   2. Rows: Generate Rows from Table… offers Screws.Size for Size; OK
#      makes the rows M3, M4 and M5 (one undo step).
#   3. Rules: Add › Empty rule (Add offers a table's template too), named
#      "Made sizes"; the palette's combination in table put in its then
#      from the keyboard; its table typed (Screws): the options' columns
#      follow by their names, the rule is saved, and its text shows it.
#   4. Rows: M5 applied: Size is M5, and the rule moves Length to the only
#      length of M5 in the table (16 → 20 mm); the body follows.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-configuration-tables-test.sh [screenshot.png]

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-configuration-tables.XXXXXX)
FILE=$WORK/screw.mitcad
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT

R='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'
cat > "$WORK/model.json" << EOF
[
  {"cmd": "set_table", "name": "Screws", "table": {
    "columns": [{"name": "Size", "unit": "Text"}, {"name": "dk", "unit": "mm"}, {"name": "Length", "unit": "mm"}],
    "rows": [["M3", 5.5, 10], ["M4", 7, 16], ["M5", 8.5, 20]]}},
  {"cmd": "add_parameter", "name": "Size", "unit": "Text", "expression": "'M4'"},
  {"cmd": "add_parameter", "name": "Length", "unit": "mm", "expression": "16 mm"},
  {"cmd": "add_parameter", "name": "HeadDia", "unit": "mm", "expression": "XLOOKUP(Size; Screws.Size; Screws.dk)"},
  {"cmd": "set_configurations", "configurations": {"options": ["Size", "Length"]}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 10, "height": 10},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$R"}],
    "extent": {"type": "distance", "distance": "Length"}, "operation": "new_body"}}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

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

ui_start_display
ui_start_app --open "$FILE" --no-native-dialogs
ui_expect_log "Browser Root at" "the design is open"

echo "--- 1. A table column as Size's domain"
ui_mark
ui_step "Configurations (search)"        ui_command "Configurations"
ui_expect_new "Configurations window opened" "the window opened"
ui_expect_new "Configurations option Size option at" "the options' places are logged"
ui_focus_dialog '^Configurations$'
ui_mark
ui_step "select Size"                    ui_click_logged "Configurations option Size option"
ui_step "Edit Domain…"                   ui_click_logged "Configurations edit domain"
ui_expect_new "Option Domain dialog opened for Size" "the domain of Size"
ui_focus_dialog '^Option Domain$'
ui_step "Table Column (Alt+T)"           ui_key alt+t
ui_step "OK (Enter)"                     ui_key Return
ui_expect_new "Option Size domain: column Screws.Size" "the column is Size's domain"
ui_expect_new "Configurations options: Size (Screws.Size: M3, M4, M5) = M4" \
  "the Options page shows the column and its values"

echo "--- 2. Generate Rows from Table"
ui_focus_dialog '^Configurations$'
ui_mark
ui_step "the Rows tab"                   ui_click_logged "Configurations tab Rows"
ui_expect_new "Configurations page Rows" "the Rows page"
ui_expect_new "Configurations generate rows at" "Generate Rows from Table's place is logged"
ui_mark
ui_step "Generate Rows from Table…"      ui_click_logged "Configurations generate rows"
ui_expect_new "Generate Rows dialog opened: Screws.Size for Size" \
  "the column Size's values come from, and Size, are offered"
ui_focus_dialog '^Generate Rows from Table$'
ui_step "Generate (Enter)"               ui_key Return
ui_expect_new "Configurations generated rows from Screws.Size for Size: 3 rows from Screws: added M3, M4, M5" \
  "a row per size"
ui_expect_new "M3: Size = M3, Length = ; M4: Size = M4, Length = ; M5: Size = M5, Length = " "the Rows sheet shows them"
ui_expect_new "Configurations row M5 Name at" "their cells' places are logged"

echo "--- 3. A combination in table rule, from the keyboard"
ui_focus_dialog '^Configurations$'
ui_mark
ui_step "the Rules tab"                  ui_click_logged "Configurations tab Rules"
ui_expect_new "Configurations page Rules" "the Rules page"
ui_expect_new "Rules add at" "the page's buttons are logged"
ui_mark
ui_step "Add"                            ui_click_logged "Rules add"
ui_expect_new "Only the combinations of a table… | Empty rule" "Add offers its templates, a table's among them"
ui_sync
ui_step "Empty rule (E)"                 ui_key e
ui_expect_new "Rules saved: Add rule Rule 1" "a new rule, saved"
ui_expect_new "Rules naming Rule 1" "its name to be typed"
ui_step "type its name"                  xdotool type --delay 40 "Made sizes"
ui_step "Enter"                          ui_key Return
ui_expect_new "Rules saved: Rename rule Rule 1 to Made sizes" "renamed"
ui_mark
ui_step "Alt+P (the palette)"            ui_key alt+p
ui_step "combination (type-ahead)"       ui_type_ahead "combination"
ui_step "Enter"                          ui_key Return
ui_expect_new "Rule edited Made sizes: Put combination in table in then.0" "the block in the then"
ui_expect_new "Rule canvas editing then.0 table: " "its table to be typed"
ui_step "type Screws"                    xdotool type --delay 40 "Screws"
ui_step "Enter"                          ui_key Return
ui_expect_new "Rule canvas columns of Screws: Length=Length, Size=Size" \
  "the options' columns follow by their names"
ui_expect_new "Rules saved: Set table of then.0 to Screws" "the rule is complete: saved"
# The list logs a row when its state changes; after the save it is still
# the state the rename logged, so the last line of the row is what counts.
last_row=$(grep -o 'Rules row Made sizes: .*' "$UI_LOG" | tail -1)
[ "$last_row" = "Rules row Made sizes: enabled, ok" ] && echo "ok   no mark in the list" ||
  ui_fail "no mark in the list: ${last_row:-no line of the row}"
ui_expect_log "Rule as text Made sizes: always / then combination in table Screws (Length = Length, Size = Size)" \
  "the rule's text shows the action"
[ -n "$SHOT" ] && ui_capture "$SHOT"

echo "--- 4. A generated row applied: the rule moves Length"
ui_focus_dialog '^Configurations$'
ui_mark
ui_step "the Rows tab"                   ui_click_logged "Configurations tab Rows"
ui_expect_new "Configurations row M5 Name at" "the rows' places are logged"
ui_step "select M5"                      ui_click_logged "Configurations row M5 Name"
ui_step "Apply"                          ui_click_logged "Configurations apply"
ui_expect_new "Configurations applied M5" "M5 applied"
ui_expect_new "Configuration moves: Length 16 mm → 20 mm: Made sizes" \
  "Length moves to the length of M5 in the table, the rule named"
expect_volume 2000 "10 x 10 x 20"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"

ui_finish "UI configuration tables test"
