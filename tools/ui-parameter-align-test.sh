#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser tools/cli
# Numbers on a shared decimal axis in Edit Parameters (mitcad#566) through
# the real UI, on a synthetic design of user parameters made with
# mitcad-cli: 3 mm, 33 mm, 11.5 mm, 0.125 mm, -2.75 mm, 0.5 in and 30 deg,
# and a formula (p1 * 2):
#   - The window logs where each aligned number's decimal separator and unit
#     are (the layout the cells are painted with): in Value all eight
#     numbers have one separator position, integers with their implied one,
#     and one position for the units, whatever the units' lengths; so do the
#     seven numeric literals of Expression. The formula has none: it is
#     text, painted at the cell's left edge.
#   - A selected row keeps the axis (screenshots of ordinary and selected
#     rows when a folder is given).
#   - Editing in place keeps what the cells hold: an edit left with Esc
#     changes nothing; a number typed with a decimal comma is stored with a
#     point and the axis is still one after the change; a wider number
#     moves the axis for every row.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-parameter-align-test.sh [screenshot-folder]

source "$(dirname "$0")/ui-test-lib.sh"

SHOTS=${1:-}
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-parameter-align.XXXXXX)
FILE=$WORK/numbers.mitcad
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT

cat > "$WORK/model.json" << 'EOF'
[
  {"cmd": "add_parameter", "name": "p1", "expression": "3 mm", "unit": "mm"},
  {"cmd": "add_parameter", "name": "p2", "expression": "33 mm", "unit": "mm"},
  {"cmd": "add_parameter", "name": "p3", "expression": "11.5 mm", "unit": "mm"},
  {"cmd": "add_parameter", "name": "p4", "expression": "0.125 mm", "unit": "mm"},
  {"cmd": "add_parameter", "name": "p5", "expression": "-2.75 mm", "unit": "mm"},
  {"cmd": "add_parameter", "name": "p6", "expression": "0.5 in", "unit": "in"},
  {"cmd": "add_parameter", "name": "p7", "expression": "30 deg", "unit": "deg"},
  {"cmd": "add_parameter", "name": "p8", "expression": "p1 * 2", "unit": "mm"}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# places column part: "<parameters> <distinct x>" of the latest "Parameters
# <name> <column> <part> at x,y" line of each parameter in the log (the window
# logs all of them again whenever one changes): how many parameters have one,
# and at how many x positions. The log of the whole run: a window that is
# opened again has its own lines after the earlier ones.
places() {
  sed -n "s/.*Parameters \(p[0-9]*\) $1 $2 at \([0-9]*\),[0-9]*.*/\1 \2/p" "$UI_LOG" |
    awk '{ x[$1] = $2 } END { for (k in x) { n++; if (!(x[k] in seen)) { seen[x[k]] = 1; d++ } } print n + 0, d + 0 }'
}

# axis_x column part: the latest x position of the part of p1, for a
# comparison between two states.
axis_x() {
  sed -n "s/.*Parameters p1 $1 $2 at \([0-9]*\),[0-9]*.*/\1/p" "$UI_LOG" | tail -1
}

# expect_axis column part count description: `count` parameters have a
# position of that part in the column, all at one x.
expect_axis() {
  local got
  got=$(places "$1" "$2")
  [ "$got" = "$3 1" ] || ui_fail "$4: '$got' (parameters, distinct positions), expected '$3 1'"
  echo "ok   $4"
}

# expect_none column part name description: the parameter has no such
# position (since the mark).
expect_none() {
  tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qF "Parameters $3 $1 $2 at " && ui_fail "$4"
  echo "ok   $4"
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

shot() {
  [ -n "$SHOTS" ] && ui_capture "$SHOTS/parameter-align-$1.png"
  return 0
}

ui_start_display
ui_start_app --open "$FILE" --no-native-dialogs
ui_mark
ui_step "Edit Parameters (search)"     ui_command "Edit Parameters"
ui_expect_new "Parameters OK at" "the dialog opened and its cells are logged"
ui_expect_new "Parameters: p1 = 3 mm (3 mm); p2 = 33 mm (33 mm); p3 = 11.5 mm (11.5 mm); p4 = 0.125 mm (0.125 mm); p5 = -2.75 mm (-2.75 mm); p6 = 0.5 in (0.5 in); p7 = 30 deg (30 deg); p8 = p1 * 2 (6 mm)" \
  "the parameters as the model has them"

echo "--- One decimal axis in Value and in Expression"
expect_axis value decimal 8 "Value: eight numbers, one decimal separator position (integers too)"
expect_axis value unit 8 "Value: one position for the units of mm, in and deg"
expect_axis expression decimal 7 "Expression: seven numeric literals, one decimal separator position"
expect_axis expression unit 7 "Expression: one position for the units"
expect_none expression decimal p8 "the formula p1 * 2 is text: no decimal position"
expect_none expression unit p8 "the formula has no unit position either"
shot ordinary

echo "--- A selected row keeps the axis"
ui_focus_dialog '^Parameters$'
ui_step "select p4"                      ui_click_logged "Parameters p4 name"
shot selected
ui_step "select p5 (its comment)"        ui_click_logged "Parameters p5 comment"
shot selected-other
expect_axis value decimal 8 "Value: the axis is the same with a row selected"

echo "--- Editing in place changes nothing until a value is entered"
ui_focus_dialog '^Parameters$'
ui_mark
ui_step "edit p2's expression (double-click)" ui_double_click_logged "Parameters p2 expression"
ui_expect_new "Editing parameter p2 expression" "the in-place editor opened"
ui_step "leave it (Esc)"                 ui_key Escape
sleep 1
tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qF "Parameter p2 " && ui_fail "leaving the editor changed p2"
tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qF "Parameters: " && ui_fail "the parameters were listed again"
echo "ok   Esc left p2 as it was"
ui_focus_dialog '^Parameters$'

echo "--- A number typed with a decimal comma"
ui_mark
ui_step "p3 = 2,5"                       edit_cell "p3 expression" "2,5"
ui_expect_new "Parameter p3 expression: 2,5 = 2.5 mm" "read as 2.5 mm"
ui_expect_new "p3 = 2.5 mm (2.5 mm)" "stored and shown with a point"
sleep 1
expect_axis value decimal 8 "Value: still one decimal separator position"
expect_axis expression decimal 7 "Expression: still one decimal separator position"

echo "--- A wider number moves the axis of every row"
before=$(axis_x value decimal)
[ -n "$before" ] || ui_fail "no decimal position of p1 in Value"
ui_focus_dialog '^Parameters$'
ui_mark
ui_step "p5 = 12345.5"                   edit_cell "p5 expression" "12345.5"
ui_expect_new "Parameter p5 expression: 12345.5 = 12345.5 mm" "a number of five digits"
ui_expect_new "Parameters OK at" "the cells are placed again"
after=$(axis_x value decimal)
[ "$after" -gt "$before" ] || ui_fail "the axis stayed at $before (now $after) for a wider integer part"
echo "ok   the axis moved from $before to $after"
expect_axis value decimal 8 "Value: one decimal separator position after the change"
expect_axis expression decimal 7 "Expression: one decimal separator position after the change"
shot wide

echo "--- Opened again"
ui_step "close the parameters (Esc)"     ui_key Escape
ui_focus_main
ui_mark
ui_step "Edit Parameters (search)"     ui_command "Edit Parameters"
ui_expect_new "Parameters dialog opened" "the dialog opened again"
expect_axis value decimal 8 "Value: one decimal separator position"
ui_step "close the parameters (Esc)"     ui_key Escape
ui_focus_main

grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
ui_crashed && ui_fail "crashed"
ui_finish "UI parameter align test"
