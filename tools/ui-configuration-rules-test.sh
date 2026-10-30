#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/configurations app/browser/ConfigurationRulesPage.cpp app/browser/ConfigurationsDialog.cpp
# check-all sources: app/framework/FormulaEditor.cpp tools/cli core/model-base/src/configurations.rs
# check-all sources: core/model-base/src/configurations core/model/src/document/configurations.rs
# check-all sources: core/model-api/src/configurations.rs
# The rules of a configuration through the real UI (the rule editor of
# mitcad#808; rules as text, mitcad#965; Add's templates, mitcad#966), on a
# design mitcad-cli makes: a text option Color ('Black' or 'White', White)
# and a number option Length (300, 800, 1200 or 2500 mm; 1200 mm), no
# rules.
#   1. Docked: Configurations…, the Rules tab (its lead line, its empty
#      state), Add › Empty rule, the name "Black is short"; Alt+T, the rule
#      typed as two lines (Shift+Enter between them), Enter: saved, one
#      undo step. A wrong line (`maximum`) is marked with its place and
#      reason and nothing is saved; Esc restores the rule's text.
#   2. The browser's Configuration bar: Color = Black moves Length from
#      1200 mm to 800 mm (the rule named), and the bar offers 300 mm and
#      800 mm only.
#   3. Floating, the keyboard only (after a click on the Rules tab): Alt+A
#      opens Add's templates, E the empty rule, named "Not too long";
#      Alt+P, the palette's max, Enter puts it in the then with Length's
#      largest value (saved at once) and the focus on its value; Enter,
#      2000 mm; Left to its option, Enter edits it, Esc; forbid put after
#      it and Delete takes it out; Shift+F10 opens the context menu; Alt+K,
#      Check Rules, lists 2500 mm as a value that is never allowed.
#   4. With the mouse: Add › Limit a range when… makes "Limit Length when
#      Color is Black" (when Color == 'Black', then Length max 2500 mm),
#      complete and saved, the status line empty; a number dragged from
#      the palette onto its value, 300 mm: the bar's Length follows the
#      rule (300 mm only).
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-configuration-rules-test.sh [screenshot.png]

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-configuration-rules.XXXXXX)
FILE=$WORK/shelf.mitcad
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT

cat > "$WORK/model.json" << 'EOF'
[
  {"cmd": "add_parameter", "name": "Color", "unit": "Text", "expression": "'White'",
   "values": ["'Black'", "'White'"]},
  {"cmd": "add_parameter", "name": "Length", "unit": "mm", "expression": "1200 mm",
   "values": ["300 mm", "800 mm", "1200 mm", "2500 mm"]},
  {"cmd": "set_configurations", "configurations": {"options": ["Color", "Length"]}}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# drag "palette id" "canvas place": a palette entry dragged onto the canvas.
drag() {
  ui_drag_logged "Rule palette $1" "Rule canvas $2"
}

# expect_none "text" "description": nothing logged since the mark contains it.
expect_none() {
  [ "$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -cF -- "$1")" = 0 ] || ui_fail "$2: '$1' was logged"
  echo "ok   $2"
}

# rules_tab: the Configurations window (the command search) on its Rules tab.
rules_tab() {
  ui_mark
  ui_step "Configurations (search)"      ui_command "Configurations"
  ui_expect_new "Configurations window opened" "the window opened"
  ui_focus_dialog '^Configurations$'
  ui_expect_log "Configurations tab Rules at" "the Rules tab's place is logged"
  ui_mark
  ui_step "the Rules tab"                ui_click_logged "Configurations tab Rules"
  ui_expect_new "Configurations page Rules" "the Rules page"
  ui_expect_new "Rule palette compare at" "the palette's places are logged"
  ui_expect_new "Rules add at" "the page's buttons are logged"
}

ui_start_display
ui_start_app --open "$FILE" --no-native-dialogs
ui_expect_log "Configuration bar: Color = White (Black | White); Length = 1200 mm (300 mm | 800 mm | 1200 mm | 2500 mm)" \
  "the bar shows the two options"

echo "--- 1. Black is short, typed as text (Docked)"
rules_tab
ui_expect_log "Configurations lead Rules: Rules take away values and combinations that are not allowed. Most designs need none." \
  "the Rules tab's lead line"
ui_expect_log "Configurations empty Rules: No rules yet. A rule is, for example: when Color is Black, Length is at most 1000 mm." \
  "the empty state says what a rule is"
ui_mark
ui_step "Add"                            ui_click_logged "Rules add"
ui_expect_new "Rules add menu: Limit a range when… | Allow only some values when… | Forbid a combination… | Only the combinations of a table… (disabled: " \
  "Add offers its templates (no table: not that one)"
ui_expect_new "| Empty rule" "the empty rule last"
ui_sync
ui_step "Empty rule (E)"                 ui_key e
ui_expect_new "Rules saved: Add rule Rule 1" "a new rule, saved"
ui_expect_new "Rules naming Rule 1" "its name to be typed"
ui_step "type its name"                  xdotool type --delay 40 "Black is short"
ui_step "Enter"                          ui_key Return
ui_expect_new "Rules saved: Rename rule Rule 1 to Black is short" "renamed"
ui_mark
ui_step "Alt+T (the text)"               ui_key alt+t
ui_step "type the when"                  xdotool type --delay 40 "when Color == 'Black'"
ui_step "Shift+Enter (a new line)"       ui_key shift+Return
ui_step "type the then"                  xdotool type --delay 40 "then Length max 1000 mm"
ui_step "Enter"                          ui_key Return
ui_expect_new "Rules saved: Edit Black is short as text" "the two lines are saved"
ui_expect_new "Rule as text Black is short: when Color == 'Black' / then Length max 1000 mm" "the rule as text"
ui_expect_log "Rules row Black is short: enabled, ok" "no mark in the list"
[ -n "$SHOT" ] && ui_capture "$SHOT"
ui_mark
ui_step "select all"                     ui_key ctrl+a
ui_step "type a wrong when"              xdotool type --delay 40 "when Color == 'Black'"
ui_step "Shift+Enter"                    ui_key shift+Return
ui_step "type a wrong then"              xdotool type --delay 40 "then Length maximum 1000 mm"
ui_step "Enter"                          ui_key Return
ui_expect_new "Rule text not read: Line 2, column 13: maximum is no action" \
  "the wrong line is marked with its place and the reason"
ui_expect_new "Rules status: Not saved: Line 2, column 13" "the status line says it"
expect_none "Rules saved:" "nothing is saved"
ui_step "Esc"                            ui_key Escape
ui_expect_new "Rule text restored" "Esc restores the rule's text"
ui_expect_new "Rules status: none" "the status line is empty again"

echo "--- 2. The bar: Black limits Length"
ui_focus_main
ui_expect_log "Configuration bar Color at" "the Color chooser's place is logged"
ui_mark
ui_step "the bar's Color: Black"         ui_choose "Configuration bar Color" -1
ui_expect_new "Configuration chosen: Color = Black" "Black chosen"
ui_expect_new "Configuration moves: Length 1200 mm → 800 mm: Black is short" "Length moved by the rule"
ui_expect_new "Configuration bar: Color = Black (Black | White); Length = 800 mm (300 mm | 800 mm)" \
  "the bar's Length is limited by the rule"
ui_mark
ui_step "save (Ctrl+S)"                  ui_key ctrl+s
ui_expect_new "Saved $FILE" "saved"

echo "--- 3. Not too long, with the keyboard (Floating)"
ui_stop_app
ui_start_app --open "$FILE" --no-native-dialogs --chrome=floating
ui_expect_log "Configuration bar: Color = Black" "the design as saved"
rules_tab
ui_expect_log "Rules row Black is short: enabled, ok" "the saved rule is listed"
ui_mark
ui_step "Alt+A (Add)"                    ui_key alt+a
ui_expect_new "Rules add menu: " "Add opens its templates"
ui_sync
ui_step "Empty rule (E)"                 ui_key e
ui_expect_new "Rules saved: Add rule Rule 1" "a new rule"
ui_expect_new "Rules naming Rule 1" "its name to be typed"
ui_step "type its name"                  xdotool type --delay 40 "Not too long"
ui_step "Enter"                          ui_key Return
ui_expect_new "Rules saved: Rename rule Rule 1 to Not too long" "renamed"
ui_mark
ui_step "Alt+P (the palette)"            ui_key alt+p
ui_step "max (type-ahead)"               ui_type_ahead "max"
ui_step "Enter"                          ui_key Return
ui_expect_new "Rules saved: Put max in then.0" "max in the then, with its values: saved at once"
ui_expect_new "Rule as text Not too long: always / then Length max 2500 mm" "Length's largest value to start from"
ui_expect_new "Rule canvas focus then.0.0 block" "the focus on its value"
ui_mark
ui_step "Enter"                          ui_key Return
ui_expect_new "Rule canvas editing then.0.0 block: 2500 mm" "Enter edits the value"
ui_step "type 2000 mm"                   xdotool type --delay 40 "2000 mm"
ui_step "Enter"                          ui_key Return
ui_expect_new "Rules saved: Set then.0.0 to 2000 mm" "saved"
ui_mark
ui_step "Alt+B (the blocks)"             ui_key alt+b
ui_step "Left"                           ui_key Left
ui_expect_new "Rule canvas focus then.0 option" "Left: the option"
ui_step "Enter"                          ui_key Return
ui_expect_new "Rule canvas editing then.0 option: Length" "Enter edits it"
ui_step "Esc"                            ui_key Escape
ui_expect_new "Rule canvas editing then.0 option cancelled" "Esc only cancels the edit"
ui_mark
ui_step "Alt+P (the palette)"            ui_key alt+p
ui_step "forbid (type-ahead)"            ui_type_ahead "forbid"
ui_step "Enter"                          ui_key Return
ui_expect_new "Rule edited Not too long: Put forbid in then.1" "forbid after the max"
ui_expect_new "Rule canvas editing then.1 message: " "its message to be typed"
ui_step "type never"                     xdotool type --delay 40 "never"
ui_step "Enter"                          ui_key Return
ui_expect_new "Rules saved: Set message of then.1 to never" "saved"
ui_expect_new "Rule canvas focus then.1 block" "the focus on the forbid"
ui_mark
ui_step "Delete"                         ui_key Delete
ui_expect_new "Rules saved: Delete then.1" "Delete takes it out"
ui_expect_new "Rule as text Not too long: always / then Length max 2000 mm" "its text"
ui_step "Left"                           ui_key Left
ui_step "Shift+F10"                      ui_key shift+F10
ui_expect_new "Context menu: " "the context menu opens from the keyboard"
ui_step "Esc"                            ui_key Escape
ui_expect_log "Rules row Not too long: enabled, ok" "the rule is listed without a mark"
ui_mark
ui_step "Alt+K (Check Rules)"            ui_key alt+k
ui_expect_new "Check Rules never allowed: Length 2500 mm" "Check Rules lists a value that is never allowed"

echo "--- 4. Add › Limit a range when…, with the mouse: the bar follows the rule"
ui_focus_dialog '^Configurations$'
ui_mark
ui_step "Add"                            ui_click_logged "Rules add"
ui_expect_new "Rules add menu: Limit a range when…" "the templates"
ui_sync
ui_step "Limit a range when… (L)"        ui_key l
ui_expect_new "Rules saved: Add rule Limit Length when Color is Black" "a complete rule, saved at once"
ui_expect_new "Rules naming Limit Length when Color is Black" "its name, to be typed over"
ui_step "Esc (the name stays)"           ui_key Escape
ui_expect_new "Rule as text Limit Length when Color is Black: when Color == 'Black' / then Length max 2500 mm" \
  "every place filled from the design's options"
ui_expect_log "Rules row Limit Length when Color is Black: enabled, ok" "no mark in the list"
expect_none "Rules status: " "the status line stays empty"
ui_expect_log "Rule canvas then.0.0 block at" "its value's place is logged"
ui_mark
ui_step "number onto its value"          drag number "then.0.0 block"
ui_expect_new "Rule canvas target then.0.0 block" "the value is highlighted on the way"
ui_expect_new "Rule canvas editing then.0.0 block: " "the number to be typed"
ui_step "type 300 mm"                    xdotool type --delay 40 "300 mm"
ui_step "Enter"                          ui_key Return
ui_expect_new "Rules saved: Set then.0.0 to 300 mm" "the value replaced: saved"
ui_expect_new "Configuration bar: Color = Black (Black | White); Length = 300 mm (300 mm)" \
  "the bar's Length follows the rule"

ui_finish "UI configuration rules test"
