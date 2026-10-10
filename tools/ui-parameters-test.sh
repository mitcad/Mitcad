#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser
# Change Parameters (U3) through the real UI on the demo block (60 x 40 x 20
# mm: d1, d2, d3), with the volumes the model reports:
#   - Add a user parameter (width = 30 mm).
#   - Expressions in place: d1 = width + 5 mm, d2 = d1 * 2.
#   - What the model refuses stays in the cell with the reason: a cycle
#     (width = d2 / 2), an angle for a length (d3 = 30 deg).
#   - Rename width: the expressions that use it follow; deleting it is
#     refused while d1 uses it; a comment.
#   - A favourite (P9): the star lists width under Favorites, also after
#     the rename.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-parameters-test.sh [screenshot.png]

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}

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
ui_step "Change Parameters (search)"     ui_command "Change Parameters"
ui_expect_new "Parameters dialog opened" "the dialog opened"
ui_expect_new "Parameters: d1 = 60 mm (60 mm); d2 = 40 mm (40 mm); d3 = 20 mm (20 mm)" \
  "the model's parameters listed"
ui_expect_new "Parameters d1 expression at" "its cells are logged"

echo "--- A user parameter"
ui_focus_dialog '^Parameters$'
ui_mark
ui_step "User Parameter (+)"             ui_click_logged "Parameters add"
ui_focus_dialog '^Add User Parameter$'
ui_step "name, unit, expression, Enter"  xdotool type --delay 40 "width"
ui_key Tab
xdotool type --delay 40 "mm"
ui_key Tab
xdotool type --delay 40 "30"
ui_key Return
ui_expect_new "Added parameter width = 30 mm" "width added"
ui_expect_new "width = 30 mm (30 mm)" "and listed"

echo "--- A favourite (P9)"
ui_expect_new "Parameters width favorite at" "the star's place is logged"
ui_mark
ui_focus_dialog '^Parameters$'
ui_step "star width"                     ui_click_logged "Parameters width favorite"
ui_expect_new "Parameter width favorite: on" "width is a favourite"
ui_expect_new "Parameters favorites: width" "listed first, under Favorites"

echo "--- Expressions"
ui_mark
ui_step "d1 = width + 5 mm"              edit_cell "d1 expression" "width + 5 mm"
ui_expect_new "Parameter d1 expression: width + 5 mm = 35 mm" "d1 from width"
expect_volume 28000 "35 x 40 x 20"
ui_mark
ui_step "d2 = d1 * 2"                    edit_cell "d2 expression" "d1 * 2"
ui_expect_new "Parameter d2 expression: d1 * 2 = 70 mm" "d2 from d1"
expect_volume 49000 "35 x 70 x 20"

echo "--- Refused: a cycle, a unit"
ui_mark
ui_step "width = d2 / 2 (a cycle)"       edit_cell "width expression" "d2 / 2"
ui_expect_new "Parameter width expression: d2 / 2 refused: " "the cycle refused"
grep -q "Body Body1 (F2.b0): volume 49000.000 ->" "$UI_LOG" && ui_fail "the body changed"
ui_mark
ui_step "d3 = 30 deg (an angle)"         edit_cell "d3 expression" "30 deg"
ui_expect_new "Parameter d3 expression: 30 deg refused: " "the angle refused for a length"
[ -n "$SHOT" ] && ui_capture "$SHOT"

echo "--- Rename, delete, comment"
ui_mark
ui_step "rename width to w"              edit_cell "width name" "w"
ui_expect_new "Parameter width name: w = 30 mm" "renamed"
ui_expect_new "d1 = w + 5 mm (35 mm)" "d1's expression follows the new name"
ui_expect_new "Parameters favorites: w" "the favourite follows the new name"
ui_mark
ui_step "select w"                       ui_click_logged "Parameters w name"
ui_step "Delete"                         ui_click_logged "Parameters delete"
ui_expect_new "Delete parameter w refused: " "w is used by d1"
ui_mark
ui_step "a comment on d3"                edit_cell "d3 comment" "Height of the block"
ui_expect_new "Parameter d3 comment: Height of the block" "comment changed"
ui_step "OK"                             ui_click_logged "Parameters OK"
ui_focus_main
ui_mark
ui_step "undo (Ctrl+Z)"                  ui_key ctrl+z
ui_expect_new "Undo: Change Comment of d3" "the comment was an undo step"
ui_mark
ui_step "undo (Ctrl+Z)"                  ui_key ctrl+z
ui_expect_new "Undo: Rename width to w" "the rename too"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"

ui_finish "UI parameters test"
