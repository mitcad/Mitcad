#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Metric defaults (P11, docs/architecture.md) through the real
# UI, on the demo block (60 x 40 x 20 mm): wherever a unit or a measurement
# system is chosen, the default is metric and inches are only a choice.
#   - The document is in millimetres (Document Settings, Change Active
#     Units offers mm first).
#   - Export writes STEP in millimetres unless another unit is chosen.
#   - Insert Mesh and Insert DXF read files without units as millimetres.
#   - Hole and Thread start at ISO metric sizes; their size table lists ISO
#     metric first, Unified sizes only when chosen.
#   - A new user parameter is in mm, the grid's fixed spacing in mm, and
#     Physical Properties weighs steel (7.85 g/cm3) in kg.
# The model's defaults: core/model/src/api/metric_defaults_tests.rs.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-units-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

WORK=$(mktemp -d /tmp/mitcad-ui-units.XXXXXX)
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
UNITS="mm (mm | cm | m | in | ft)"

file_menu() {
  ui_key alt+f
  ui_sync
  ui_key "$1"
}

# type_into_dialog title path: a path typed into a dialog's focused field.
type_into_dialog() {
  ui_focus_dialog "$1"
  ui_key ctrl+a
  xdotool type --delay 20 "$2"
  sleep 0.3
  ui_key Return
}

ui_start_display
ui_start_app --demo --no-native-dialogs
ui_step "fit (F6)"                         ui_key F6

echo "--- The document"
ui_step "select Document Settings"         ui_click_logged "Browser Root/Document Settings"
ui_step "expand it (Right)"                ui_key Right
ui_expect_log "Browser Root/Document Settings/Units: mm at" "the document is in millimetres"
ui_mark
ui_step "double-click the unit"            ui_double_click_logged "Browser Root/Document Settings/Units: mm"
ui_expect_new "Units dialog opened: mm (mm | cm | m | in | ft)" "Change Active Units offers mm first"
ui_focus_dialog '^Units$'
ui_step "keep it (Esc)"                    ui_key Escape
ui_focus_main

echo "--- Export and import"
ui_mark
ui_step "File > Export"                    file_menu e
ui_expect_new "Export dialog opened: unit $UNITS" "Export offers millimetres first"
type_into_dialog '^Export$' "$WORK/block.step"
ui_focus_main
ui_expect_new "Exported $WORK/block.step: step, 1 body(ies), mm" "the STEP file is written in millimetres"
grep -q "MILLI" "$WORK/block.step" || ui_fail "the STEP file names no millimetres"
echo "ok   the file's unit is the millimetre"
ui_step "File > Export an STL file"        file_menu e
type_into_dialog '^Export$' "$WORK/block.stl"
ui_focus_main
ui_mark
ui_step "File > Import it"                 file_menu i
type_into_dialog '^Import$' "$WORK/block.stl"
ui_expect_new "Insert Mesh dialog opened: unit $UNITS" "a mesh's unit is asked, millimetres first"
ui_step "keep millimetres (Enter)"         type_into_dialog '^Insert Mesh$' ""
ui_focus_main
ui_expect_volume "New body .*(F[0-9]*\.b0): volume \([0-9.]*\) mm3" 48000 "the mesh is as large as the block" 1e-4
ui_step "undo the import (Ctrl+Z)"         ui_key ctrl+z
ui_step "File > Export the sketch to DXF"  file_menu e
type_into_dialog '^Export$' "$WORK/sketch.dxf"
ui_focus_main
ui_mark
ui_step "File > Import it"                 file_menu i
type_into_dialog '^Import$' "$WORK/sketch.dxf"
ui_expect_new "Insert DXF dialog opened: unit $UNITS" "a drawing without a unit is read in millimetres"
ui_step "on XY (Enter)"                    type_into_dialog '^Insert DXF$' ""
ui_focus_main
ui_expect_new "Inserted sketch.dxf into Sketch2: 4 curves" "the drawing came in"
ui_step "undo the insert (Ctrl+Z)"         ui_key ctrl+z

echo "--- Hole and Thread sizes"
ui_mark
ui_step "hole (H)"                         ui_key h
ui_expect_new "standard=iso_metric, size=6, designation=M6x1, class=6H" "a tapped hole starts at M6x1 6H"
ui_expect_new "Panel Hole options size: 1 | 1.1 | 1.2 | 1.4 | 1.6" "the sizes are ISO metric"
ui_expect_new "Panel Hole options designation: M6x1 | M6x0.75" "M6 coarse first"
ui_expect_new "Panel Hole options class: 4H | 5G | 5H | 6G | 6H | 7G | 7H" "the nut's classes"
ui_expect_log "Panel Hole input tap at" "the panel is laid out"
ui_step "Tapped"                           ui_choose "Panel Hole input tap" 1
ui_expect_new "Hole: Hole Tap Type = Tapped" "tapped"
ui_sync
ui_mark
ui_step "ANSI Unified (only by choice)"    ui_choose "Panel Hole input standard" 1
ui_expect_new "Hole: Thread Type = ANSI Unified Screw Threads" "Unified chosen"
ui_expect_new "Panel Hole options designation: 1/4-20 UNC | 1/4-28 UNF | 1/4-32 UNEF" "it starts at 1/4 UNC"
ui_step "cancel (Esc)"                     ui_key Escape
ui_mark
ui_step "thread (search)"                  ui_command "Thread"
ui_expect_new "Panel Thread choices: standard=iso_metric, size=10, designation=M10x1.5, class=6g" \
  "a thread starts at M10x1.5 6g"
ui_step "cancel (Esc)"                     ui_key Escape

echo "--- Parameters, grid, physical properties"
ui_mark
ui_step "Change Parameters (search)"       ui_command "Change Parameters"
ui_expect_new "Parameters add at" "the dialog opened"
ui_focus_dialog '^Parameters$'
ui_step "User Parameter (+)"               ui_click_logged "Parameters add"
ui_expect_new "Add User Parameter dialog opened: unit mm" "a new parameter is in millimetres"
ui_focus_dialog '^Add User Parameter$'
ui_step "close it (Esc)"                   ui_key Escape
ui_focus_dialog '^Parameters$'
ui_step "close the parameters (Esc)"       ui_key Escape
ui_focus_main
ui_mark
ui_step "Grid Settings (search)"           ui_command "Grid Settings"
ui_expect_new "Grid settings dialog opened: automatic, fixed spacing 10 mm" "the grid is in millimetres"
ui_focus_dialog '^Grid Settings$'
ui_step "close it (Esc)"                   ui_key Escape
ui_focus_main
ui_mark
ui_step "physical properties (search)"     ui_command "Physical Properties"
ui_expect_new "Physical properties: Body1 volume 48000 mass 0.3768 center (30, 20, 10)" \
  "48 cm3 of steel (7.85 g/cm3) weighs 0.3768 kg"
ui_focus_dialog "Properties"
ui_step "close the dialog (Esc)"           ui_key Escape
ui_focus_main

grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
ui_finish "UI units test"
