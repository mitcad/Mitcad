#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser app/framework/ParameterEdits.cpp
# Parameter limits (mitcad#665) in Edit Parameters, through the real UI
# on the demo block (60 x 40 x 20 mm: d1, d2, d3), with the volumes the
# model reports:
#   - d1's Max = 50: d1 = 60 mm is limited to 50 mm; its Value cell is
#     marked (60 mm → 50 mm ▲max) and its tooltip says why; the block is
#     50 x 40 x 20.
#   - Typing 70 into d1's expression says at once that it will be limited;
#     Enter keeps 70 mm as the expression, the value stays 50 mm.
#   - "Only limited" shows d1 alone, and all three again when cleared.
#   - Strict on d1 while its expression (70 mm) is above the maximum is
#     refused with the reason.
#   - d1's Min = d2 (a limit from another parameter): d1 gets a slider
#     between 40 and 50 mm; Home on it sets d1 to 40 mm (40 x 40 x 20).
#   - Undo takes the slider's change back.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-parameter-limits-test.sh [screenshot.png]

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}

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
ui_start_app --demo
ui_mark
ui_step "Edit Parameters (search)"     ui_command "Edit Parameters"
ui_expect_new "Parameters dialog opened" "the dialog opened"
ui_expect_new "Parameters d1 max at" "the limits' cells are logged"
ui_expect_new "Parameters only limited at" "the filter's place is logged"

echo "--- A maximum: the mark and its tooltip"
ui_mark
ui_step "d1 max = 50"                    edit_cell "d1 max" "50"
ui_expect_new "Parameter d1 max: 50 = 50 mm" "d1 has a maximum"
ui_expect_new "Parameter d1 limited: d1 is limited to its maximum 50 mm (its expression gives 60 mm)" \
  "the limit is said at once"
ui_expect_new "d1 = 60 mm (50 mm ▲max)" "the row is marked"
ui_expect_new "Parameters limited: d1 = 60 mm → 50 mm ▲max (d1 is limited to its maximum 50 mm (its expression gives 60 mm); Maximum = 50)" \
  "the Value cell's mark and tooltip"
expect_volume 40000 "50 x 40 x 20"

echo "--- Typing a value above the maximum"
ui_mark
ui_focus_dialog '^Parameters$'
ui_step "edit d1's expression"           ui_double_click_logged "Parameters d1 expression"
ui_sync
ui_key ctrl+a
xdotool type --delay 40 "70"
ui_expect_new "Parameter d1 typed 70 mm: d1: 70 mm will be limited to 50 mm (▲max)" \
  "said while it is typed"
ui_key Return
ui_expect_new "Parameter d1 expression: 70 = 50 mm" "the expression is kept, the value limited"
ui_expect_new "d1 = 70 mm (50 mm ▲max)" "still marked"
[ -n "$SHOT" ] && ui_capture "$SHOT"

echo "--- Only limited"
ui_mark
ui_focus_dialog '^Parameters$'
ui_step "Only limited (on)"              ui_click_logged "Parameters only limited"
ui_expect_new "Parameters only limited: on: d1" "only d1 is shown"
ui_mark
ui_step "Only limited (off)"             ui_click_logged "Parameters only limited"
ui_expect_new "Parameters only limited: off: d1, d2, d3" "all are shown again"

echo "--- Strict is refused while the expression is outside"
ui_mark
ui_focus_dialog '^Parameters$'
ui_step "d1 strict"                      ui_click_logged "Parameters d1 strict"
ui_expect_new "Parameter d1 strict: on refused: " "strict refused"
ui_expect_new "70 mm is above the maximum 50 mm" "with the reason"

echo "--- A minimum from another parameter, and the slider"
ui_mark
ui_step "d1 min = d2"                    edit_cell "d1 min" "d2"
ui_expect_new "Parameter d1 min: d2 = 50 mm" "d1's minimum is d2"
ui_expect_new "Parameters d1 slider handle at" "d1 has a slider"
ui_mark
ui_focus_dialog '^Parameters$'
ui_step "the slider's handle"            ui_click_logged "Parameters d1 slider handle"
sleep 0.5
tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -q "Parameter d1 slider: " &&
  ui_fail "clicking the handle changed d1"
echo "ok   clicking the handle changes nothing"
ui_step "Home (the minimum)"             ui_key Home
ui_expect_new "Parameter d1 slider: 40 mm = 40 mm" "the slider sets the minimum"
expect_volume 32000 "40 x 40 x 20"
ui_mark
ui_focus_dialog '^Parameters$'
ui_step "OK"                             ui_click_logged "Parameters OK"
ui_focus_main
ui_mark
ui_step "undo (Ctrl+Z)"                  ui_key ctrl+z
ui_expect_new "Undo: Change d1" "the slider's change is an undo step"
expect_volume 40000 "50 x 40 x 20 again"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"

ui_finish "UI parameter limits test"
