#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/drawings app/view/AnnotationDialogs.cpp core/model/src/drawing core/model/src/document.rs
# check-all sources: core/model-api/src/drawings.rs core/model-api/src/drawing_annotations.rs
# check-all sources: core/model-api/src/drawing_templates.rs tools/cli
# Parameters in a drawing's texts (mitcad#2483) through the real UI, in the
# Docked layout, on a 40 x 30 x 20 block that mitcad-cli makes with the user
# parameters Length (120 mm) and Maker ('ACME', a text):
#   - File › Create Drawing with the ISO template, a front view at 2:1.
#   - Text: its Text field's menu opened with the Menu key, Insert Parameter
#     (I) lists the parameters, the user ones first; Length chosen with Enter goes in at the
#     cursor; " BY " typed; the menu by a right-click, Maker chosen through
#     the dialog's filter: L={param:Length} BY {param:Maker}, placed: the
#     sheet shows L=120 mm BY ACME.
#   - Note: a reference typed in its Text field ({param:Maker}) works the
#     same: MADE BY ACME.
#   - Edit Parameters: Length 150 mm: the text shows L=150 mm BY ACME.
#   - The text's Edit Text… (the Note dialog): Insert Parameter in its menu
#     (the Menu key), and a reference to a parameter the design lacks named
#     under the text; cancelled.
#   - Edit Title Block: a field MAKER whose Text gets {param:Maker} from a
#     right-click's Insert Parameter, then {param:Finish} typed: the missing
#     parameter named under the field; Done: the title block shows
#     MAKER=ACME {param:Finish}.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-drawing-parameters-test.sh [screenshot.png]

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-drawing-parameters.XXXXXX)
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
R='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'
FILE="$WORK/block.mitcad"

cat > "$WORK/block.json" << EOF
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 40, "height": 30},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$R"}],
    "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
  {"cmd": "add_parameter", "name": "Length", "expression": "120 mm"},
  {"cmd": "add_parameter", "name": "Maker", "expression": "'ACME'"}
]
EOF
"$CLI" run "$WORK/block.json" --save "$FILE" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# The sheet's placement, logged as "Drawing sheet view x0,y0 x1,y1 x2,y2":
# where the sheet points (0, 0), (100, 0) and (0, 100) mm are in the
# window. sheet_at x y: the screen position of a sheet point (mm).
sheet_at() {
  local view
  view=$(grep -o "Drawing sheet view [-0-9,]* [-0-9,]* [-0-9,]*" "$UI_LOG" | tail -1)
  [ -n "$view" ] || ui_fail "the app did not log the drawing's sheet"
  awk -v view="$view" -v x="$1" -v y="$2" -v wx="$X" -v wy="$Y" -v s="$UI_SCALE" 'BEGIN {
    split(view, part, " "); split(part[4], o, ","); split(part[5], a, ","); split(part[6], b, ",")
    px = o[1] + (a[1] - o[1]) * x / 100 + (b[1] - o[1]) * y / 100
    py = o[2] + (a[2] - o[2]) * x / 100 + (b[2] - o[2]) * y / 100
    printf "%d %d\n", wx + px * s + 0.5, wy + py * s + 0.5 }'
}
sheet_click() { ui_sync; xdotool mousemove $(sheet_at "$1" "$2") click "${3:-1}"; ui_sync; }

# settled "logged place": waits until the place logged last for it stays
# the same over a sync and a short wait (a panel just built is laid out
# again once).
settled() {
  local before after
  for _ in $(seq 1 20); do
    before=$(grep -F "$1 at " "$UI_LOG" | tail -1)
    ui_sync
    sleep 0.3
    after=$(grep -F "$1 at " "$UI_LOG" | tail -1)
    [ -n "$after" ] && [ "$before" = "$after" ] && return 0
  done
  ui_fail "the place of '$1' did not settle"
}

# field_text "Drawing panel <name>" text: a panel field's text replaced,
# then Enter.
field_text() {
  settled "$1"
  ui_click_logged "$1"
  ui_sync
  ui_key ctrl+a
  xdotool type --delay 40 -- "$2"
  ui_key Return
  ui_sync
}

# choose_parameter [filter]: in the Insert Parameter dialog just opened,
# the filter typed (none: the first parameter), then Insert (Enter).
choose_parameter() {
  ui_focus_dialog '^Insert Parameter$'
  [ -z "${1:-}" ] || xdotool type --delay 40 -- "$1"
  ui_sync
  ui_key Return
}

# scroll_to "Drawing panel <name>" "Drawing panel <over>": while the panel
# logs the control out of sight, the wheel turned down over the other one;
# done when its place logged last is in sight.
scroll_to() {
  local last
  for _ in $(seq 1 20); do
    ui_sync
    last=$(grep -F -e "$1 at " -e "$1 out of sight" "$UI_LOG" | tail -1)
    [[ $last == *" at "* ]] && return 0
    xdotool mousemove $(ui_logged_at "$2") click 5
    sleep 0.3
  done
  ui_fail "'$1' did not come into sight"
}

# no_refusal: no drawing command refused since the mark.
no_refusal() {
  grep -q "Drawing command refused" <(tail -n +$((UI_MARK + 1)) "$UI_LOG") && ui_fail "a drawing command was refused"
  return 0
}

ui_start_display
ui_start_app --open "$FILE" --no-native-dialogs
ui_expect_log "block.mitcad" "the design opened"

echo "--- Create Drawing with a front view"
ui_mark
ui_step "Create Drawing (search)"          ui_command "Create Drawing"
ui_expect_new "Create Drawing dialog opened" "its dialog asks for the sheet"
ui_focus_dialog '^Create Drawing$'
ui_step "Template: ISO (Alt+T, Home)"     ui_key alt+t Home
ui_step "OK"                               ui_key Return
ui_focus_main
ui_expect_new "Drawing opened: D1 Drawing1, sheet 1 of 1" "its sheet over the view"
ui_expect_new "Drawing sheet view " "the sheet placed"
ui_expect_new "Drawing tool: Base View" "Base View is on"
# The front view at 2:1 about 120, 120: the block's left side at x 80,
# from y 100 to 140.
ui_mark
ui_step "click the sheet at 120, 120"      sheet_click 120 120
ui_expect_new '"type":"base"' "the base view asked for"
ui_expect_new "Drawing view 1 at" "the front view drawn"
ui_expect_new "Drawing tool: Projected View" "projected views follow"
ui_step "end the tool (Esc)"               ui_key Escape
ui_expect_new "Drawing tool: selection" "no tool"
no_refusal

echo "--- Text: Insert Parameter by the Menu key and by a right-click"
ui_mark
settled "Ribbon DRAWING/TEXT"
ui_step "Text"                             ui_drawing_tool "Text" DRAWING/TEXT
ui_expect_new "Drawing tool: Text" "the text tool is on"
settled "Drawing panel drawingOption_text"
ui_step "its text"                         ui_click_logged "Drawing panel drawingOption_text"
xdotool type --delay 40 -- "L="
ui_expect_new "Drawing option text: L=" "L= typed"
ui_mark
ui_step "the field's menu (Menu key)"      ui_key Menu
ui_expect_new "Insert Parameter menu: drawingOption_text" "its menu with Insert Parameter"
ui_step "Insert Parameter (I)"             ui_key i
ui_expect_new "Insert Parameter dialog opened: " "the design's parameters listed, the user parameters first"
ui_step "Length, the first (Enter)"        choose_parameter
ui_expect_new "Insert Parameter dialog: Length" "Length chosen"
ui_focus_main
ui_expect_new "Parameter inserted in drawingOption_text: {param:Length}" "its reference at the cursor"
ui_expect_new "Drawing option text: L={param:Length}" "the tool's text has it"
xdotool type --delay 40 -- " BY "
ui_mark
ui_step "right-click the field"            ui_click_logged "Drawing panel drawingOption_text" 3
ui_expect_new "Insert Parameter menu: drawingOption_text" "its menu"
ui_step "Insert Parameter (I)"             ui_key i
ui_expect_new "Insert Parameter dialog opened: " "the parameters listed"
ui_step "filter Mak, Enter"                choose_parameter Mak
ui_expect_new "Insert Parameter dialog: Maker" "Maker chosen through the filter"
ui_focus_main
ui_expect_new "Drawing option text: L={param:Length} BY {param:Maker}" "both references in the text"
ui_mark
ui_step "where the text goes"              sheet_click 30 40
ui_expect_new '"type":"text"' "a text asked for"
ui_expect_new "1 = L=120 mm BY ACME" "the sheet shows both values"
no_refusal

echo "--- Note: a reference typed"
ui_mark
ui_step "Note"                             ui_drawing_tool "Note"
ui_expect_new "Drawing tool: Note" "the note tool is on"
settled "Drawing panel drawingOption_text"
ui_step "its text"                         ui_click_logged "Drawing panel drawingOption_text"
ui_key ctrl+a
xdotool type --delay 40 -- "MADE BY {param:Maker}"
ui_expect_new "Drawing option text: MADE BY {param:Maker}" "the reference typed"
ui_step "the front view's left side"       sheet_click 80.5 120
ui_expect_new "Drawing picked line (1)" "the side picked"
ui_step "where the text goes"              sheet_click 60 165
ui_expect_new '"type":"note"' "a note asked for"
ui_expect_new "2 = MADE BY ACME" "the note shows the text parameter"
no_refusal
ui_step "end the tool (Esc)"               ui_key Escape

echo "--- Edit Parameters: the text follows"
ui_mark
ui_step "Edit Parameters (search)"         ui_command "Edit Parameters"
ui_expect_new "Parameters Length expression at" "the parameters' table"
ui_focus_dialog '^Parameters$'
ui_double_click_logged "Parameters Length expression"
ui_sync
ui_key ctrl+a
xdotool type --delay 40 "150 mm"
ui_key Return
ui_expect_new "Parameter Length expression: 150 mm" "Length set to 150 mm"
ui_step "close the parameters (Esc)"       ui_key Escape
ui_focus_main
ui_expect_new "1 = L=150 mm BY ACME" "the text shows the new value"
no_refusal

echo "--- The Note dialog of the text: Insert Parameter, a missing parameter"
ui_double_click_expect "Drawing annotation 1" "Note dialog opened: Text 1" "a double-click opens the text"
ui_focus_dialog '^Note$'
ui_mark
ui_step "to its end (Ctrl+End)"            ui_key ctrl+End
ui_step "the text's menu (Menu key)"       ui_key Menu
ui_expect_new "Insert Parameter menu: noteText" "its menu with Insert Parameter"
ui_step "Insert Parameter (I)"             ui_key i
ui_expect_new "Insert Parameter dialog opened: " "the parameters listed"
ui_step "filter Mak, Enter"                choose_parameter Mak
ui_expect_new "Parameter inserted in noteText: {param:Maker}" "the reference in the note's text"
ui_focus_dialog '^Note$'
ui_step "a parameter it lacks"             xdotool type --delay 40 -- " {param:Finish}"
ui_expect_new "Missing parameters under noteMissingParameters: Finish" "the missing parameter named"
ui_step "cancel (Esc)"                     ui_key Escape
ui_focus_main

echo "--- Edit Title Block: a field of a parameter"
ui_mark
ui_step "right-click the empty sheet"      sheet_click 60 200 3
ui_step "choose Edit Title Block"          ui_menu_choose "Edit Title Block"
ui_expect_new "Title block editor: on, sheet 1: " "the editor on"
ui_expect_new "Drawing panel titleBlockTool_field at" "its tools in the panel"
ui_mark
settled "Drawing panel titleBlockTool_field"
ui_step "Field"                            ui_click_logged "Drawing panel titleBlockTool_field"
ui_expect_new "Title block tool: field" "the field tool"
ui_step "its cell's first corner"          sheet_click 240 15
ui_step "its cell's other corner"          sheet_click 290 20
ui_expect_new "Title block field added: " "the field's cell"
ui_expect_new "Drawing panel titleBlockFieldText at" "its text in the panel"
ui_step "Label: MAKER"                     field_text "Drawing panel titleBlockFieldLabel" "MAKER"
ui_mark
settled "Drawing panel titleBlockFieldText"
ui_step "its text, emptied"                ui_click_logged "Drawing panel titleBlockFieldText"
ui_key ctrl+a BackSpace
ui_step "right-click the text"             ui_click_logged "Drawing panel titleBlockFieldText" 3
ui_expect_new "Insert Parameter menu: titleBlockFieldText" "its menu with Insert Parameter"
ui_step "Insert Parameter (I)"             ui_key i
ui_step "filter Mak, Enter"                choose_parameter Mak
ui_focus_main
ui_expect_new "Parameter inserted in titleBlockFieldText: {param:Maker}" "the reference in the field"
ui_expect_new "label MAKER, text {param:Maker}," "the field's text"
ui_mark
ui_step "a parameter it lacks"             xdotool type --delay 40 -- " {param:Finish}"
ui_key Return
ui_expect_new "Missing parameters under titleBlockFieldMissing: Finish" "the missing parameter named under the field"
ui_expect_new "label MAKER, text {param:Maker} {param:Finish}," "the field's text with both"
# The warning under the field makes the panel higher than its room: Done
# may have gone out of sight below. The panel scrolled down (the wheel over
# the warning, which takes no wheel itself) until Done is logged in sight.
ui_step "scroll the panel to Done"         scroll_to "Drawing panel titleBlockDone" "Drawing panel titleBlockFieldMissing"
ui_mark
settled "Drawing panel titleBlockDone"
ui_step "Done"                             ui_click_logged "Drawing panel titleBlockDone"
ui_expect_new '"cmd":"edit_drawing_title_block"' "the block sent to the model"
ui_expect_new "Title block editor: done (" "the editor off"
ui_expect_new "MAKER=ACME {param:Finish}" "the title block shows the value and the missing reference as written"
no_refusal

[ -n "$SHOT" ] && ui_capture "$SHOT"
ui_crashed && ui_fail "crashed"
ui_finish "UI drawing parameters test"
