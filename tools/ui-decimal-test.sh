#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Decimal commas (mitcad#2) through the real UI: a value typed with a comma
# is read as with a point, and kept and shown with a point.
#   - A sketch dimension typed as 42,5 is 42.5 mm.
#   - An extrude distance typed as 12,5 is 12.5 mm and shows as 12.5 mm
#     once the field is left; the body is 42.5 x 20 x 12.5 mm.
#   - Change Parameters lists both with points; an entry of 7,5 is 7.5 mm,
#     and max(1,5) is refused with the hint to write max(1; 5).
# The model's rule: core/model/src/expr/mod.rs and its tests.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-decimal-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

# edit_cell "name column" text: double-click a cell, replace its text, Enter.
edit_cell() {
  ui_focus_dialog '^Parameters$'
  ui_double_click_logged "Parameters $1"
  ui_sync
  ui_key ctrl+a
  xdotool type --delay 40 "$2"
  ui_key Return
}

ui_start_display
ui_start_app

echo "--- A sketch dimension"
ui_step "create sketch on XY"            ui_create_sketch xy
ui_step "rectangle tool (R)"             ui_key r
ui_step "first corner at the origin"     ui_sketch_click 0 0
ui_step "opposite corner"                ui_sketch_click 40 20
ui_expect_log "Added rectangle 40 x 20 mm at (0, 0)" "a 40 x 20 rectangle"
ui_step "end the tool (Esc)"             ui_key Escape
ui_step "sketch dimension (D)"           ui_key d
ui_step "pick the bottom line"           ui_sketch_click 20 0
ui_step "place the value below it"       ui_sketch_click 20 -10
ui_expect_log "Sketch value editor at" "the value editor opened"
ui_step "type 42,5"                      xdotool type --delay 40 "42,5"
ui_step "Enter"                          ui_key Return
ui_expect_log "length d1 = 42.5 mm" "the dimension is 42.5 mm"
ui_step "end the tool (Esc)"             ui_key Escape
ui_step "finish sketch (Ctrl+Enter)"     ui_key ctrl+Return
ui_expect_log "Sketch finished" "sketch finished"

echo "--- An extrude distance"
ui_mark
ui_step "extrude (E)"                    ui_key e
ui_expect_new "Panel Extrude input taper at" "its panel"
ui_step "distance 12,5"                  ui_type_in "Panel Extrude input distance" "12,5"
ui_expect_new "Extrude Distance: 12,5 = 12.5 mm" "read as 12.5 mm"
ui_step "to the taper field"             ui_click_logged "Panel Extrude input taper"
ui_expect_new "Extrude Distance: 12.5 mm = 12.5 mm" "the field shows 12.5 mm once left"
ui_step "OK (Enter)"                     ui_key Return
ui_expect_volume "New body Body1 (F2.b0): volume \([0-9.]*\) mm3" 10625 "42.5 x 20 x 12.5 mm"

echo "--- Change Parameters"
ui_mark
ui_step "Change Parameters (search)"     ui_command "Change Parameters"
ui_expect_new "Parameters: d1 = 42.5 mm (42.5 mm); d2 = 12.5 mm (12.5 mm)" \
  "the expressions are kept with points"
ui_expect_new "Parameters d2 expression at" "its cells are logged"
ui_mark
ui_step "d2 = 7,5"                       edit_cell "d2 expression" "7,5"
ui_expect_new "Parameter d2 expression: 7,5 = 7.5 mm" "read as 7.5 mm"
ui_expect_new "d2 = 7.5 mm (7.5 mm)" "kept as 7.5 mm"
ui_expect_volume "Body Body1 (F2.b0): volume [0-9.]* -> \([0-9.]*\) mm3" 6375 "42.5 x 20 x 7.5 mm"
ui_mark
ui_step "d2 = max(1,5)"                  edit_cell "d2 expression" "max(1,5)"
ui_expect_new "Parameter d2 expression: max(1,5) refused: " "max(1,5) is max(1.5): refused"
tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qF "or a space after the comma: 'max(1; 5)'" ||
  ui_fail "the refusal does not say to write max(1; 5)"
echo "ok   the refusal says to write max(1; 5)"
ui_step "close the parameters (Esc)"     ui_key Escape
ui_focus_main

grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
ui_finish "UI decimal comma test"
