#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser app/print app/MainWindowContextMenu.cpp app/framework/TestDriver.cpp app/commands/MaterialCommands.cpp core/model tools/cli app/files/FileCommands.cpp app/framework/QuickAccessGroup.cpp
# Material ▸ of the context menus (mitcad#1046) through the real UI, on
# three boxes of the default material (Steel) in a design with a material
# of its own (Acme PETG) and a user library with Shop PETG:
#   - The view's menu on Body1: Material ▸ lists Steel (checked: what Body1
#     is made of) and Acme PETG, then More…; Acme PETG assigns it with its
#     appearance, one undo step; Ctrl+Z gives Body1 back its default.
#   - Material ▸ More… of Body3's Browser row opens the Material tool
#     (mitcad#1059); Shop PETG of My library given there is remembered, and
#     undone, so that the design has no copy of it.
#   - Body1 and Body2 picked in the Browser: Material ▸ offers Shop PETG
#     after a separator as a recent library material; one pick copies it
#     into the design and assigns it to both, one undo step.
#   - Body1's menu then checks Shop PETG; Body1 and Body3 together, of
#     different materials, check nothing.
#   - The root's Material ▸ sets its default material.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-material-menu-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
ROOT=$(cd "$(dirname "$0")/.." && pwd)
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-material-menu.XXXXXX")
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

# (Nothing to save after an undo back to the saved design: the file holds it.)
save() {
  ui_save_if_changed "$FILE"
}

# Three 40 x 20 x 20 boxes 10 mm apart and a material of the design's own.
cat > "$WORK/model.json" << 'EOF'
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [-45, -10],
    "length": 40, "width": 20, "height": 20, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [5, -10],
    "length": 40, "width": 20, "height": 20, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [55, -10],
    "length": 40, "width": 20, "height": 20, "operation": "new_body"}},
  {"cmd": "create_material", "id": "acme_petg", "name": "Acme PETG", "density": 1.27,
   "appearance": "plastic_orange", "filament": {"filament_type": "PETG", "temperature": 240}}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

ui_start_display
ui_start_app --open "$FILE" --no-native-dialogs
ui_step "fit (F6)"                         ui_key F6
ui_expect_log "Body F1.b0 at" "the boxes are in the view"
ui_expect_log "Browser Root/Bodies/Body3 at" "the browser lists the bodies"

echo "--- The view's menu: a design material, one undo step"
ui_mark
ui_step "right-click Body1 in the view"    ui_click_logged "Body F1.b0" 3
ui_expect_new "Context menu: " "the view's context menu opened"
ui_step "Material > Acme PETG"             ui_menu_choose "Material" "Acme PETG"
ui_expect_new "Context menu Material: Steel | Acme PETG | More…" "the design's materials, then More…"
ui_expect_new "Context menu Material checked: Steel" "Steel, what Body1 is made of, is checked"
ui_expect_new "Material assigned from menu: Acme PETG to 1 bodies" "Acme PETG assigned without a window"
if tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -q "Materials opened"; then
  ui_fail "the pick opened the Materials window"
fi
echo "ok   no window opened"
save
expect_file '.bodies[0] | [.material, .appearance]' '["acme_petg","plastic_orange"]' \
  "Body1 is Acme PETG and looks as it"
ui_mark
ui_step "undo"                             ui_key ctrl+z
ui_expect_new "Undo: Set Material of Body1" "one undo step"
save
expect_file '.bodies[0] | [.material, .appearance]' '[null,null]' "Body1 has its default material again"
ui_step "Escape (clear the selection)"     ui_key Escape

echo "--- More… opens the Material tool; a library material there"
ui_mark
ui_step "right-click Body3"                ui_click_logged "Browser Root/Bodies/Body3" 3
ui_step "Material > More..."               ui_menu_choose "Material" "More..."
ui_expect_new "Material tool for bodies F3.b0" "the Material tool for Body3"
if tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -q "Materials opened"; then
  ui_fail "More… opened the Materials window"
fi
echo "ok   no window opened"
ui_step "the picker: Shop PETG"            ui_picker_choose "Panel Material input material" "Material picker" "Shop PETG"
ui_mark
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Material shop_petg set for 1 body(ies), appearance replace" "Shop PETG set for Body3"
ui_mark
ui_step "undo"                             ui_key ctrl+z
ui_expect_new "Undo: Set Material of Body3" "undone, with its copy"
save
expect_file '[.materials[]?.name]' '["Acme PETG"]' "the design has no copy of Shop PETG"
ui_step "Escape (clear the selection)"     ui_key Escape

echo "--- Two bodies picked: a recent library material copied in and assigned to both"
ui_mark
ui_step "click Body1"                      ui_click_logged "Browser Root/Bodies/Body1"
ui_expect_new "Browser selected: Root/Bodies/Body1" "Body1 selected"
ui_step "Ctrl+click Body2"                 xdotool mousemove $(ui_logged_at "Browser Root/Bodies/Body2") keydown ctrl click 1 keyup ctrl
ui_expect_new "Browser selected: Root/Bodies/Body1, Root/Bodies/Body2" "Body1 and Body2 selected"
ui_mark
ui_step "right-click Body2"                ui_click_logged "Browser Root/Bodies/Body2" 3
ui_step "Material > Shop PETG"             ui_menu_choose "Material" "Shop PETG"
ui_expect_new "Context menu Material: Steel | Acme PETG | Shop PETG | More…" \
  "the design's materials, then the recent one, then More…"
ui_expect_new "Context menu Material recent: Shop PETG" "Shop PETG is offered as a recent library material"
ui_expect_new "Context menu Material checked: Steel" "both bodies are Steel: checked"
ui_expect_new "Material assigned from menu: Shop PETG to 2 bodies" "one pick assigns both"
save
expect_file '.bodies[0].material as $m | [.materials[] | select(.id == $m) | .name]' '["Shop PETG"]' \
  "Body1's is the design's copy of Shop PETG"
expect_file '(.bodies[0].material == .bodies[1].material) and (.bodies[2].material == null)' 'true' \
  "Body2 has the same, Body3 none"
ui_mark
ui_step "undo"                             ui_key ctrl+z
ui_expect_new "Undo: Material" "one undo step takes both back"
save
expect_file '[.bodies[0].material, .bodies[1].material, [.materials[]?.name]]' '[null,null,["Acme PETG"]]' \
  "both have their default again, the copy gone with them"
ui_mark
ui_step "redo (Ctrl+Y)"                    ui_key ctrl+y
save
expect_file '.bodies[1].material as $m | [.materials[] | select(.id == $m) | .name]' '["Shop PETG"]' \
  "redone: both are Shop PETG again"
ui_step "Escape (clear the selection)"     ui_key Escape

echo "--- The check mark: one material for all targets only"
ui_mark
ui_step "right-click Body1"                ui_click_logged "Browser Root/Bodies/Body1" 3
ui_step "open Material"                    ui_menu_choose "Material"
ui_expect_new "Context menu Material checked: Shop PETG" "Body1's Shop PETG is checked"
ui_step "Escape (close the submenu)"       ui_key Escape
ui_step "Escape (close the menu)"          ui_key Escape
ui_step "Escape (clear the selection)"     ui_key Escape
ui_mark
ui_step "click Body1"                      ui_click_logged "Browser Root/Bodies/Body1"
ui_expect_new "Browser selected: Root/Bodies/Body1" "Body1 selected"
ui_step "Ctrl+click Body3"                 xdotool mousemove $(ui_logged_at "Browser Root/Bodies/Body3") keydown ctrl click 1 keyup ctrl
ui_expect_new "Browser selected: Root/Bodies/Body1, Root/Bodies/Body3" "Body1 and Body3 selected"
ui_mark
ui_step "right-click Body3"                ui_click_logged "Browser Root/Bodies/Body3" 3
ui_step "open Material"                    ui_menu_choose "Material"
ui_expect_new "Context menu Material checked: none" "Shop PETG and Steel: nothing checked"
ui_step "Escape (close the submenu)"       ui_key Escape
ui_step "Escape (close the menu)"          ui_key Escape
ui_step "Escape (clear the selection)"     ui_key Escape

echo "--- The root's default material"
ui_mark
ui_step "right-click Root"                 ui_click_logged "Browser Root" 3
ui_step "Material > Acme PETG"             ui_menu_choose "Material" "Acme PETG"
ui_expect_new "Material assigned from menu: Acme PETG to component Root" "the root's default is Acme PETG"
save
expect_file '.component_materials' '[{"component":"C0","material":"acme_petg"}]' "the file keeps the root's default"

ui_finish "UI Material menu"
