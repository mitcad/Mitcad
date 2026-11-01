#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser app/framework/DialogFolders.cpp app/framework/DialogFolders.hpp core/model/src/parameters_csv.rs core/model/src/document/parameters_csv.rs core/model-api/src/parameters_csv.rs
# The parameters as CSV (mitcad#666) in Edit Parameters, on the demo
# block (60 x 40 x 20 mm: d1, d2, d3), with Qt's own file dialogs
# (--no-native-dialogs):
#   - Export...: Mitcad's table written to a file (header, d1's row).
#   - Import... of a parameter table of the .f3d application: the preview
#     lists what is added, changed, renamed, refused and skipped (a forward
#     reference, a unitless count with floor, degrees as °, a quoted
#     comment with a comma, a unit's name renamed, a text parameter
#     added (mitcad#664), an unknown name taking the value, one refused) and changes
#     nothing; Cancel leaves the design as it is.
#   - Import again, accepted: the values, the favourite and the block's
#     new volume.
#   - Mitcad's table with limits (mitcad#665): d2 gets a maximum that
#     clamps it, shown in the preview; the block follows. An import is one
#     undo step.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-parameters-csv-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

WORK=$(mktemp -d /tmp/mitcad-ui-parameters-csv.XXXXXX)
trap 'ui_cleanup; rm -rf "$WORK"' EXIT

# A parameter table as the .f3d application exports it.
printf '%s\r\n' \
  'Name,Unit,Expression,Value,Comments,Favorite' \
  'd1,mm,Width + 5 mm,35.00,,false' \
  'Width,mm,30 mm,30.00,"Outer width, without the flange",true' \
  'Count,,floor(Width / 8 mm),3.00,,false' \
  'Angle,°,30 deg,30.00,,false' \
  'mil,mm,2 mm,2.00,,false' \
  "Label,Text,'6qt','6qt',,false" \
  'Broken,mm,Unknown * 2,12.00,,false' \
  'Refused,mm,Unknown * 3,,,false' > "$WORK/table.csv"
# Mitcad's own table: d2 gets a maximum below its value.
printf '%s\r\n' \
  'name,kind,unit,expression,value,comment,favorite,min,max,step,strict' \
  'd2,model,mm,40 mm,40,,false,,35 mm,,false' > "$WORK/limits.csv"

# expect_volume expected description: the last body volume change logged
# since the mark.
expect_volume() {
  local volume
  volume=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" |
    sed -n 's/.*Body Body1 (F2.b0): volume [0-9.]* -> \([0-9.]*\) mm3.*/\1/p' | tail -1)
  [ -n "$volume" ] || ui_fail "$2: no volume logged"
  awk -v v="$volume" -v e="$1" 'BEGIN { exit !((v - e) ^ 2 < (1e-6 * e) ^ 2 + 1e-4) }' ||
    ui_fail "$2: volume $volume mm3, expected $1"
  echo "ok   $2: $volume mm3"
}

ui_start_display
ui_start_app --demo --no-native-dialogs
ui_mark
ui_step "Edit Parameters (search)"     ui_command "Edit Parameters"
ui_expect_new "Parameters dialog opened" "the dialog opened"
ui_expect_new "Parameters: d1 = 60 mm (60 mm); d2 = 40 mm (40 mm); d3 = 20 mm (20 mm)" \
  "the model's parameters listed"
ui_expect_new "Parameters import at" "Import's place is logged"
ui_expect_new "Parameters export at" "Export's place is logged"

echo "--- Export"
ui_mark
ui_focus_dialog '^Parameters$'
ui_step "Export..."                      ui_click_logged "Parameters export"
ui_step "type the path, Enter"           ui_type_path "Export Parameters to CSV" "$WORK/exported.csv"
ui_expect_new "Exported parameters to $WORK/exported.csv: 3 parameter(s)" "exported"
head -1 "$WORK/exported.csv" | grep -q '^name,kind,unit,expression,value,comment,favorite,min,max,step,strict' ||
  ui_fail "the exported file has no header of Mitcad's table"
grep -q '^d1,model,mm,60 mm,60,' "$WORK/exported.csv" || ui_fail "the exported file has no row of d1"
echo "ok   the file has the header and d1's row"

echo "--- Import, previewed and cancelled"
ui_mark
ui_focus_dialog '^Parameters$'
ui_step "Import..."                      ui_click_logged "Parameters import"
ui_step "type the path, Enter"           ui_type_path "Import Parameters from CSV" "$WORK/table.csv"
ui_focus_dialog '^Import Parameters$'
ui_expect_new "Parameters import preview of table.csv: 6 to add, 1 to change, 1 renamed, 1 refused" \
  "the preview's counts"
ui_expect_new "Parameters import row d1: change (expression) Width + 5 mm = 35 mm" \
  "d1 changes, from a later row (a forward reference)"
ui_expect_new "Parameters import row Width: add 30 mm = 30 mm" "Width is added"
ui_expect_new "Parameters import row Count: add floor(Width / 8 mm) = 3" "a unitless count with floor"
ui_expect_new "Parameters import row Angle: add 30 deg = 30 deg" "degrees as °"
ui_expect_new "Parameters import row mil as mil_1: add 2 mm = 2 mm" "a unit's name is renamed"
ui_expect_new "Parameters import row Label: add '6qt' = 6qt" "a text parameter is added (mitcad#664)"
ui_expect_new "Parameters import row Broken: add 12 mm = 12 mm, from its value: " \
  "an unknown name takes the value"
ui_expect_new "Parameters import row Refused: refuse: " "without a value: refused"
tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -q "Body Body1 (F2.b0): volume" && ui_fail "the preview changed the block"
ui_mark
ui_step "Cancel (Escape)"                ui_key Escape
ui_expect_new "Import of parameters from table.csv cancelled" "cancelled"
sleep 0.5
tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -q "Body Body1 (F2.b0): volume" && ui_fail "Cancel changed the block"
echo "ok   nothing changed"

echo "--- Import, accepted"
ui_mark
ui_focus_dialog '^Parameters$'
ui_step "Import..."                      ui_click_logged "Parameters import"
ui_step "type the path, Enter"           ui_type_path "Import Parameters from CSV" "$WORK/table.csv"
ui_focus_dialog '^Import Parameters$'
ui_expect_new "Parameters import preview of table.csv: " "the preview again"
ui_step "Import (Enter)"                 ui_key Return
ui_expect_new "Imported parameters from table.csv: 6 to add, 1 to change, 1 renamed, 1 refused" \
  "imported"
expect_volume 28000 "35 x 40 x 20"
ui_expect_new "d1 = Width + 5 mm (35 mm)" "d1 from Width"
ui_expect_new "Count = floor(Width / 8 mm) (3)" "the count"
ui_expect_new "mil_1 = 2 mm (2 mm)" "the renamed parameter"
ui_expect_new "Broken = 12 mm (12 mm)" "the value of the unknown name's row"
ui_expect_new "Parameters favorites: Width" "Width is a favourite"

echo "--- Limits from Mitcad's table (mitcad#665)"
ui_mark
ui_focus_dialog '^Parameters$'
ui_step "Import..."                      ui_click_logged "Parameters import"
ui_step "type the path, Enter"           ui_type_path "Import Parameters from CSV" "$WORK/limits.csv"
ui_focus_dialog '^Import Parameters$'
ui_expect_new "Parameters import preview of limits.csv: 1 to change" "the preview of the limits"
ui_expect_new "Parameters import row d2: change (limits) 40 mm = 35 mm, limits: max 35 mm, limited by max" \
  "d2 gets a maximum, which clamps it"
ui_step "Import (Enter)"                 ui_key Return
ui_expect_new "Imported parameters from limits.csv: 1 to change" "imported"
expect_volume 24500 "35 x 35 x 20"

echo "--- One undo step"
ui_focus_dialog '^Parameters$'
ui_step "OK"                             ui_click_logged "Parameters OK"
ui_focus_main
ui_mark
ui_step "undo (Ctrl+Z)"                  ui_key ctrl+z
ui_expect_new "Undo: Import Parameters" "the import was one undo step"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"

ui_finish "UI parameters CSV test"
