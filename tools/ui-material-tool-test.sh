#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/MaterialCommands.cpp app/commands/MaterialBinding.cpp app/browser/LibraryPicker.cpp app/browser/BrowserController.cpp app/browser/BrowserMaterialMenu.cpp app/framework/CommandPanel.cpp app/framework/CommandSession.cpp app/files/MainWindowPrint.cpp app/print core/model tools/cli
# The Material and Appearance tools (mitcad#1059) through the real UI, on
# three boxes of the default material (Steel), a material of the design's
# own (Acme PETG), a user library with Shop PETG, the text parameter Color
# ('Black', allowed values 'Black' and 'Red') and Body3's material bound to
# Color before the tool existed (mitcad-cli: as the windows of mitcad#861
# wrote it):
#   - Body1 and Body2 picked in the Browser, Material (search): the panel
#     notes "2 bodies: Steel"; the picker button opens the picker, Shop
#     PETG of My library is chosen by its search; OK copies it into the
#     design and gives both bodies it, one undo step; Ctrl+Z restores both
#     and takes the copy away.
#   - Body3's Material ▸ More… opens the Material tool with the binding made
#     earlier: the fx on, Varies With Color, the rows Black (now) and Red.
#   - Body1's Material ▸ More…, the fx on, Varies With Color, ABS Plastic for
#     Black and PLA Plastic for Red: OK binds Body1's material to Color;
#     Color = 'Red' in Edit Parameters makes Body1 PLA.
#   - The Appearance tool: Body2's Appearance… with Paint - Red chosen in the
#     appearance picker; then Body2's top face in Paint - Black through the
#     Faces input, and Default for that face takes its own away again.
#   - The root's Material ▸ More…: the picked component's default material
#     (Acme PETG).
#   - The windows: Edit Materials and Edit Appearances open with no Assign
#     and no assignment panel.
#   - Edit Materials… last in the Material tool's picker (mitcad#1415):
#     Body2 picked, Shop PETG chosen, Edit Materials… (typing Edit, Enter)
#     opens the Materials manager on Shop PETG while the panel stays open;
#     its copy in My library, renamed Bench PETG, is offered once the
#     manager closes, and OK gives it to Body2, still picked, in one undo
#     step.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-material-tool-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
ROOT=$(cd "$(dirname "$0")/.." && pwd)
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-material-tool.XXXXXX")
FILE=$WORK/boxes.mitcad
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT

# My library with one material of its own (an item file as Save to writes
# it).
export MITCAD_PRINT_LIBRARY_DIR=$WORK/print-library
mkdir -p "$MITCAD_PRINT_LIBRARY_DIR/materials"
cat > "$MITCAD_PRINT_LIBRARY_DIR/materials/shop_petg.json" << 'EOF'
{
  "format": "mitcad-print-item",
  "version": 1,
  "material": {
    "id": "shop_petg",
    "name": "Shop PETG",
    "density": 1.27,
    "appearance": "plastic_black",
    "filament": {"filament_type": "PETG", "temperature": 240},
    "version": 1
  }
}
EOF

# expect_file filter expected description: a jq filter on the saved file.
expect_file() {
  local actual
  actual=$(jq -c "$1" "$FILE") || ui_fail "$3: cannot read $FILE"
  [ "$actual" = "$2" ] || ui_fail "$3: $1 is '$actual', expected '$2'"
  echo "ok   $3"
}

save() {
  ui_mark
  ui_key ctrl+s
  ui_expect_new "Saved $FILE" "saved"
}

# not_since text description: the text is not in the log since the mark.
not_since() {
  if tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qF -- "$1"; then
    ui_fail "$2: '$1' is in the log"
  fi
  echo "ok   $2"
}

# picker_edit button picker entry: the picker opened from its button (after
# a ui_mark), its last entry (mitcad#1415) found by typing Edit and chosen
# with Return, as the keyboard reaches the other entries.
picker_edit() {
  ui_click_logged "$1"
  ui_expect_new "$2 opened: " "the $2 opened"
  ui_focus_dialog "^${2% picker} Picker\$"
  xdotool type --delay 40 -- "Edit"
  ui_expect_new "$2 search: Edit, first $3" "typing Edit finds $3"
  ui_key Return
  ui_expect_new "$2 chose: $3" "$3 chosen in the $2"
}

# set_text name text: the parameter's expression in Edit Parameters, the
# table closed again.
set_text() {
  ui_mark
  ui_step "Edit Parameters (search)"     ui_command "Edit Parameters"
  ui_expect_new "Parameters dialog opened" "the table opened"
  ui_expect_log "Parameters $1 expression at" "$1's cells are logged"
  ui_focus_dialog '^Parameters$'
  ui_double_click_logged "Parameters $1 expression"
  ui_sync
  ui_key ctrl+a
  xdotool type --delay 40 "'$2'"
  ui_key Return
  ui_expect_new "Parameter $1 expression: '$2' = " "$1 = '$2'"
  ui_step "OK (the table)"                 ui_click_logged "Parameters OK"
  ui_focus_main
}

BOUND="switch(Color; 'Black'; 'acme_petg'; 'Red'; 'pla')"

# Three 40 x 20 x 20 boxes 10 mm apart, a material of the design's own, the
# option Color and Body3's material bound to it.
cat > "$WORK/model.json" << 'EOF'
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [-45, -10],
    "length": 40, "width": 20, "height": 20, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [5, -10],
    "length": 40, "width": 20, "height": 20, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [55, -10],
    "length": 40, "width": 20, "height": 20, "operation": "new_body"}},
  {"cmd": "create_material", "id": "acme_petg", "name": "Acme PETG", "density": 1.27,
   "appearance": "plastic_orange", "filament": {"filament_type": "PETG", "temperature": 240}},
  {"cmd": "add_parameter", "name": "Color", "expression": "'Black'", "values": ["'Black'", "'Red'"]},
  {"cmd": "set_body_material", "uid": "F3.b0",
   "expression": "switch(Color; 'Black'; 'acme_petg'; 'Red'; 'pla')"}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

ui_start_display
ui_start_app --open "$FILE" --no-native-dialogs
ui_step "fit (F6)"                         ui_key F6
ui_expect_log "Body F1.b0 at" "the boxes are in the view"
ui_expect_log "Browser Root/Bodies/Body3 at" "the browser lists the bodies"

echo "--- A library material and two bodies, OK; Ctrl+Z restores both"
ui_mark
ui_step "click Body1"                      ui_click_logged "Browser Root/Bodies/Body1"
ui_expect_new "Browser selected: Root/Bodies/Body1" "Body1 selected"
ui_step "Ctrl+click Body2"                 xdotool mousemove $(ui_logged_at "Browser Root/Bodies/Body2") keydown ctrl click 1 keyup ctrl
ui_expect_new "Browser selected: Root/Bodies/Body1, Root/Bodies/Body2" "Body1 and Body2 selected"
ui_mark
ui_step "Material (search)"                ui_command "Material"
ui_expect_new "Command search chose solid.material" "the search found the Material tool"
ui_expect_new "Panel Material bodies note: 2 bodies: Steel" "the panel notes what the bodies are made of"
ui_expect_new "Panel Material tile material: Default (Steel) (No material of its own)" \
  "the picker button shows what both have"
ui_expect_new "Panel Material input material at" "the picker button's place is logged"
ui_step "the picker: Shop PETG"            ui_picker_choose "Panel Material input material" "Material picker" "Shop PETG"
ui_expect_new "Panel Material tile material: Shop PETG (My library)" "the picker button shows Shop PETG"
ui_mark
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Material tool: Shop PETG/" "the tool's log line"
ui_expect_new " to 2 bodies" "for both bodies"
save
expect_file '.bodies[0].material as $m | [.materials[] | select(.id == $m) | .name]' '["Shop PETG"]' \
  "Body1 is the design's copy of Shop PETG"
expect_file '(.bodies[0].material == .bodies[1].material) and (.bodies[2].material != .bodies[0].material)' 'true' \
  "Body2 has the same, Body3 not"
ui_mark
ui_step "undo"                             ui_key ctrl+z
ui_expect_new "Undo: Material" "one undo step"
save
expect_file '[.bodies[0].material, .bodies[1].material, [.materials[]?.name]]' '[null,null,["Acme PETG"]]' \
  "both have their default again, the copy gone with them"
ui_step "Escape (clear the selection)"     ui_key Escape

echo "--- A binding made earlier shows in the tool"
ui_mark
ui_step "right-click Body3"                ui_click_logged "Browser Root/Bodies/Body3" 3
ui_step "Material > More..."               ui_menu_choose "Material" "More..."
ui_expect_new "Material tool for bodies F3.b0" "More… opens the Material tool"
not_since "Materials opened" "no window opened"
ui_expect_new "Material tool: the targets follow $BOUND" "the tool reads Body3's binding"
ui_expect_new "Panel Material label row0: Black (now)" "the row Black, the value now"
ui_expect_new "Panel Material label row1: Red" "the row Red"
ui_expect_new "Panel Material bodies note: 1 body: varies with Color" "the panel notes the binding"
ui_step "Cancel (Escape)"                  ui_key Escape
ui_step "Escape (clear the selection)"     ui_key Escape

echo "--- The fx: Body1's material varies with Color"
ui_mark
ui_step "right-click Body1"                ui_click_logged "Browser Root/Bodies/Body1" 3
ui_step "Material > More..."               ui_menu_choose "Material" "More..."
ui_expect_new "Material tool for bodies F1.b0" "the Material tool for Body1"
ui_expect_new "Panel Material input material_fx at" "the fx button is logged"
ui_step "fx"                               ui_click_logged "Panel Material input material_fx"
ui_expect_new "Material: Varies with an option = on" "the fx is on"
ui_expect_new "Panel Material input varies at" "Varies With shows"
ui_mark
ui_step "Varies With: Color"               ui_choose "Panel Material input varies" 1
ui_expect_new "Material tool varies with Color: 2 rows" "a row per value of Color"
ui_expect_new "Panel Material label row0: Black (now)" "the row Black"
ui_expect_new "Panel Material input row1 at" "the row Red is logged"
ui_mark
ui_step "Black: ABS Plastic"               ui_choose_text "Panel Material input row0" "ABS"
ui_expect_new " = ABS Plastic" "ABS Plastic for Black"
ui_mark
ui_step "Red: PLA Plastic"                 ui_choose_text "Panel Material input row1" "PLA P"
ui_expect_new " = PLA Plastic" "PLA Plastic for Red"
ui_mark
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Material tool: varies with Color to 1 bodies: switch(Color; 'Black'; 'abs'; 'Red'; 'pla'; 'steel')" \
  "Body1 bound to Color, Steel for any other value"
save
expect_file '.bodies[0] | [.material, .material_expr]' \
  "[\"abs\",\"switch(Color; 'Black'; 'abs'; 'Red'; 'pla'; 'steel')\"]" "Body1 is ABS and follows Color"
set_text Color Red
save
expect_file '.bodies[0].material' '"pla"' "Color = 'Red' makes Body1 PLA"
ui_step "Escape (clear the selection)"     ui_key Escape

echo "--- The Appearance tool: the picker, and a face"
ui_mark
ui_step "right-click Body2"                ui_click_logged "Browser Root/Bodies/Body2" 3
ui_step "choose Appearance..."             ui_menu_choose "Appearance..."
ui_expect_new "Appearance tool for F2.b0" "Appearance… opens the Appearance tool"
not_since "Appearances dialog opened" "no window opened"
ui_expect_new "Panel Appearance input value at" "the picker button's place is logged"
ui_step "the picker: Paint - Red"          ui_picker_choose "Panel Appearance input value" "Appearance picker" "Paint - Red"
ui_expect_new "Panel Appearance tile value: Paint - Red (Library)" "the picker button shows Paint - Red"
ui_mark
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Appearance paint_red for 1 body(ies)" "Body2 is red"
ui_step "Escape (clear the selection)"     ui_key Escape
ui_mark
ui_step "Appearance (search)"              ui_command "Appearance"
ui_expect_new "Command search chose solid.appearance" "the Appearance tool"
ui_step "the Faces input"                  ui_click_logged "Panel Appearance input faces"
ui_step "pick Body2's top face"            ui_click_pick "face F2.b0/F2:top"
ui_step "the picker: Paint - Black"        ui_picker_choose "Panel Appearance input value" "Appearance picker" "Paint - Black"
ui_mark
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Appearance paint_black for 0 body(ies), 1 face(s)" "the top face is black"
save
expect_file '[.bodies[1].appearance, (.bodies[1].face_appearances | length)]' '["paint_red",1]' \
  "Body2 red, its top face its own"
ui_step "Escape (clear the selection)"     ui_key Escape
ui_mark
ui_step "Appearance (search)"              ui_command "Appearance"
ui_step "the Faces input"                  ui_click_logged "Panel Appearance input faces"
ui_step "pick the top face again"          ui_click_pick "face F2.b0/F2:top"
ui_step "the picker: Default"              ui_picker_choose "Panel Appearance input value" "Appearance picker" "Default"
ui_mark
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Appearance  for 0 body(ies), 1 face(s)" "Default for the face"
save
expect_file '(.bodies[1].face_appearances // []) | length' '0' "the face shows its body's again"
ui_step "Escape (clear the selection)"     ui_key Escape

echo "--- The root's default material"
ui_mark
ui_step "right-click Root"                 ui_click_logged "Browser Root" 3
ui_step "Material > More..."               ui_menu_choose "Material" "More..."
ui_expect_new "Material tool for component C0" "the Material tool for the root"
ui_expect_new "Panel Material bodies note: 1 component: Default" "the panel notes the component"
ui_step "the picker: Acme PETG"            ui_picker_choose "Panel Material input material" "Material picker" "Acme PETG"
ui_mark
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Default material of Root: acme_petg" "the root's default is Acme PETG"
save
expect_file '.component_materials' '[{"component":"C0","material":"acme_petg"}]' "the file keeps the root's default"
ui_step "Escape (clear the selection)"     ui_key Escape

echo "--- The windows manage only"
ui_mark
ui_step "Edit Materials (search)"          ui_command "Edit Materials"
ui_expect_new "Materials opened" "the Materials window opened"
ui_expect_new "Materials new at" "its New button is logged"
not_since "Materials assign" "the Materials window has no Assign"
not_since "Materials varies" "the Materials window has no assignment panel"
ui_focus_dialog '^Materials$'
ui_step "Close"                            ui_click_logged "Materials close"
ui_focus_main
ui_mark
ui_step "Edit Appearances (search)"        ui_command "Edit Appearances"
ui_expect_new "Appearances dialog opened" "the Appearances window opened"
ui_expect_new "Appearances close at" "its Close button is logged"
not_since "Appearances assign" "the Appearances window has no Assign"
not_since "Appearances varies" "the Appearances window has no assignment panel"
ui_focus_dialog '^Appearances$'
ui_step "Close"                            ui_click_logged "Appearances close"
ui_focus_main

echo "--- Edit Materials… at the end of the picker (mitcad#1415): a material made there is offered"
ui_mark
ui_step "click Body2"                      ui_click_logged "Browser Root/Bodies/Body2"
ui_expect_new "Browser selected: Root/Bodies/Body2" "Body2 selected"
ui_mark
ui_step "Material (search)"                ui_command "Material"
ui_expect_new "Command search chose solid.material" "the Material tool"
ui_expect_new "Panel Material input material at" "the picker button's place is logged"
ui_step "the picker: Shop PETG"            ui_picker_choose "Panel Material input material" "Material picker" "Shop PETG"
ui_mark
ui_step "the picker: Edit Materials…"      picker_edit "Panel Material input material" "Material picker" "Edit Materials…"
ui_expect_new "Material tool: Edit Materials… on Shop PETG" "the tool opens the manager on its material"
ui_expect_new "Materials opened" "the Materials manager opened"
ui_expect_new "Material selected: Shop PETG (user)" "on Shop PETG"
ui_expect_new "Materials duplicate at" "Duplicate under the list"
ui_focus_dialog '^Materials$'
ui_mark
ui_step "Duplicate"                        ui_click_logged "Materials duplicate"
ui_expect_new "Materials duplicate menu: This design | My library" "Duplicate offers This design first while a design is open"
# The menu opens with nothing active: Down takes This design, Down again My library.
ui_key Down Down Return
ui_expect_new "Material duplicated: Shop PETG as " "a copy made while the tool's panel is open"
ui_expect_new " in user" "into My library"
ui_sync
ui_mark
xdotool type --delay 40 "Bench PETG"
ui_key Return
ui_expect_new "Material Shop PETG (copy) (user) name = Bench PETG" "Enter renamed it"
ui_mark
ui_step "close (Esc)"                      ui_key Escape
ui_expect_new "Materials closed" "closed"
ui_expect_new "Material tool: materials read again, " "the tool read its materials again"
ui_expect_new ", new: Bench PETG" "Bench PETG is offered"
ui_focus_main
ui_step "the picker: Bench PETG"           ui_picker_choose "Panel Material input material" "Material picker" "Bench PETG"
ui_expect_new "Panel Material tile material: Bench PETG (My library)" "the picker button shows Bench PETG"
ui_mark
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Material tool: Bench PETG/" "the tool's log line"
ui_expect_new " to 1 bodies" "Body2, still picked, gets Bench PETG"
save
expect_file '.bodies[1].material as $m | [.materials[] | select(.id == $m) | .name]' '["Bench PETG"]' \
  "Body2 is the design's copy of Bench PETG"
ui_mark
ui_step "undo"                             ui_key ctrl+z
ui_expect_new "Undo: Set Material of Body2" "one undo step"
save
expect_file '[.bodies[1].material, any(.materials[]?; .name == "Bench PETG")]' '[null,false]' \
  "Body2 has its default again, the copy gone with it"
ui_step "Escape (clear the selection)"     ui_key Escape

ui_finish "UI Material and Appearance tools"
