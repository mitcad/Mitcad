#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser/ConfigurationsDialog.cpp app/browser/ConfigurationsData.cpp app/browser/ConfigurationBar.cpp
# check-all sources: app/browser/ConfigurationBindingsPage.cpp app/browser/MainWindowConfigurationPanel.cpp tools/cli
# check-all sources: core/model-base/src/configurations.rs core/model-base/src/configurations
# check-all sources: core/model/src/document/configurations.rs core/model-api/src/configurations.rs
# The configuration words of mitcad#968 (docs/ui-conventions.md,
# "Configurations") through the real UI, on a design mitcad-cli makes: a
# text option Color (Black or White, White), a number option Length
# (300 mm … 2000 mm, step 10 mm, 1500 mm), and Depth = Length / 4 as an
# option whose value is calculated by its formula; one row Standard; a
# 20 x 20 mm extrusion Length long.
#   1. Configurations…: the Options headers (Option, Unit, Possible Values,
#      Value, Comment, By Formula, Used By) and the Bindings tab's Option
#      filter; the Value column shows the current values, Depth's
#      evaluated value (375 mm) with its expression, not the formula.
#   2. A double-click on Length's Value edits it; one on Depth's does not
#      (a By Formula value is not edited there).
#   3. The Configuration panel's row chooser is labelled Row.
#   4. Length = 1200 in the bar: opened again, the Value column shows the
#      current values, Length 1200 mm and Depth's 300 mm.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-configuration-terms-test.sh [screenshot.png]

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-configuration-terms.XXXXXX)
FILE=$WORK/shelf.mitcad
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT

R='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'
cat > "$WORK/model.json" << EOF
[
  {"cmd": "add_parameter", "name": "Color", "unit": "Text", "expression": "'White'",
   "values": ["'Black'", "'White'"]},
  {"cmd": "add_parameter", "name": "Length", "unit": "mm", "expression": "1500 mm",
   "min": "300 mm", "max": "2000 mm", "step": "10 mm"},
  {"cmd": "add_parameter", "name": "Depth", "unit": "mm", "expression": "Length / 4"},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 20, "height": 20},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$R"}],
    "extent": {"type": "distance", "distance": "Length"}, "operation": "new_body"}},
  {"cmd": "set_configurations", "configurations": {
    "options": ["Color", "Length", "Depth"], "closed": false,
    "rows": [{"name": "Standard", "values": {"Color": "'White'", "Length": "1200 mm"}}]}}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

ui_start_display
ui_start_app --open "$FILE" --no-native-dialogs
ui_expect_log "Browser Root at" "the design is open"

echo "--- 1. The Options tab's words and the current values"
ui_mark
ui_step "Configurations (search)"        ui_command "Configurations"
ui_expect_new "Configurations window opened" "the window opened"
ui_expect_new "Configurations options header: Option | Unit | Possible Values | Value | Comment | By Formula | Used By" \
  "the Options headers name possible values, the current value and By Formula"
ui_expect_new "Configurations bindings filter label: Option:" "the Bindings tab filters by Option"
ui_expect_new "Configurations options: Color (Black, White) = White; Length (300 mm … 2000 mm, step 10 mm) = 1500 mm" \
  "the Value column shows the options' current values"
ui_expect_new "; Depth (free) = 375 mm, by formula Length" \
  "Depth shows its evaluated value, its expression in the tooltip"
grep -q "Depth (free) = Length" "$UI_LOG" && ui_fail "the Value column showed Depth's unevaluated formula"
ui_expect_new "Configurations option Depth value at" "the Value cells' places are logged"

echo "--- 2. Only a value the user sets is edited in the Value column"
ui_focus_dialog '^Configurations$'
ui_double_click_expect "Configurations option Length value" "Editing option Length value" \
  "Length's Value is edited in place"
ui_step "cancel the edit (Esc)"          ui_key Escape
ui_mark
ui_focus_dialog '^Configurations$'
ui_step "double-click Depth's Value"     ui_double_click_logged "Configurations option Depth value"
ui_sync
sleep 1
tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -q "Editing option Depth" &&
  ui_fail "a By Formula option's Value was opened for editing"
echo "ok   Depth's Value (By Formula) is not edited"
[ -n "$SHOT" ] && ui_capture "$SHOT"
ui_focus_dialog '^Configurations$'
ui_step "close the window (Esc)"         ui_key Escape

echo "--- 3. The row chooser in the Configuration panel"
ui_expect_log "Configuration bar row label: Row" "the panel's row chooser is labelled Row"
ui_expect_log "Configuration bar: Color = White (Black | White); Length = 1500 mm" "the bar shows the options"

echo "--- 4. A change in the bar: the Value column follows"
ui_focus_main
ui_mark
ui_step "the bar's Length field"         ui_click_logged "Configuration bar Length"
ui_sync
ui_key ctrl+a
xdotool type --delay 40 "1200"
ui_key Return
ui_expect_new "Configuration chosen: Length = 1200" "the field's value is chosen"
ui_expect_new "Depth = 300 mm (driven)" "the bar shows Depth's new value"
# The field keeps the keyboard after Enter and would take the command
# search's S as text: Tab moves on to the Length slider, which leaves the
# letter keys to the window's shortcuts.
ui_step "leave the field (Tab)"          ui_key Tab
ui_focus_main
ui_step "Configurations (search)"        ui_command "Configurations"
ui_expect_new "Configurations window opened" "the window opened again"
ui_expect_new "Length (300 mm … 2000 mm, step 10 mm) = 1200 mm; Depth (free) = 300 mm, by formula Length" \
  "the Value column shows the current values after the change, Depth's evaluated"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"

ui_finish "UI configuration terms test"
