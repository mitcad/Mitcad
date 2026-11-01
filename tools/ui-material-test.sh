#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser app/print app/MainWindowContextMenu.cpp app/commands/MaterialCommands.cpp core/model tools/cli
# Material ▸ More… (mitcad#640; the Material tool since mitcad#1059; the
# submenu Material, mitcad#1046, is tools/ui-material-menu-test.sh) through
# the real UI, on three boxes (F1.b0 with an appearance set by hand, F2.b0
# and F3.b0 with the default look) and a material of the design's own, each
# chosen in the Material tool's picker and given with OK:
#   - Body2's Material ▸ More… gives it PLA and PLA's appearance; Keep
#     appearances set by hand is not offered.
#   - Body1's with the design's Acme PETG offers Keep appearances set by
#     hand (Body1 is painted); with it on, the material is set and the paint
#     stays.
#   - Again with PLA and the check off: Body1 looks as PLA; undo takes it
#     back in one step.
#   - Body2 and Body3 selected in the browser (Ctrl+click), Appearance… and
#     Material ▸ More… of Body3's row are for both (mitcad#703): ABS is set
#     for the two bodies, Body1 keeps its material.
#   - Material ▸ More… of the root component sets its default material.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-material-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-material.XXXXXX)
FILE=$WORK/boxes.mitcad
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT

# expect_file filter expected description: a jq filter on the saved file.
expect_file() {
  local actual
  actual=$(jq -c "$1" "$FILE") || ui_fail "$3: cannot read $FILE"
  [ "$actual" = "$2" ] || ui_fail "$3: $1 is '$actual', expected '$2'"
  echo "ok   $3"
}

# expect_new_match regex description: ui_expect_new with an extended
# regular expression (two bodies in either order).
expect_new_match() {
  for _ in $(seq 1 30); do
    tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qE -- "$1" && { echo "ok   $2"; return; }
    sleep 0.2
  done
  ui_fail "$2: /$1/ not in the log since the mark"
}

save() {
  ui_mark
  ui_key ctrl+s
  ui_expect_new "Saved $FILE" "saved"
}

# material_of row name: Material ▸ More… of a browser row's context menu
# starts the Material tool; the material `name` chosen in its picker.
material_of() {
  ui_step "right-click $1"                 ui_click_logged "Browser $1" 3
  ui_step "choose Material > More..."      ui_menu_choose "Material" "More..."
  ui_expect_new "Panel Material input material at" "the Material tool is open"
  ui_step "the picker: $2"                 ui_picker_choose "Panel Material input material" "Material picker" "$2"
}

# Two 40 x 20 x 20 boxes 10 mm apart, Body1 painted red by hand, and a
# material of the design's own.
cat > "$WORK/model.json" << 'EOF'
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [-45, -10],
    "length": 40, "width": 20, "height": 20, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [5, -10],
    "length": 40, "width": 20, "height": 20, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [55, -10],
    "length": 40, "width": 20, "height": 20, "operation": "new_body"}},
  {"cmd": "set_body_appearance", "uid": "F1.b0", "appearance": "paint_red"},
  {"cmd": "create_material", "id": "acme_petg", "name": "Acme PETG", "density": 1.27,
   "appearance": "plastic_orange", "filament": {"filament_type": "PETG", "temperature": 240}}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

ui_start_display
ui_start_app --open "$FILE" --no-native-dialogs
ui_step "fit (F6)"                         ui_key F6
ui_expect_log "Body F2.b0 at" "the boxes are in the view"

echo "--- Body2: a built-in material and its appearance"
ui_mark
material_of "Root/Bodies/Body2" "PLA Plastic"
ui_expect_new "Panel Material input keep_appearance at" "the check's place is logged"
if tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep "Panel Material input keep_appearance at" | tail -1 | grep -qv "(hidden)"; then
  ui_fail "Keep appearances set by hand is offered for a body without a paint"
fi
echo "ok   no Keep appearances set by hand for Body2"
ui_mark
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Material pla set for 1 body(ies), appearance replace" "PLA set for Body2"
save
expect_file '.bodies[1] | [.material, .appearance]' '["pla","plastic_white"]' "Body2 is PLA and looks as PLA"
ui_step "Escape (clear the selection)"     ui_key Escape

echo "--- Body1: the design's material, the hand-set appearance kept"
ui_mark
material_of "Root/Bodies/Body1" "Acme PETG"
ui_expect_new "Panel Material keep_appearance note: Set by hand: Body1" "the panel names the painted body"
ui_step "Keep appearances set by hand"     ui_click_logged "Panel Material input keep_appearance"
ui_expect_new "Material: Keep appearances set by hand = on" "the check is on"
ui_mark
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Material acme_petg set for 1 body(ies), appearance keep" "Acme PETG set, the appearance kept"
save
expect_file '.bodies[0] | [.material, .appearance]' '["acme_petg","paint_red"]' "Body1 is Acme PETG and still red"
ui_step "Escape (clear the selection)"     ui_key Escape

echo "--- Body1: PLA, replacing the appearance; undo"
ui_mark
material_of "Root/Bodies/Body1" "PLA Plastic"
ui_expect_new "Panel Material keep_appearance note: Set by hand: Body1" "the painted body is named again"
ui_mark
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Material pla set for 1 body(ies), appearance replace" "PLA set, the appearance replaced"
save
expect_file '.bodies[0] | [.material, .appearance]' '["pla","plastic_white"]' "Body1 looks as PLA"
ui_mark
ui_step "undo"                             ui_key ctrl+z
ui_expect_new "Undo: Set Material of Body1" "one undo step"
save
expect_file '.bodies[0] | [.material, .appearance]' '["acme_petg","paint_red"]' "Body1 is back to Acme PETG, red"
ui_step "Escape (clear the selection)"     ui_key Escape

echo "--- Body2 and Body3 selected: Appearance... and Material... of a selected row are for both (mitcad#703)"
ui_mark
ui_step "click Body2"                      ui_click_logged "Browser Root/Bodies/Body2"
ui_expect_new "Browser selected: Root/Bodies/Body2" "Body2 selected"
ui_step "Ctrl+click Body3"                 xdotool mousemove $(ui_logged_at "Browser Root/Bodies/Body3") keydown ctrl click 1 keyup ctrl
ui_expect_new "Browser selected: Root/Bodies/Body2, Root/Bodies/Body3" "Body2 and Body3 selected"
ui_mark
ui_step "right-click Body3"                ui_click_logged "Browser Root/Bodies/Body3" 3
ui_step "choose Appearance..."             ui_menu_choose "Appearance..."
expect_new_match 'Appearance tool for (F2\.b0, F3\.b0|F3\.b0, F2\.b0)$' "the Appearance tool for both selected bodies"
ui_step "Cancel (Escape)"                  ui_key Escape
ui_mark
ui_step "click Body2"                      ui_click_logged "Browser Root/Bodies/Body2"
ui_step "Ctrl+click Body3"                 xdotool mousemove $(ui_logged_at "Browser Root/Bodies/Body3") keydown ctrl click 1 keyup ctrl
ui_expect_new "Browser selected: Root/Bodies/Body2, Root/Bodies/Body3" "Body2 and Body3 selected again"
ui_mark
material_of "Root/Bodies/Body3" "ABS Plastic"
expect_new_match 'Material tool for bodies (F2\.b0, F3\.b0|F3\.b0, F2\.b0)$' "the Material tool for both selected bodies"
expect_new_match 'Panel Material bodies note: (1 body: PLA Plastic, 1 body: Steel|1 body: Steel, 1 body: PLA Plastic)$' \
  "the panel notes what each is made of"
ui_mark
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Material abs set for 2 body(ies), appearance replace" "ABS set for Body2 and Body3"
save
expect_file '[.bodies[] | .material]' '["acme_petg","abs","abs"]' "Body2 and Body3 are ABS, Body1 is not"
ui_step "Escape (clear the selection)"     ui_key Escape

echo "--- The root's default material"
ui_mark
material_of "Root" "ABS Plastic"
ui_expect_new "Material tool for component C0" "the tool for the root"
ui_mark
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Default material of Root: abs" "the root's default is ABS"
save
expect_file '.component_materials' '[{"component":"C0","material":"abs"}]' "the file keeps the root's default"

ui_finish "UI Material tool from the menus"
