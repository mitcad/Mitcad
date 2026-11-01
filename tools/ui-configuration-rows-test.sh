#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser/ConfigurationsDialog.cpp app/browser/ConfigurationsData.cpp app/browser/ConfigurationBar.cpp
# check-all sources: core/model-base/src/configurations.rs core/model-base/src/configurations
# check-all sources: core/model/src/document/configuration_rows_csv.rs core/model/src/document/configuration_tables.rs
# check-all sources: core/model/src/tables_csv.rs core/model-api/src/configuration_rows_csv.rs tools/cli
# Filling a closed table (mitcad#1044, docs/configurations.md decision 12)
# through the real UI, on a design mitcad-cli makes: a 10 x 10 mm
# extrusion k high (k = 2 mm), no configurations yet; Qt's own file dialogs
# (--no-native-dialogs).
#   1. Configurations…, Rows: Import CSV… with a small table of screw
#      sizes (size, d [mm], k [mm]: M3 3 3, M4 4 4, M5 5 5), previewed
#      (size the key option, d and k parameter columns, size and d made),
#      imported in one step: three rows, the table closed; the options
#      are the row M3, so k takes its 3 mm (300 mm3).
#   2. M4's d and k selected and copied (Ctrl+C), pasted at M5's d
#      (Ctrl+V): M5 takes 4 mm and 4 mm.
#   3. Options: Add Option… on the closed table with rows offers the
#      parameter column d; OK moves it to the options with its rows'
#      values.
#   4. The Configuration bar's size: M5 sets k to M5's pasted 4 mm; the
#      body follows (400 mm3).
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-configuration-rows-test.sh [screenshot.png]

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-configuration-rows.XXXXXX)
FILE=$WORK/screw.mitcad
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT

R='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'
cat > "$WORK/model.json" << EOF
[
  {"cmd": "add_parameter", "name": "k", "unit": "mm", "expression": "2 mm"},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 10, "height": 10},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$R"}],
    "extent": {"type": "distance", "distance": "k"}, "operation": "new_body"}}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }
printf '%s\r\n' 'size,d [mm],k [mm]' 'M3,3,3' 'M4,4,4' 'M5,5,5' > "$WORK/screws.csv"

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

# shift_click "logged place": extends the selection to it.
shift_click() {
  xdotool keydown shift
  ui_click_logged "$1"
  xdotool keyup shift
}

ui_start_display
ui_start_app --open "$FILE" --no-native-dialogs
ui_expect_log "Browser Root at" "the design is open"

echo "--- 1. Import CSV… in the Rows tab"
ui_mark
ui_step "Configurations (search)"        ui_command "Configurations"
ui_expect_new "Configurations window opened" "the window opened"
ui_focus_dialog '^Configurations$'
ui_step "the Rows tab"                   ui_click_logged "Configurations tab Rows"
ui_expect_new "Configurations import csv at" "Import CSV…'s place is logged"
ui_mark
ui_step "Import CSV…"                    ui_click_logged "Configurations import csv"
ui_step "type the path, Enter"           ui_type_path "Import Rows from CSV" "$WORK/screws.csv"
ui_focus_dialog '^Import Rows$'
ui_expect_new "Rows import preview of screws.csv: 3 rows from the file: added M3, M4, M5; made the parameters size, d" \
  "the preview: three rows, size and d made"
ui_expect_new "Rows import column size [Text]: key option, a new parameter" "the first column is the key option"
ui_expect_new "Rows import column k [mm]: parameter column" "k is a parameter column"
sleep 0.5
tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -q "Configurations rows: closed" && ui_fail "the preview changed the table"
ui_mark
ui_step "Import (Enter)"                 ui_key Return
ui_expect_new "Configurations imported rows from screws.csv: 3 rows from the file: added M3, M4, M5" "imported"
ui_expect_new "Configurations rows: closed, default none, current M3: M3: size = M3, d = 3 mm, k = 3 mm; M4: size = M4, d = 4 mm, k = 4 mm; M5: size = M5, d = 5 mm, k = 5 mm" \
  "the Rows sheet: a row per size, a column per parameter, the table closed"
ui_expect_new "Configurations row M5 d at" "the cells' places are logged"
expect_volume 300 "10 x 10 x 3: k follows the row M3 the options are on"

echo "--- 2. Copy and paste a block of cells"
ui_focus_dialog '^Configurations$'
ui_mark
ui_step "M4's d"                         ui_click_logged "Configurations row M4 d"
ui_step "to M4's k (Shift+click)"        shift_click "Configurations row M4 k"
ui_step "Ctrl+C"                         ui_key ctrl+c
ui_expect_new "Configurations copied 1 x 2 cells" "two cells copied"
ui_step "M5's d"                         ui_click_logged "Configurations row M5 d"
ui_step "Ctrl+V"                         ui_key ctrl+v
ui_expect_new "Configurations pasted 1 x 2 cells at M5 d: added none" "pasted at M5's d"
ui_expect_new "M5: size = M5, d = 4 mm, k = 4 mm" "M5 takes M4's values"

echo "--- 3. Add Option… on the closed table with rows"
ui_focus_dialog '^Configurations$'
ui_mark
ui_step "the Options tab"                ui_click_logged "Configurations tab Options"
ui_expect_new "Configurations add option at" "the Options page"
ui_mark
ui_step "Add Option…"                    ui_click_logged "Configurations add option"
ui_expect_new "Add Option dialog opened: " "Add Option offers the parameters"
ui_focus_dialog '^Add Option$'
ui_step "d (type-ahead), OK (Enter)"     ui_key d Return
ui_expect_new "Configurations added option d" "d is an option"
ui_expect_new "Configurations option d moved from the parameter columns" "with its rows' values"
[ -n "$SHOT" ] && ui_capture "$SHOT"

echo "--- 4. A size on the Configuration bar"
ui_focus_dialog '^Configurations$'
ui_step "close (Esc)"                    ui_key Escape
ui_focus_main
ui_expect_log "Configuration bar size at" "the size chooser's place is logged"
ui_mark
ui_step "the bar's size: M5"             ui_choose "Configuration bar size" 2
ui_expect_new "Configuration chosen: size = M5" "M5 chosen"
expect_volume 400 "10 x 10 x 4: k follows M5's row"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"

ui_finish "UI configuration rows test"
