#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/drawings app/framework/Dialogs.cpp app/framework/DialogFolders.cpp core/model/src/drawing
# check-all sources: core/model-drawing/src core/model-api/src/drawings.rs core/model-api/src/drawing_templates.rs tools/cli
# The title block editor (mitcad#1625) through the real UI, in the Docked
# layout, on a 40 x 30 x 20 block that mitcad-cli makes; the user's library
# is the test's own folder (MITCAD_PRINT_LIBRARY_DIR):
#   - File › Create Drawing with the ISO template (A3, its block 180 x 36
#     in the frame's lower right corner, 230,10 to 410,46 on the sheet).
#   - Edit Title Block from the sheet's context menu: the editor on, its
#     Grid 5 mm; a line drawn (two clicks), a field added (two corners)
#     and its Label STOCK and Text {material} typed; Image… with a PNG
#     logo, placed by two corners; Done: edit_drawing_title_block, the
#     title block filled in with the field.
#   - A second sheet (+), then Save as Template… from the first sheet's
#     tab menu: the template saved to the user's library with its image.
#   - Create Drawing again with that template: the new drawing's title
#     block has the field.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-drawing-title-block-test.sh [screenshot.png]

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-drawing-title-block.XXXXXX)
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
R='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'
FILE="$WORK/block.mitcad"
export MITCAD_PRINT_LIBRARY_DIR=$WORK/library
mkdir -p "$MITCAD_PRINT_LIBRARY_DIR"

cat > "$WORK/block.json" << EOF
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 40, "height": 30},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$R"}],
    "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}}
]
EOF
"$CLI" run "$WORK/block.json" --save "$FILE" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }
# A logo: a PNG of one red pixel.
base64 -d > "$WORK/logo.png" <<< "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg=="

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

# panel_click "Drawing panel <name>": a click on a control of the panel once
# its place settled.
panel_click() {
  settled "$1"
  ui_click_logged "$1"
  ui_sync
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

# no_refusal: no drawing command refused since the mark.
no_refusal() {
  grep -q "Drawing command refused" <(tail -n +$((UI_MARK + 1)) "$UI_LOG") && ui_fail "a drawing command was refused"
  return 0
}

ui_start_display
ui_start_app --open "$FILE" --no-native-dialogs
ui_expect_log "block.mitcad" "the design opened"

echo "--- Create Drawing with the ISO template"
ui_mark
ui_step "Create Drawing (search)"          ui_command "Create Drawing"
ui_expect_new "Create Drawing dialog opened" "its dialog asks for the sheet"
ui_focus_dialog '^Create Drawing$'
ui_step "Template: ISO (Alt+T, Home)"     ui_key alt+t Home
ui_step "OK"                               ui_key Return
ui_focus_main
ui_expect_new "Create Drawing dialog: template iso, A3, first_angle" "the ISO template on its A3"
ui_expect_new "Drawing opened: D1 Drawing1, sheet 1 of 1" "its sheet over the view"
ui_expect_new "Drawing sheet view " "the sheet placed"
ui_expect_new "Drawing tool: Base View" "Base View is on"
ui_step "end the tool (Esc)"               ui_key Escape
ui_expect_new "Drawing tool: selection" "no tool"

echo "--- Edit Title Block"
ui_mark
ui_step "right-click the empty sheet"      sheet_click 60 200 3
ui_expect_new "Context menu: Sheet… | Edit Title Block | Add Revision… | Add Sheet" "the sheet's menu"
ui_step "choose Edit Title Block"          ui_menu_choose "Edit Title Block"
ui_expect_new "Title block editor: on, sheet 1: " "the editor on"
ui_expect_new "Drawing hint: Click a line, field or image to select it" "its hint"
ui_expect_new "Drawing panel titleBlockDone at" "Done in its panel"
ui_mark
settled "Drawing panel titleBlockGrid"
ui_step "Grid: 5 mm (three below 1 mm)"   ui_choose "Drawing panel titleBlockGrid" 3
ui_expect_new "Title block grid: 5 mm" "the grid of 5 mm"

echo "--- A line"
ui_mark
ui_step "Line"                             panel_click "Drawing panel titleBlockTool_line"
ui_expect_new "Title block tool: line" "the line tool"
ui_step "its first end"                    sheet_click 300 30
ui_expect_new "Title block first point 70,20" "its first end on the grid"
ui_step "its second end"                   sheet_click 330 30
ui_expect_new "Title block line added: 70,20 100,20" "the line in the block"

echo "--- A field of the material"
ui_mark
ui_step "Field"                            panel_click "Drawing panel titleBlockTool_field"
ui_expect_new "Title block tool: field" "the field tool"
ui_step "its cell's first corner"          sheet_click 235 40
ui_step "its cell's other corner"          sheet_click 285 45
ui_expect_new "Title block field added: 5,30 50x5" "the field's cell"
ui_expect_new "Drawing panel titleBlockFieldText at" "its text in the panel"
ui_step "Label: STOCK"                     field_text "Drawing panel titleBlockFieldLabel" "STOCK"
ui_step "Text: {material}"                 field_text "Drawing panel titleBlockFieldText" "{material}"
ui_expect_new "label STOCK, text {material}, height 3.5" "the field's label and text"

echo "--- A PNG logo"
ui_mark
ui_step "Image…"                           panel_click "Drawing panel titleBlockTool_image"
ui_step "the logo's path, Enter"           ui_type_path "Title Block Image" "$WORK/logo.png"
ui_focus_main
ui_expect_new "Title block image chosen: logo.png (png)" "the logo chosen"
ui_step "its box's first corner"           sheet_click 390 15
ui_step "its box's other corner"           sheet_click 405 40
ui_expect_new "Title block image added: png 160,5 15x25" "the logo placed"

echo "--- Done"
ui_mark
ui_step "Done"                             panel_click "Drawing panel titleBlockDone"
ui_expect_new '"cmd":"edit_drawing_title_block"' "the block sent to the model"
ui_expect_new "Title block editor: done (" "the editor off"
ui_expect_new "Drawing title block (iso): " "the title block drawn again"
grep -F "Drawing title block (iso): " "$UI_LOG" | tail -1 | grep -qF "STOCK=" ||
  ui_fail "the title block has no STOCK field: $(grep -F "Drawing title block (iso): " "$UI_LOG" | tail -1)"
echo "ok   the title block has the field"
no_refusal

echo "--- Save as Template"
ui_mark
ui_step "+ (a second sheet)"               ui_click_logged "Drawing Add Sheet"
ui_expect_new "Drawing Sheet tab 2 at" "the second sheet's tab"
settled "Drawing Sheet tab 1"
ui_step "the first tab's menu"             ui_click_logged "Drawing Sheet tab 1" 3
ui_expect_new "Drawing sheet menu: sheet 1" "the tab's menu"
ui_step "Save as Template… (4 x Down)"    ui_key Down Down Down Down Return
ui_expect_new "Drawing save template dialog opened: sheet 1" "its dialog"
ui_focus_dialog '^Save as Template$'
ui_step "Name: Company Logo"               xdotool type --delay 40 -- "Company Logo"
ui_step "OK (the User library)"            ui_key Return
ui_focus_main
ui_expect_new "Drawing template saved: user:company-logo" "saved to the user's library"
SAVED=$MITCAD_PRINT_LIBRARY_DIR/drawing-templates/company-logo.json
[ -f "$SAVED" ] || ui_fail "no $SAVED"
grep -q '"images"' "$SAVED" || ui_fail "the saved template has no image"
grep -q '"STOCK"' "$SAVED" || ui_fail "the saved template has no STOCK field"
echo "ok   the template file has the field and the logo"
no_refusal

echo "--- A new drawing with the template"
ui_mark
ui_step "Create Drawing (search)"          ui_command "Create Drawing"
ui_expect_new "Create Drawing dialog opened" "its dialog"
ui_focus_dialog '^Create Drawing$'
ui_step "Template: Company Logo (Alt+T, Home, Down, Down)" ui_key alt+t Home Down Down
ui_step "OK"                               ui_key Return
ui_focus_main
ui_expect_new "Create Drawing dialog: template user:company-logo, A3, first_angle" "the user's template"
ui_expect_new "Drawing created: D2 Drawing2 in C0" "the second drawing"
ui_expect_new "Drawing title block (user:company-logo): " "its title block from the template"
grep -F "Drawing title block (user:company-logo): " "$UI_LOG" | tail -1 | grep -qF "STOCK=" ||
  ui_fail "the new drawing's title block has no STOCK field"
echo "ok   the new drawing's title block has the field"
no_refusal

[ -n "$SHOT" ] && ui_capture "$SHOT"
ui_crashed && ui_fail "crashed"
ui_finish "UI drawing title block test"
