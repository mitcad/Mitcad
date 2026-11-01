#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser/ParametersDialog.cpp app/browser/ParametersDialog.hpp app/browser/TablesPage.cpp
# check-all sources: app/browser/TablesPage.hpp app/framework/FormulaEditor.cpp app/framework/FormulaEditor.hpp
# check-all sources: app/framework/ParameterField.cpp app/framework/ParameterField.hpp
# check-all sources: app/framework/ParameterSuggest.cpp app/framework/ParameterSuggest.hpp
# check-all sources: core/expr/src core/model/src/tables.rs core/model/src/document/tables.rs
# check-all sources: core/model/src/tables_csv.rs core/model/src/document/tables_csv.rs
# check-all sources: core/model-api/src/tables.rs core/model-api/src/tables_csv.rs
# Tables in Edit Parameters (mitcad#667, docs/table-lookups.md), on the
# demo block (60 x 40 x 20 mm: d1, d2, d3), with Qt's own file dialogs
# (--no-native-dialogs):
#   - The Tables tab: New makes the table Screws (a Key column of texts, a
#     Value column in mm, one empty row); the header renames the columns
#     Size and dk and Enter there or in the comment leaves the dialog open;
#     the wheel over dk's unit box changes nothing, a choice in its list
#     does (mm to cm and back, said in the status bar); cells typed: M4 7,
#     Add Row, M5; `abc` in dk is refused and stays red with the model's
#     reason; 8.5 is taken.
#   - The Parameters tab: d3 = XLOOKUP('M5'; Screws.Size; Screws.dk), the
#     block 8.5 mm high; the table says d3 uses it; M5's dk = 10 makes the
#     block 10 mm high.
#   - Import CSV...: a file with M5's dk 9 and a row M6 10, previewed (the
#     columns, d3 from 10 mm to 9 mm), imported; the block 9 mm high.
#   - The Formula Editor of d3: `Screws.` completes the table's columns,
#     `Screws.d` and Tab put dk in; M6 makes the block 10 mm high.
#   - Undo: the formula, then the import, each one step.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-tables-test.sh [screenshot.png]

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}
WORK=$(mktemp -d /tmp/mitcad-ui-tables.XXXXXX)
trap 'ui_cleanup; rm -rf "$WORK"' EXIT

printf '%s\r\n' 'Size [Text],dk [mm]' 'M4,7' 'M5,9' 'M6,10' > "$WORK/Screws.csv"

# expect_volume expected description: the last body volume change logged
# since the mark.
expect_volume() {
  local volume
  for _ in $(seq 1 75); do
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

# edit_cell "row column" text: a click on a cell of the table's grid,
# the text typed over it, Enter.
edit_cell() {
  ui_focus_dialog '^Parameters$'
  ui_click_logged "Tables cell $1"
  ui_sync
  xdotool type --delay 40 -- "$2"
  ui_key Return
}

# edit_header column name: a column's name in the grid's header.
edit_header() {
  ui_focus_dialog '^Parameters$'
  ui_click_logged "Tables column $1 name"
  ui_sync
  ui_key ctrl+a
  xdotool type --delay 40 -- "$2"
  ui_key Return
}

ui_start_display
ui_start_app --demo --no-native-dialogs
ui_mark
ui_step "Edit Parameters (search)"     ui_command "Edit Parameters"
ui_expect_new "Parameters dialog opened" "the dialog opened"
ui_expect_new "Parameters tab Tables at" "the Tables tab's place is logged"

echo "--- A new table: its columns and cells"
ui_mark
ui_focus_dialog '^Parameters$'
ui_step "the Tables tab"                 ui_click_logged "Parameters tab Tables"
ui_expect_new "Parameters tab shown: Tables" "the Tables tab"
ui_expect_new "Tables: none" "no tables yet"
ui_expect_new "Tables new at" "New's place is logged"
ui_mark
ui_step "New"                            ui_click_logged "Tables new"
ui_expect_new "New Table dialog opened: Table1" "New Table asks for a name"
ui_focus_dialog '^New Table$'
ui_step "Screws"                         ui_type_field "New Table" "Screws"
ui_step "OK (Enter)"                     ui_key Return
ui_expect_new "Table Screws created" "the table made"
ui_expect_new "Tables: Screws (2 columns, 1 rows)" "listed"
ui_expect_new "Table Screws columns: Key [Text], Value [mm]" "a key column and a value column"
ui_expect_new 'Table Screws rows: [[null,null]]' "one empty row"
ui_expect_new "Table Screws used by: none" "no parameter uses it yet"
ui_expect_new "Tables column 2 name at" "the header's boxes are logged"
ui_mark
ui_step "column 1: Size"                 edit_header 1 Size
ui_expect_new "Table Screws column 1 renamed: Key -> Size" "the first column renamed"
ui_mark
ui_step "column 2: dk"                   edit_header 2 dk
ui_expect_new "Table Screws column 2 renamed: Value -> dk" "the second column renamed"
ui_expect_new "Table Screws columns: Size [Text], dk [mm]" "the header follows"
ui_focus_dialog '^Parameters$' # Enter in a column's name left the dialog open
ui_mark
ui_step "a comment, Enter"               ui_type_in "Tables comment" "Socket head cap screws"
ui_key Return
ui_expect_new "Table Screws comment: Socket head cap screws" "Enter applies the comment"
ui_focus_dialog '^Parameters$' # and leaves the dialog open

echo "--- The unit box: the wheel changes nothing, a choice in its list does"
ui_mark
ui_step "the wheel over dk's unit"       xdotool mousemove $(ui_logged_at "Tables column 2 unit") click 5 click 5
sleep 0.5
tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -q "Table Screws column dk unit" && ui_fail "the wheel changed the unit"
echo "ok   the unit stays mm"
ui_mark
ui_step "dk's unit: its list"            ui_click_logged "Tables column 2 unit"
ui_sync
ui_step "cm (Down, Enter)"               ui_key Down Return
ui_expect_new "Table Screws column dk unit: mm -> cm" "a choice in the list changes the unit"
ui_expect_new "Table Screws: dk: mm → cm, the numbers kept." "the status bar says what it did"
ui_expect_new "Table Screws columns: Size [Text], dk [cm]" "the header follows"
ui_mark
ui_focus_dialog '^Parameters$'
ui_step "dk's unit: its list"            ui_click_logged "Tables column 2 unit"
ui_sync
ui_step "mm (Up, Enter)"                 ui_key Up Return
ui_expect_new "Table Screws column dk unit: cm -> mm" "and back to mm"
ui_expect_new "Table Screws columns: Size [Text], dk [mm]" "the header follows"
ui_mark
ui_step "M4"                             edit_cell "1 1" "M4"
ui_expect_new "Table Screws row 1 Size: M4" "a text cell"
ui_mark
ui_step "7"                              edit_cell "1 2" "7"
ui_expect_new "Table Screws row 1 dk: 7" "a number cell"
ui_expect_new 'Table Screws rows: [["M4",7]]' "the row as the model keeps it"
ui_mark
ui_focus_dialog '^Parameters$'
ui_step "Add Row"                        ui_click_logged "Tables add row"
ui_expect_new 'Table Screws rows: [["M4",7],[null,null]]' "a new empty row after it"
ui_expect_new "Tables cell 2 2 at" "the new row's cells are logged"
ui_mark
ui_step "M5"                             edit_cell "2 1" "M5"
ui_expect_new "Table Screws row 2 Size: M5" "the new row's key"
ui_mark
ui_step "abc in dk (refused)"            edit_cell "2 2" "abc"
ui_expect_new "Table Screws row 2 dk: abc refused: " "a text in a column of numbers is refused"
ui_expect_new "Table Screws refused cells: row 2 dk: abc (" "it stays in its cell, red, with the reason"
[ -n "$SHOT" ] && ui_capture "$SHOT"
ui_mark
ui_step "8.5 in dk"                      edit_cell "2 2" "8.5"
ui_expect_new "Table Screws row 2 dk: 8.5" "a number taken"
ui_expect_new 'Table Screws rows: [["M4",7],["M5",8.5]]' "the table"
ui_expect_new "Table Screws refused cells: none" "nothing refused any more"

echo "--- A dimension through XLOOKUP"
ui_mark
ui_focus_dialog '^Parameters$'
ui_step "the Parameters tab"             ui_click_logged "Parameters tab Parameters"
ui_expect_new "Parameters tab shown: Parameters" "the Parameters tab"
ui_expect_log "Parameters d3 expression at" "d3's cells are logged"
ui_mark
ui_focus_dialog '^Parameters$'
ui_step "d3's expression"                ui_double_click_logged "Parameters d3 expression"
ui_sync
ui_key ctrl+a
xdotool type --delay 40 -- "XLOOKUP('M5'; Screws.Size; Screws.dk)"
ui_sync
ui_expect_new 'Parameter suggestions for d3 expression "Screws.": Size, dk' "the field offers the table's columns"
ui_step "Enter"                          ui_key Return
ui_expect_new "Parameter d3 expression: XLOOKUP('M5'; Screws.Size; Screws.dk) = 8.5 mm" "d3 looks its value up"
expect_volume 20400 "the block 60 x 40 x 8.5"
ui_mark
ui_focus_dialog '^Parameters$'
ui_step "the Tables tab"                 ui_click_logged "Parameters tab Tables"
ui_expect_new "Table Screws used by: d3" "the table says d3 uses it"
ui_mark
ui_step "M5's dk = 10"                   edit_cell "2 2" "10"
ui_expect_new "Table Screws row 2 dk: 10" "the cell changed"
ui_expect_new "Table Screws changed parameters: d3" "d3 follows the cell"
expect_volume 24000 "the block 60 x 40 x 10"

echo "--- Import CSV..."
ui_mark
ui_focus_dialog '^Parameters$'
ui_step "Import CSV..."                  ui_click_logged "Tables import"
ui_step "type the path, Enter"           ui_type_path "Import Table from CSV" "$WORK/Screws.csv"
ui_focus_dialog '^Import Table$'
ui_expect_new "Table import preview of Screws.csv as Screws: " "the preview, for the table of the file's name"
ui_expect_new "Table import column Size [Text]" "the file's first column"
ui_expect_new "Table import column dk [mm]" "and its second"
ui_expect_new "Table import changes d3: 10 mm -> 9 mm" "d3 would change"
sleep 0.5
tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -q "Body Body1 (F2.b0): volume" && ui_fail "the preview changed the block"
ui_mark
ui_step "Import (Enter)"                 ui_key Return
ui_expect_new "Imported table Screws from Screws.csv: " "imported"
ui_expect_new 'Table Screws rows: [["M4",7],["M5",9],["M6",10]]' "the file's rows"
expect_volume 21600 "the block 60 x 40 x 9"

echo "--- The Formula Editor completes Screws."
ui_mark
ui_focus_dialog '^Parameters$'
ui_step "the Parameters tab"             ui_click_logged "Parameters tab Parameters"
ui_expect_new "Parameters tab shown: Parameters" "the Parameters tab"
ui_step "select d3"                      ui_click_logged "Parameters d3 name"
ui_sync
ui_mark
ui_step "Edit Formula..."                ui_click_logged "Parameters formula"
ui_focus_dialog '^Formula Editor$'
ui_expect_new "Formula editor OK at" "the Formula Editor opened"
ui_mark
ui_key ctrl+a
xdotool type --delay 40 -- "XLOOKUP('M6'; Screws.Size; Screws."
ui_expect_new 'Formula completions for "Screws.": Size, dk' "the table's columns offered"
ui_mark
xdotool type --delay 40 -- "d"
ui_expect_new 'Formula completions for "Screws.d": dk' "filtered by what is typed"
ui_step "Tab"                            ui_key Tab
ui_expect_new "Formula completion inserted dk: XLOOKUP('M6'; Screws.Size; Screws.dk" "dk put in"
xdotool type --delay 40 -- ")"
ui_sync
ui_mark
ui_step "OK"                             ui_click_logged "Formula editor OK"
ui_expect_new "XLOOKUP('M6'; Screws.Size; Screws.dk) = 10 mm" "d3 looks M6 up"
expect_volume 24000 "the block 60 x 40 x 10"

echo "--- Undo"
ui_focus_dialog '^Parameters$'
ui_step "OK (the table)"                 ui_click_logged "Parameters OK"
ui_focus_main
ui_mark
ui_step "undo (Ctrl+Z)"                  ui_key ctrl+z
ui_expect_new "Undo: " "the formula undone"
expect_volume 21600 "9 mm high again"
ui_mark
ui_step "undo (Ctrl+Z)"                  ui_key ctrl+z
ui_expect_new "Undo: Import Table Screws" "the import was one undo step"
expect_volume 24000 "10 mm high again"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"

ui_finish "UI tables test"
