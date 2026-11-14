#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/files app/commands
# Metric defaults (P11, docs/architecture.md) through the real
# UI, on the demo block (60 x 40 x 20 mm): wherever a unit or a measurement
# system is chosen, the default is metric and inches are only a choice.
#   - The document is in millimetres (Design Settings, Change Active
#     Units offers mm first).
#   - Export writes STEP in millimetres unless another unit is chosen.
#   - Insert Mesh and Insert DXF read files without units as millimetres.
#   - Hole and Thread start at ISO metric sizes; their size table lists ISO
#     metric first, Unified sizes only when chosen.
#   - A new user parameter is in mm, the grid's fixed spacing in mm, and
#     Physical Properties weighs steel (7.85 g/cm3) in kg.
#   - Default units for new designs (mitcad#1036): mm by default; set to in
#     in Preferences > General, the empty untitled design starts again in
#     inches (mitcad#1452), New makes a design in inches (Units: in in the
#     browser, not modified) whose rectangle typed 2 x 1 is 2 x 1 in and
#     extrude distance 1 is an inch; that design, with its body, stays in
#     inches when the preference changes; the untitled design at a new
#     start is in inches, an opened design in mm keeps mm; set back to mm,
#     New is in mm again.
#     tools/ui-windows-default-units-test.ps1 does the same on Windows.
# The model's defaults: core/model-tests/api/src/metric_defaults_tests.rs.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-units-test.sh
# check-all sources: core/dxf
# check-all sources: app/framework/AppSettings.cpp app/framework/DocumentSession.cpp
# check-all sources: app/view/ViewController.cpp tools/cli
# check-all sources: app/MainWindowStart.cpp app/WindowManager.cpp

source "$(dirname "$0")/ui-test-lib.sh"

CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-units.XXXXXX)
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
UNITS="mm (mm | cm | m | in | ft)"

file_menu() {
  ui_key alt+f
  ui_sync
  ui_key "$1"
}

# type_into_dialog title path: a path typed into a dialog's focused field
# (ui_type_path), then Enter; no path: only Enter.
type_into_dialog() {
  if [ -z "$2" ]; then
    ui_focus_dialog "$(ui_title_regex "$1")"
    ui_key Return
  else
    ui_type_path "$1" "$2"
  fi
}

ui_start_display
ui_start_app --demo --no-native-dialogs
ui_step "fit (F6)"                         ui_key F6

echo "--- The document"
ui_step "select Design Settings"         ui_click_logged "Browser Root/Design Settings"
ui_step "expand it (Right)"                ui_key Right
ui_expect_log "Browser Root/Design Settings/Units: mm at" "the document is in millimetres"
ui_mark
ui_step "double-click the unit"            ui_double_click_logged "Browser Root/Design Settings/Units: mm"
ui_expect_new "Units dialog opened: mm (mm | cm | m | in | ft)" "Change Active Units offers mm first"
ui_focus_dialog '^Units$'
ui_step "keep it (Esc)"                    ui_key Escape
ui_focus_main

echo "--- Export and import"
ui_mark
ui_step "File > Export"                    file_menu e
ui_expect_new "Export dialog opened: unit $UNITS" "Export offers millimetres first"
type_into_dialog 'Export' "$WORK/block.step"
ui_focus_main
ui_expect_new "Exported $WORK/block.step: step, 1 body, mm" "the STEP file is written in millimetres"
grep -q "MILLI" "$WORK/block.step" || ui_fail "the STEP file names no millimetres"
echo "ok   the file's unit is the millimetre"
ui_step "File > Export an STL file"        file_menu e
type_into_dialog 'Export' "$WORK/block.stl"
ui_focus_main
ui_mark
ui_step "File > Import it"                 file_menu i
type_into_dialog 'Import' "$WORK/block.stl"
ui_expect_new "Insert Mesh dialog opened: unit $UNITS" "a mesh's unit is asked, millimetres first"
ui_step "keep millimetres (Enter)"         type_into_dialog 'Insert Mesh' ""
ui_focus_main
ui_expect_volume "New body .*(F[0-9]*\.b0): volume \([0-9.]*\) mm3" 48000 "the mesh is as large as the block" 1e-4
ui_step "undo the import (Ctrl+Z)"         ui_key ctrl+z
ui_step "File > Export the sketch to DXF"  file_menu e
type_into_dialog 'Export' "$WORK/sketch.dxf"
ui_focus_main
ui_mark
ui_step "File > Import it"                 file_menu i
type_into_dialog 'Import' "$WORK/sketch.dxf"
ui_expect_new "Insert DXF dialog opened: unit $UNITS" "a drawing without a unit is read in millimetres"
ui_step "on XY (Enter)"                    type_into_dialog 'Insert DXF' ""
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
ui_step "Edit Parameters (search)"       ui_command "Edit Parameters"
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

echo "--- Default units for new designs (mitcad#1036)"
# The preference is this script's own: ui-test-lib.sh gives every script a
# settings folder of its own (XDG_CONFIG_HOME), and the steps below set it
# back to mm.
SETTINGS=$XDG_CONFIG_HOME/Mitcad/Mitcad.conf
# A design in millimetres to open while new designs are in inches.
printf '[{"cmd": "sketch.create"}]\n' > "$WORK/metric.json"
"$CLI" run "$WORK/metric.json" --save "$WORK/metric.mitcad" > "$WORK/cli-metric.log" 2>&1 ||
  { cat "$WORK/cli-metric.log"; ui_fail "mitcad-cli (a design in millimetres)"; }

# expect_units unit description: the browser's Design Settings opened, its
# Units row logged since the mark with the unit.
expect_units() {
  ui_step "select Design Settings"         ui_click_logged "Browser Root/Design Settings"
  ui_step "expand it (Right)"              ui_key Right
  ui_expect_new "Browser Root/Design Settings/Units: $1 at" "$2"
}

# new_design_units unit keys...: Preferences > General > Default units for
# new designs (Alt+U) moved with the keys, OK; the choice saved.
new_design_units() {
  local unit=$1
  shift
  ui_mark
  ui_step "Preferences: General (search)"  ui_command "Preferences: General"
  ui_focus_dialog '^Preferences$'
  ui_step "Default units for new designs (Alt+U)" ui_key alt+u
  ui_step "choose $unit ($*)"              ui_key "$@"
  ui_step "OK (Enter)"                     ui_key Return
  ui_focus_main
  ui_expect_new "Preferences: default units for new designs $unit" "new designs are to be in $unit"
  grep -q "^newDesigns=$unit\$" "$SETTINGS" || ui_fail "units/newDesigns is not $unit in the settings"
  echo "ok   the setting is $unit"
}

ui_stop_app
ui_start_app
ui_mark
expect_units mm "the untitled design starts in millimetres by default"
new_design_units in Down Down Down
ui_expect_new "The untitled design starts again in in" "the empty untitled design takes inches (mitcad#1452)"
expect_units in "the untitled design is in inches now"
ui_mark
ui_step "New (Ctrl+N)"                     ui_key ctrl+n
ui_expect_new "New design in in" "New makes a design in inches"
ui_expect_new "Opened the new design with Component1 active" "the new design is open"
[ "$(ui_window_title)" = "Untitled - MitCad" ] ||
  ui_fail "the new design is modified: '$(ui_window_title)'"
echo "ok   the new design is not modified"
expect_units in "the browser shows Units: in"
ui_step "create sketch on XY"              ui_create_sketch xy
ui_step "rectangle tool (R)"               ui_key r
ui_step "first corner at the origin"       ui_sketch_click 0 0
ui_step "width 2 (Tab) height 1"           eval 'xdotool type --delay 40 2; ui_key Tab; xdotool type --delay 40 1'
ui_step "confirm (Enter)"                  ui_key Return
ui_expect_log "Added rectangle 50.8 x 25.4 mm at (0, 0)" "2 x 1 typed are 2 x 1 in"
ui_step "end the tool (Esc)"               ui_key Escape
ui_step "finish sketch (Ctrl+Enter)"       ui_key ctrl+Return
ui_expect_log "Sketch finished" "sketch finished"
ui_mark
ui_step "extrude (E)"                      ui_key e
ui_expect_new "Panel Extrude input taper at" "its panel"
ui_step "distance 1"                       ui_type_in "Panel Extrude input distance" "1"
ui_expect_new "Extrude Distance: 1 = 1 in" "the extrude's distance is in inches"
ui_step "to the taper field"               ui_click_logged "Panel Extrude input taper"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_volume "New body Body1 (F2.b0): volume \([0-9.]*\) mm3" 32774.128 "2 x 1 x 1 in (50.8 x 25.4 x 25.4 mm)"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
# A design with something in it keeps its units when the preference
# changes; the preference goes back to in for the next start.
new_design_units mm Up Up Up
ui_sync
tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qE "starts again in|New design in" &&
  ui_fail "the preference changed the open design with a body"
echo "ok   the open design with a body stays in inches"
new_design_units in Down Down Down

ui_stop_app
ui_start_app
ui_mark
expect_units in "the untitled design at the start is in inches"
ui_stop_app
ui_start_app --open "$WORK/metric.mitcad"
ui_mark
expect_units mm "an opened design keeps its millimetres"
new_design_units mm Up Up Up
ui_mark
ui_step "New (Ctrl+N)"                     ui_key ctrl+n
ui_expect_new "New design in mm" "New makes a design in millimetres again"
expect_units mm "the browser shows Units: mm"

ui_finish "UI units test"
